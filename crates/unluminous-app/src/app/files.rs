//! The files that are open, which pane each is in, and which of them is showing.
//!
//! Everything that belongs to one file rather than to the window lives in an [`OpenFile`]: the
//! document, how far it is scrolled, which of the three ways of looking at it is showing, what git
//! has to say about it, and — since `task-1664` — what has been laid out for it and which pane it
//! is in. The window keeps one of these for each tab.
//!
//! There is always at least one. A window that has just started holds one tab with an untitled
//! document in it, and closing the last tab leaves another untitled one rather than a window with no
//! document and a special case everywhere for it.
//!
//! ## The transient tab
//!
//! At most one tab in each pane is transient, and it is the one a single click in the explorer
//! reuses. Clicking a second file replaces its contents instead of adding a tab, so reading through
//! a folder does not leave thirty tabs behind. Double clicking a file, or typing into the transient
//! tab, makes it permanent — editing a file you were only glancing at plainly means you meant to
//! open it. This is what the reference editor does, and it is what `task-1649` describes when it says a double
//! click opens a file in a new tab.
//!
//! ## A tab holding a picture
//!
//! `task-1658` asks to be able to look at an image, so a tab holds either text or a picture. It still
//! holds a `Document` either way — one made by `Document::at_path`, which carries the path and nothing
//! else — so the tab is named after the file, the explorer marks the row as open and the tab strip needs
//! no second kind of tab. What tells them apart is [`OpenFile::picture`], and the window asks that one
//! question in the two places it matters: what to draw in the editing area, and what to refuse to save.

use std::path::{Path, PathBuf};

use crate::services::project_state::DiskStamp;
use unluminous_core::{Anchor, Document, Layout, Preview, Selection};

use crate::app::{PlacedDiagram, PlacedPicture, ViewMode};
use crate::components::gutter::{BlameRow, Change};
use crate::services::picture::Picture;

/// What has been worked out about one file and kept between frames: the laid out text, and the
/// Markdown preview with its pictures and diagrams.
///
/// These were ten fields on `UnluminousApp` until `task-1664`, because there was one editing area and so
/// one file being drawn. With panes there are several, at several widths, and a single set of them
/// is not slow so much as **wrong** in the way a cache is wrong: the first pane lays its file out,
/// the second lays its own over the top, and the next frame does it again, so a large file is laid
/// out from scratch twice a frame for ever.
///
/// Keyed by the thing they describe, they are correct without anybody thinking about it: each pane's
/// width is stable from frame to frame, so nothing is laid out that has not changed. It also all but
/// removes `stale`, which existed because the revision counts changes to *one* document and two
/// documents could be at the same number — a shared cache confusing two files. A cache on the file
/// cannot confuse two files, so what is left of the flag is the one case where a tab's document is
/// **replaced** in place and the cache belongs to the document that has gone.
#[derive(Default)]
pub struct Cached {
    /// The text as it was last laid out.
    pub layout: Layout,
    pub laid_out_revision: u64,
    pub laid_out_width: f32,
    /// Set when the layout has to be worked out again whatever the revision says.
    pub stale: bool,
    /// The Markdown preview, worked out from the source and kept until the source changes.
    pub preview: Option<Preview>,
    pub preview_layout: Layout,
    pub preview_revision: u64,
    pub preview_width: f32,
    /// Where each of the preview's pictures is drawn, worked out with the preview and drawn from
    /// every frame.
    pub preview_pictures: Vec<PlacedPicture>,
    /// The diagrams in the preview, in the order they appear.
    pub preview_diagrams: Vec<PlacedDiagram>,
    /// What this file's live text defines and where its words are, worked out from it and kept
    /// until the text changes.
    ///
    /// The ownership rule `task-1675` follows: **a file that is open is owned by its `Document`**,
    /// so the project's index deliberately holds nothing for it and this is the answer instead.
    /// Keyed on `text_revision`, the same key `colour_the_file` is keyed on, so a caret move
    /// recomputes nothing.
    pub symbols: Option<crate::app::symbols::TabSymbols>,
    /// What in this file could be collapsed, read from its live text and kept until that text
    /// changes. Keyed on `text_revision`, the same key the two above are keyed on.
    ///
    /// Which of them *are* collapsed is not here: that is state rather than a reading, so it lives
    /// in the `Document` where the two functions that move bytes can move it.
    pub fold_regions: Option<crate::app::folding::TabRegions>,
    /// This file's comments and strings, and the `text_revision` they were read at.
    ///
    /// A by-product of `colour_the_file`, which already runs `syntax::scan` over the same text at
    /// the same revision. Reading the blocks that could be collapsed needs exactly that and nothing
    /// else from a tokeniser, and a second pass over a 273 kilobyte file is 2.5 ms of every
    /// keystroke — `task-1666`'s rule, applied to the pass `task-1686` would otherwise have added.
    pub fold_tokens: Option<(u64, unluminous_core::folding::Tokens)>,
    /// The `fold_revision` the layout was built at, beside the text revision it was built at.
    ///
    /// A second key rather than folding into the first, because collapsing a block changes the
    /// layout and nothing else: keyed on `text_revision` a fold would re-colour the file and rebuild
    /// the Markdown preview. See `tasks/task-1686-folding-tdd.md` section 5.1.
    pub laid_out_folds: u64,
}

impl Cached {
    /// Nothing has been worked out yet, so everything has to be.
    fn fresh() -> Self {
        Self { stale: true, ..Self::default() }
    }

    /// Releases the growth headroom of a completed layout once its tab has been put away.
    ///
    /// A visible file must never pause for this, and an edited one must keep its editing headroom,
    /// so it is asked for at exactly two points: the first full layout of a file, and the moment a
    /// tab is displaced by another. Both are off the input path.
    fn compact_layouts(&mut self) {
        self.layout.compact_capacity();
        self.preview_layout.compact_capacity();
    }
}

/// A place in a view that is to stay where it is while the text is laid out again.
///
/// Zooming changes how tall every line is, so a scroll position — a number of points down the
/// document — means something different afterwards, and the reader is left looking at a different
/// part of the file. What is remembered instead is the text that was under a point on the screen
/// and how far down the view that point was; putting the two back together once the new layout
/// exists gives the scroll position that leaves that text exactly where it was.
///
/// Taken before the font size changes, because it has to describe the layout the reader can still
/// see, and used up on the first frame the file is laid out again — which for a file in a pane that
/// is not on the screen may be a good while later, and is still the right answer, because a file
/// that has not been laid out again has not moved.
#[derive(Debug, Clone, Copy)]
pub struct ViewAnchor {
    /// The text that is to stay put.
    pub at: Anchor,
    /// How far below the top of the view it was, in points.
    pub above: f32,
}

/// One open file, and everything about it that is not about the window.
/// A tab in the editing area that a plugin draws.
///
/// It has no path on disk, is never modified and cannot be saved. Its label comes from the manifest, and
/// `key` is the `<plugin id>/<tab id>` the command line and the project's `.unluminous` folder name it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginTab {
    pub key: String,
    pub plugin: String,
    pub label: String,
}

/// Where a tab lives.
///
/// The editing area is a row of panes and a tab is in one of them, which is what `task-1664` settled
/// and what [`OpenFiles`] is arranged around. `task-1904` adds a second kind of place: a **File Editor
/// node** on the Base of Infinite Space is a tab too, with its own document, its own gutter, its own
/// folds and breakpoints and its own place in the undo history.
///
/// It is one value with two cases rather than a pane number and an optional node id, because a tab is
/// in exactly one place and two fields that have to agree are two fields that can stop agreeing. The
/// invariants [`OpenFiles`] keeps - panes numbered without gaps, no pane empty - are about the panes,
/// so a tab living on a node is simply not counted by either of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Home {
    /// In the editing area, in this pane, counting from the left.
    Pane(usize),
    /// On the canvas, in the File Editor node with this id.
    Node(u64),
}

impl Default for Home {
    fn default() -> Self {
        Home::Pane(0)
    }
}

impl Home {
    /// Which pane, when it is in one at all.
    pub fn pane(self) -> Option<usize> {
        match self {
            Home::Pane(pane) => Some(pane),
            Home::Node(_) => None,
        }
    }

    /// Which node, when it is on one.
    pub fn node(self) -> Option<u64> {
        match self {
            Home::Node(node) => Some(node),
            Home::Pane(_) => None,
        }
    }
}

pub struct OpenFile {
    pub document: Document,
    /// Rendered web content, when this tab belongs to the embedded browser rather than the editor.
    pub browser: Option<crate::services::browser::BrowserTab>,
    /// What is in the browser toolbar's address field, for a tab that holds a page.
    ///
    /// On the tab rather than in `egui`'s memory, which is the same reason `space::node::Browser` keeps
    /// its own: a tab that is not showing is not drawn, and a half-typed address must not go with it.
    /// Never written to `open-files.txt` — what a project comes back with is the page, not what somebody
    /// was part way through typing. `task-1905`.
    pub typed_address: String,
    /// Whether what is in the address bar is the person's rather than the page's. See
    /// `components::browser_view::Toolbar::editing`.
    pub editing_address: bool,
    /// What size this tab's document was last given, when something other than the window's own setting
    /// gave it one.
    ///
    /// **On the tab rather than on the node**, which is the fault the Codex Sol review of `task-1905` found:
    /// keyed on the node, a second tab shown in the same node was never restyled — the node's cache already
    /// held the wanted size — and a tab dragged back into a pane kept the node's size for ever. It is the
    /// document that carries a base style, so it is the document that has to remember what it was given.
    /// `None` for a tab nothing has resized, which is what a pane's tab always is.
    pub sized_at: Option<f32>,
    pub view_mode: ViewMode,
    /// How far the source is scrolled.
    pub scroll: f32,
    /// How far the Markdown preview is scrolled, which is separate from the source.
    pub preview_scroll: f32,
    /// What the source is to be scrolled back to once it has been laid out at a new font size, and
    /// the same for the preview. See [`ViewAnchor`].
    pub zoom_anchor: Option<ViewAnchor>,
    pub preview_anchor: Option<ViewAnchor>,
    /// What is selected in this tab's Markdown preview, as a range into the preview's own text.
    ///
    /// On the tab beside the scroll position it lives with, rather than on the window, because a
    /// preview belongs to a file and two panes can be showing two of them. It is emptied when the
    /// preview is worked out again, since a byte range into text that has been rebuilt means
    /// nothing — see `UnluminousApp::refresh_preview`.
    pub preview_selection: Selection,
    /// One row a paragraph, once this file has been annotated with git blame.
    pub blame: Option<Vec<BlameRow>>,
    /// Which paragraphs differ from the version git has.
    pub line_changes: Vec<(usize, Change)>,
    /// True while this is the tab a single click reuses.
    pub transient: bool,
    /// Set once git has been asked what it thinks of this file, so it is not asked every frame.
    pub git_asked: bool,
    /// The picture, when this tab holds one rather than text.
    pub picture: Option<Picture>,
    /// The plugin, when this tab is a plugin's own rather than a file.
    ///
    /// The picture precedent, followed exactly. A tab is a `Document`, and a tab that holds something
    /// else holds it beside one: the four questions the window asks a tab — is it modified, can it be
    /// saved, has it a preview, has it a gutter — answer the same way for a picture and for a plugin.
    pub plugin: Option<PluginTab>,
    /// The revision this file's marked passages were last pushed into `services::file_marks` at.
    ///
    /// A document that has not changed since it was last pushed cannot have gained a mark, so this
    /// makes keeping the store up to date one integer comparison a tab a frame rather than a
    /// comparison of two lists.
    pub marked_revision: Option<u64>,
    /// The revision this file's breakpoints were last pushed into `services::breakpoint_store` at.
    ///
    /// The same integer comparison for the same reason, one line below the marks it copies: a
    /// document that has not changed since it was last pushed cannot have gained a breakpoint.
    pub breakpoints_at: Option<u64>,
    /// The revision this file was last coloured at, so a plugin's syntax colouring is not run twice
    /// for one revision. One per file rather than one for the window, so the file in the second pane
    /// is coloured too.
    pub coloured_revision: Option<u64>,
    /// The tokens of this file as they were last read, so the next reading starts at the edit.
    ///
    /// One per file rather than one for the window, for `coloured_revision`'s reason: the file in
    /// the second pane is coloured too, and a shared cache would have the two overwriting each
    /// other's reading every frame. See `unluminous_core::incremental`. `task-1804` §5.2.
    pub syntax_tokens: unluminous_core::IncrementalTokens,
    /// Where the diagram has been moved and scaled to, for a tab holding a Mermaid file.
    ///
    /// Beside `preview_scroll` rather than instead of it, because they are two different ways of
    /// moving about: the Markdown preview scrolls like text, and a diagram is panned and zoomed like
    /// a picture.
    pub diagram: crate::components::diagram_view::View,
    /// Where this tab lives: a pane of the editing area, or a node on the canvas. See [`Home`].
    pub home: Home,
    /// When this tab was last shown, from `OpenFiles`' own counter. The tab showing in a pane is the
    /// one in it with the highest stamp.
    pub shown_at: u64,
    /// What has been laid out for this file, kept between frames.
    pub cached: Cached,
    /// What the file looked like on disk the last time this tab read it or wrote it.
    ///
    /// A tab is owned by its `Document` and Unluminous deliberately watches nothing, which is right until
    /// something else changes the file: the tab then goes on showing text that is no longer what is
    /// there, and `editor text` answers with it. `UnluminousApp::reread_if_the_file_changed` compares this
    /// against the folder before a read, which is the rule the symbol index already follows for a
    /// closed file — the disk-owned side is re-checked at the moment of use.
    ///
    /// `None` for a tab that has never been saved, and for one whose file could not be measured.
    pub disk: Option<DiskStamp>,
}

impl OpenFile {
    pub fn new(document: Document) -> Self {
        Self {
            document,
            browser: None,
            typed_address: String::new(),
            editing_address: false,
            sized_at: None,
            view_mode: ViewMode::Raw,
            scroll: 0.0,
            preview_scroll: 0.0,
            zoom_anchor: None,
            preview_anchor: None,
            preview_selection: Selection::caret(0),
            blame: None,
            line_changes: Vec::new(),
            transient: false,
            git_asked: false,
            picture: None,
            plugin: None,
            marked_revision: None,
            breakpoints_at: None,
            coloured_revision: None,
            syntax_tokens: unluminous_core::IncrementalTokens::default(),
            diagram: crate::components::diagram_view::View::default(),
            home: Home::Pane(0),
            shown_at: 0,
            cached: Cached::fresh(),
            disk: None,
        }
    }

    /// Note what the file looks like on disk now, which is what a later read is compared against.
    ///
    /// Called wherever this tab and the file agree: when it is opened, when it is read again, and
    /// when it is written. A tab with no path has nothing to measure.
    pub fn note_what_is_on_disk(&mut self) {
        self.disk = self.document.path().and_then(DiskStamp::of);
    }

    /// True when the file has changed underneath this tab since it last read or wrote it.
    ///
    /// False for a tab with unsaved changes, which is not a question about the disk: those are the
    /// person's, and throwing them away is what `tab reload --discard` is for. False also for a tab
    /// that has never been saved and for a file that has been deleted — a tab whose file has gone
    /// keeps what it has rather than being emptied.
    pub fn the_file_changed_underneath(&self) -> bool {
        if self.document.is_modified() {
            return false;
        }
        let Some(was) = self.disk else { return false };
        let Some(path) = self.document.path() else { return false };
        DiskStamp::of(path).is_some_and(|now| now != was)
    }

    /// A tab holding a picture rather than text.
    pub fn picture(path: &Path) -> Self {
        Self { picture: Some(Picture::open(path)), ..Self::new(Document::at_path(path)) }
    }

    /// A permanent rendered browser tab whose native view is owned by the window's browser host.
    pub fn browser(tab: crate::services::browser::BrowserTab) -> Self {
        Self { browser: Some(tab), ..Self::new(Document::new()) }
    }

    /// True when this tab renders a web page rather than an editable document.
    pub fn is_browser(&self) -> bool {
        self.browser.is_some()
    }

    /// True when this tab holds a picture, which is what decides how the editing area is drawn and
    /// what `Save` refuses to do.
    pub fn is_picture(&self) -> bool {
        self.picture.is_some()
    }

    /// What the tab is called: the file's name, or `untitled` when it has never been saved.
    pub fn name(&self) -> String {
        // A tab a plugin draws is called what its manifest calls it. Falling through to the document's path
        // named it `untitled`, because a plugin tab has no path — which is also why it is asked first.
        if let Some(plugin) = &self.plugin {
            return plugin.label.clone();
        }
        if let Some(browser) = &self.browser {
            return browser.name();
        }
        self.document
            .path()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled".to_owned())
    }

    /// True when this tab holds a plugin's own contents rather than a file.
    pub fn is_a_plugin(&self) -> bool {
        self.plugin.is_some()
    }

    pub fn path(&self) -> Option<&Path> {
        self.document.path()
    }

    /// Git's idea of this file is about the file on disk, so it is thrown away when the file
    /// changes underneath it or a different file takes the tab.
    pub fn forget_git(&mut self) {
        self.blame = None;
        self.line_changes.clear();
        self.git_asked = false;
    }

    /// A different document has taken this tab, so everything worked out from the last one goes.
    ///
    /// The revision starts again at one for the document that has arrived, so comparing revisions
    /// alone would leave the new file wearing the old one's layout and the old one's colours.
    pub fn forget_what_was_worked_out(&mut self) {
        self.cached = Cached::fresh();
        self.coloured_revision = None;
    }

    /// A different document has taken this tab, so where the last one was being read means nothing.
    ///
    /// Separate from [`Self::forget_what_was_worked_out`] because that is also how showing a tab
    /// throws away what was laid out for it, and a tab being shown is a tab whose document is the
    /// one it always had: an anchor thrown away there is a tab that jumps the next time the font
    /// changes while it is not the one on the screen, which is the fault `task-1672` is about.
    pub fn forget_where_it_was_being_read(&mut self) {
        self.zoom_anchor = None;
        self.preview_anchor = None;
    }

    /// Remember where each view is, so a change of font size can put it back.
    ///
    /// The top of the view, which is what a person means by "do not move the file about" when the
    /// size is changed from the Settings window or from a tab they are not looking at. A zoom over
    /// the text asks for a point of its own — the pointer, or the caret — and sets that first,
    /// which is why an anchor already taken is left alone: the one nearest to what the reader is
    /// actually doing wins, and both describe the layout as it is now.
    pub fn anchor_the_views(&mut self) {
        if self.zoom_anchor.is_none() {
            let at = self.cached.layout.anchor_at_y(self.scroll);
            self.zoom_anchor = Some(ViewAnchor { at, above: 0.0 });
        }
        if self.preview_anchor.is_none() {
            let at = self.cached.preview_layout.anchor_at_y(self.preview_scroll);
            self.preview_anchor = Some(ViewAnchor { at, above: 0.0 });
        }
    }
}

/// The open files, which pane each is in, and which pane has the keyboard.
///
/// ## Panes
///
/// `task-1664` asks for the reference editor's split view: the editing area cut into panes side by side, each
/// with its own tabs, so several files are on the screen at once. A pane is **a set of tabs and
/// which of them is showing**, and everything else a person would call the state of an editor —
/// the scroll position, the view mode, the blame, the laid out text — is already on the tab.
///
/// Where a tab lives is **written on the tab**, as [`OpenFile::home`], rather than held as a
/// list of indices in a pane. Every index into `files` shifts when a tab is opened or closed, so a
/// pane holding indices would have to be fixed up by all seven of the operations below, and a
/// fix-up is the sort of thing that is right for a month. A number on the tab survives every
/// shuffle of the vector without a line of maintenance.
///
/// Which tab is *showing* in a pane is answered the same way. [`OpenFile::shown_at`] is stamped
/// from `clock` each time a tab is shown, and the tab showing in a pane is the one in it with the
/// highest stamp. That is a walk of a handful of integers, and it gives the right answer for free
/// in the case that would otherwise need thinking about: close the tab that is showing and the one
/// that comes forward is the one you were looking at before it, which is what the reference editor does.
///
/// Two invariants are kept by [`Self::tidy`] after every change, and asserted in the tests:
///
/// - **Panes are numbered `0..panes` with no gaps**, so a pane can be found by counting from the
///   left and drawn in that order.
/// - **No pane is empty.** A pane that loses its last tab is removed, except the last remaining
///   pane, which is left with a fresh untitled tab — the rule [`Self::close`] already keeps for the
///   window as a whole, so nothing that draws a pane needs a special case for an empty one.
pub struct OpenFiles {
    files: Vec<OpenFile>,
    /// How many panes the editing area is divided into. Never less than one.
    panes: usize,
    /// Where the keyboard is: a pane of the editing area, or a File Editor node on the canvas.
    ///
    /// **A `Home` rather than a pane number**, so that `active()` answers with the node's file while a
    /// node has the keys - which is what makes `editor text`, `tab save`, the gutter, the folds and
    /// four hundred lines of `show_editor` work on a node with no second implementation.
    focus: Home,
    /// The last pane the keyboard was in.
    ///
    /// A memory of a past value rather than a second opinion about the present one, which is the
    /// distinction `app::Maximised` already draws: two dozen callers want a pane **number**, and while
    /// the keyboard is on a node there is no true answer to give them. Set whenever `focus` becomes a
    /// pane, and never read to decide where a key press goes.
    last_pane: usize,
    /// Each pane's share of the editing area's width, in the same order. Sums to one.
    ///
    /// A fraction rather than a measurement so that opening the project on a screen of another size
    /// gives the same proportions rather than the same points.
    widths: Vec<f32>,
    /// Stamps [`OpenFile::shown_at`]. Counts up and is never reset.
    clock: u64,
}

impl OpenFiles {
    /// One tab, holding `document`, in one pane.
    pub fn new(document: Document) -> Self {
        Self {
            files: vec![OpenFile::new(document)],
            panes: 1,
            focus: Home::Pane(0),
            last_pane: 0,
            widths: vec![1.0],
            clock: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Which tab is showing: the most recently shown tab in the pane that has the keyboard.
    ///
    /// This is the meaning of "the open file" everywhere else in the window, which is why it is
    /// derived rather than stored. Nothing outside this file had to learn about panes to go on
    /// asking it.
    pub fn active_index(&self) -> usize {
        self.showing_at(self.focus).unwrap_or(0)
    }

    pub fn active(&self) -> &OpenFile {
        &self.files[self.active_index()]
    }

    pub fn active_mut(&mut self) -> &mut OpenFile {
        let index = self.active_index();
        &mut self.files[index]
    }

    pub fn iter(&self) -> impl Iterator<Item = &OpenFile> {
        self.files.iter()
    }

    /// Every open file, to be changed. The editor's font is one setting for the whole window, so
    /// there has to be a way to reach the tabs that are not showing.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut OpenFile> {
        self.files.iter_mut()
    }

    pub fn get(&self, index: usize) -> Option<&OpenFile> {
        self.files.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut OpenFile> {
        self.files.get_mut(index)
    }

    /// Which tab holds `path`, if any.
    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.files.iter().position(|file| file.path() == Some(path))
    }

    /// The tab at `index`. Panics past the end, as `active` does: an index here always came from
    /// [`Self::index_of`] or from a walk of the tabs.
    pub fn at(&self, index: usize) -> &OpenFile {
        &self.files[index]
    }

    pub fn at_mut(&mut self, index: usize) -> &mut OpenFile {
        &mut self.files[index]
    }

    // ------------------------------------------------------------------------------- the panes

    /// How many panes the editing area is divided into.
    pub fn pane_count(&self) -> usize {
        self.panes
    }

    /// Which pane has the keyboard, or last had it while it is on a node.
    pub fn focused_pane(&self) -> usize {
        self.last_pane
    }

    /// Where the keyboard is.
    pub fn focus(&self) -> Home {
        self.focus
    }

    /// Put it back where it was, whatever kind of place that was.
    ///
    /// What the pane loop restores with. `focus_pane(keyboard)` would drag a keyboard living on a
    /// canvas node back into the editing area on every frame.
    pub fn restore_focus(&mut self, home: Home) {
        self.focus = home;
        if let Home::Pane(pane) = home {
            self.last_pane = pane.min(self.panes.saturating_sub(1));
        }
    }

    /// Put the keyboard in a File Editor node, so `active()` answers with its file.
    pub fn focus_node(&mut self, node: u64) {
        self.focus = Home::Node(node);
    }

    /// Which tab is **showing** in a File Editor node, if it has one.
    ///
    /// `showing_at` rather than `position`, which is the same function `showing_in` answers the same
    /// question about a pane with: the one in it that was shown most recently. `task-1905` gives a node a
    /// strip of tabs, and "the first tab that lives here" is only the same answer while a node holds one.
    pub fn tab_in_node(&self, node: u64) -> Option<usize> {
        self.showing_at(Home::Node(node))
    }

    /// Every tab living in one File Editor node, in the order they were opened.
    ///
    /// What the node's own strip draws, and it is `tabs_in_pane`'s twin.
    pub fn tabs_in_node(&self, node: u64) -> Vec<usize> {
        self.files
            .iter()
            .enumerate()
            .filter(|(_, file)| file.home == Home::Node(node))
            .map(|(index, _)| index)
            .collect()
    }

    /// Every tab living on a node, which is what closing a canvas has to close.
    pub fn tabs_on_nodes(&self) -> Vec<usize> {
        self.files
            .iter()
            .enumerate()
            .filter(|(_, file)| file.home.node().is_some())
            .map(|(index, _)| index)
            .collect()
    }

    /// Move a tab onto a node, taking it out of whatever pane it was in.
    ///
    /// **A file already open is moved rather than copied**, which is `OpenFiles::open`'s own rule and
    /// the reason `Split Right` moves a tab: two `Document`s over one path would be two windows on one
    /// file, and whichever was saved second would win.
    pub fn move_to_node(&mut self, index: usize, node: u64) -> bool {
        if index >= self.files.len() {
            return false;
        }
        // A tab moved into a node **joins** whatever is there rather than replacing it — `task-1905`
        // asks for several tabs on a node, and `stamp` below is what makes this the one showing.
        self.files[index].home = Home::Node(node);
        self.stamp(index);
        self.focus = Home::Node(node);
        // **The editing area always has a tab**, which is the promise `close` already keeps and the
        // one thing moving a tab onto a node could break: the window starts with one untitled tab,
        // `open` reuses an empty one, and a node that took it would leave pane zero with nothing to
        // draw. A fresh untitled tab is what stands in its place, exactly as closing the last tab
        // leaves one.
        if !self.files.iter().any(|file| file.home.pane().is_some()) {
            self.files.push(OpenFile::new(Document::new()));
            self.panes = 1;
            self.last_pane = 0;
            self.widths = vec![1.0];
        }
        self.tidy();
        true
    }

    /// Put the keyboard in a pane. A number past the end is refused rather than clamped, so a
    /// command line that names a pane that is not there is told so.
    pub fn focus_pane(&mut self, pane: usize) -> bool {
        if pane >= self.panes {
            return false;
        }
        self.focus = Home::Pane(pane);
        self.last_pane = pane;
        true
    }

    /// The keyboard to the next pane, wrapping round at the right hand end.
    pub fn next_pane(&mut self) {
        self.focus_pane((self.last_pane + 1) % self.panes);
    }

    pub fn previous_pane(&mut self) {
        self.focus_pane((self.last_pane + self.panes - 1) % self.panes);
    }

    /// Which pane a tab is in. Zero for a tab living on a canvas node, which is in none.
    pub fn pane_of(&self, index: usize) -> usize {
        self.files.get(index).and_then(|file| file.home.pane()).unwrap_or(0)
    }

    /// Where a tab lives.
    pub fn home_of(&self, index: usize) -> Home {
        self.files.get(index).map(|file| file.home).unwrap_or_default()
    }

    /// The tabs in one pane, as indices into the open files, in the order they are drawn.
    pub fn tabs_in(&self, pane: usize) -> Vec<usize> {
        self.files
            .iter()
            .enumerate()
            .filter(|(_, file)| file.home == Home::Pane(pane))
            .map(|(index, _)| index)
            .collect()
    }

    /// Which tab is showing in `pane`: the one in it that was shown most recently.
    pub fn showing_in(&self, pane: usize) -> Option<usize> {
        self.showing_at(Home::Pane(pane))
    }

    /// The same question about any home: which of the tabs living there was shown most recently.
    pub fn showing_at(&self, home: Home) -> Option<usize> {
        self.files
            .iter()
            .enumerate()
            .filter(|(_, file)| file.home == home)
            .max_by_key(|(_, file)| file.shown_at)
            .map(|(index, _)| index)
    }

    /// Each pane's share of the editing area's width, left to right.
    pub fn pane_widths(&self) -> &[f32] {
        &self.widths
    }

    /// Move the divider between pane `left` and the one after it by `delta` of the whole width.
    ///
    /// `smallest` is the least share a pane may have, which the caller works out from how wide the
    /// editing area is: a divider that could be dragged past its neighbour would be a way of losing
    /// a pane off the side of the window.
    pub fn move_divider(&mut self, left: usize, delta: f32, smallest: f32) {
        if left + 1 >= self.panes || self.widths.len() != self.panes {
            return;
        }
        let total = self.widths[left] + self.widths[left + 1];
        let smallest = smallest.min(total / 2.0);
        let taken = (self.widths[left] + delta).clamp(smallest, total - smallest);
        self.widths[left] = taken;
        self.widths[left + 1] = total - taken;
    }

    /// Set one pane's share directly, which is what the command line does, sharing what is left
    /// between the others in the proportions they already had.
    pub fn set_pane_width(&mut self, pane: usize, fraction: f32) -> bool {
        if pane >= self.panes || self.panes < 2 {
            return false;
        }
        let wanted = fraction.clamp(0.05, 0.95);
        let rest: f32 =
            self.widths.iter().enumerate().filter(|(at, _)| *at != pane).map(|(_, w)| w).sum();
        for (at, width) in self.widths.iter_mut().enumerate() {
            if at == pane {
                *width = wanted;
            } else if rest > 0.0 {
                *width = *width / rest * (1.0 - wanted);
            } else {
                *width = (1.0 - wanted) / (self.panes - 1) as f32;
            }
        }
        true
    }

    /// Every pane the same width, which is what double clicking a divider asks for.
    pub fn reset_pane_widths(&mut self) {
        self.widths = vec![1.0 / self.panes as f32; self.panes];
    }

    /// Put a pane to the right of the one that has the keyboard, and move the tab that is showing
    /// into it.
    ///
    /// The tab **moves** rather than being copied, which is where Unluminous and the reference editor part company:
    /// The reference editor's `Split Right` shows the same file in both splits, and Unluminous cannot, because two
    /// tabs on one file would be two documents over one path and saving either would throw the
    /// other away. This is the reference editor's `Split and Move Right` under the name a person looks for.
    /// `tasks/task-1664-split-view-tdd.md` §3 records what was weighed.
    ///
    /// **When the pane holds only that tab**, taking it away would empty the pane it came from and
    /// leave the window looking exactly as it did. So the tab stays where it is and the new pane
    /// opens empty, with a fresh untitled tab in it. That is what a person means by putting a pane
    /// on the right: the next file they open lands in it, because opening a file always lands in
    /// the pane with the keyboard.
    pub fn split_right(&mut self) {
        let pane = self.last_pane;
        let alone = self.tabs_in(pane).len() < 2;
        let showing = self.showing_in(pane);
        self.add_pane_after(pane);
        let new = pane + 1;
        if alone {
            let mut fresh = OpenFile::new(Document::new());
            fresh.home = Home::Pane(new);
            let at =
                showing.map(|index| index + 1).unwrap_or(self.files.len()).min(self.files.len());
            self.files.insert(at, fresh);
            self.focus_pane(new);
            self.stamp(at);
        } else if let Some(index) = showing {
            self.files[index].home = Home::Pane(new);
            self.focus_pane(new);
            self.stamp(index);
        }
        self.tidy();
    }

    /// Move the tab that is showing into the pane beside it. `false` when there is no pane that way.
    pub fn move_tab(&mut self, right: bool) -> bool {
        let pane = self.last_pane;
        let target = if right { pane + 1 } else { pane.checked_sub(1).unwrap_or(usize::MAX) };
        if target >= self.panes {
            return false;
        }
        let Some(index) = self.showing_in(pane) else {
            return false;
        };
        self.files[index].home = Home::Pane(target);
        self.focus_pane(target);
        self.stamp(index);
        // The pane it left may now be empty, in which case `tidy` removes it and the panes after it
        // are renumbered — including the one the tab has just been put into.
        self.tidy();
        true
    }

    /// Put the tab at `index` into `pane`, `position` tabs along it. This is what dragging a tab
    /// does, and what `unluminous-cli tab move` asks for.
    ///
    /// `position` counts the tabs of the target pane **as they are on the screen now**, including
    /// the tab being moved when it is already in that pane — because that is what a person dragging
    /// one is looking at. Taking it out first shifts everything after it up by one, so a move within
    /// a pane to a place further along has one subtracted from it here rather than at every call.
    ///
    /// A position past the end means the end, so dropping a tab anywhere to the right of the last
    /// one puts it last. Dropping it into a pane with nothing in it works for the same reason.
    ///
    /// The tab is **shown** where it lands and the keyboard follows it, which is what dragging
    /// something somewhere means; and the pane it left is folded away by [`Self::tidy`] if it was
    /// its last tab, exactly as [`Self::move_tab`] already leaves it.
    pub fn drag_tab(&mut self, index: usize, pane: usize, position: usize) -> bool {
        if index >= self.files.len() || pane >= self.panes {
            return false;
        }
        // **A tab coming from a node is not already in a pane**, and `pane_of` answers `0` for one — so
        // dragging the first tab out of a node into pane zero took the "dropped where it already is" branch
        // below and nothing moved. `task-1905`.
        let from = self.files[index].home.pane();
        let within = from
            .map(|from| self.tabs_in(from).iter().position(|at| *at == index).unwrap_or(0))
            .unwrap_or(0);
        let mut position = position;
        if from == Some(pane) {
            if within < position {
                position -= 1;
            }
            if within == position {
                // Dropped where it already is. Showing it is still right — a person who picked a tab
                // up and put it back plainly means to be looking at it — but nothing moves.
                self.focus_pane(pane);
                self.stamp(index);
                return true;
            }
        }
        let mut file = self.files.remove(index);
        file.home = Home::Pane(pane);
        // Where in the vector: at the tab that is to come after it, or after the last one when it is
        // going on the end. Worked out after the removal, so the indices are the ones being inserted
        // into rather than the ones that were there a moment ago.
        let targets = self.tabs_in(pane);
        let at = match targets.get(position) {
            Some(index) => *index,
            None => targets.last().map(|index| index + 1).unwrap_or(self.files.len()),
        };
        self.files.insert(at, file);
        self.focus_pane(pane);
        self.stamp(at);
        self.tidy();
        true
    }

    /// Move a tab into a File Editor **node**, at `position` along that node's own strip.
    ///
    /// [`Self::drag_tab`]'s twin, and the two are apart because a pane and a node are two kinds of home:
    /// `drag_tab` refuses a pane number past the end, and a node is named by an id rather than counted.
    /// What they share is the one subtlety — `position` counts the target's tabs **as they are on the
    /// screen now**, including the tab being carried when it is already there, so a move further along its
    /// own strip has one subtracted from it here rather than at every call. `task-1905`.
    pub fn drag_tab_to_node(&mut self, index: usize, node: u64, position: usize) -> bool {
        if index >= self.files.len() {
            return false;
        }
        let home = Home::Node(node);
        let already = self.files[index].home == home;
        let mut position = position;
        if already {
            let within = self.tabs_in_node(node).iter().position(|at| *at == index).unwrap_or(0);
            if within < position {
                position -= 1;
            }
            if within == position {
                // Dropped where it already is. Showing it is still right — a person who picked a tab up
                // and put it back plainly means to be looking at it — but nothing moves.
                self.focus_node(node);
                self.stamp(index);
                return true;
            }
        }
        let mut file = self.files.remove(index);
        file.home = home;
        let targets = self.tabs_in_node(node);
        let at = match targets.get(position) {
            Some(index) => *index,
            None => targets.last().map(|index| index + 1).unwrap_or(self.files.len()),
        };
        self.files.insert(at, file);
        self.focus_node(node);
        self.stamp(at);
        // **The editing area always has a tab**, which is `move_to_node`'s own promise and the one thing
        // dragging the last tab out of a pane could break.
        if !self.files.iter().any(|file| file.home.pane().is_some()) {
            self.files.push(OpenFile::new(Document::new()));
            self.panes = 1;
            self.last_pane = 0;
            self.widths = vec![1.0];
        }
        self.tidy();
        true
    }

    /// Fold the pane that has the keyboard into the one beside it: the pane on its left where there
    /// is one, otherwise the pane on its right. The reference editor's `Unsplit`.
    pub fn unsplit(&mut self) -> bool {
        if self.panes < 2 {
            return false;
        }
        let pane = self.last_pane;
        let target = if pane > 0 { pane - 1 } else { 1 };
        for file in self.files.iter_mut().filter(|file| file.home == Home::Pane(pane)) {
            file.home = Home::Pane(target);
        }
        self.focus_pane(target);
        self.tidy();
        true
    }

    /// Every tab back into one pane. The reference editor's `Unsplit All`.
    pub fn unsplit_all(&mut self) -> bool {
        if self.panes < 2 {
            return false;
        }
        for file in &mut self.files {
            if file.home.pane().is_some() {
                file.home = Home::Pane(0);
            }
        }
        self.focus_pane(0);
        self.tidy();
        true
    }

    /// Put the tabs back into the panes a project was left in.
    ///
    /// `panes` is one number a tab, in tab order, and anything it says that would break an invariant
    /// is corrected rather than refused: a pane number past the end is clamped, a list of the wrong
    /// length leaves the tabs it does not reach in pane zero, and a set of numbers that leaves a
    /// pane empty is collapsed by [`Self::tidy`]. A hand edited state file must not stop a project
    /// opening, which is the rule the whole of `services::project_state` keeps.
    pub fn restore_panes(&mut self, panes: &[usize], widths: &[f32], focus: usize) {
        let most = panes.iter().copied().max().map(|highest| highest + 1).unwrap_or(1);
        self.panes = most.max(1);
        for (file, pane) in self.files.iter_mut().zip(panes) {
            file.home = Home::Pane((*pane).min(most.saturating_sub(1)));
        }
        self.widths = if widths.len() == self.panes {
            widths.to_vec()
        } else {
            vec![1.0 / self.panes as f32; self.panes]
        };
        let focus = focus.min(self.panes.saturating_sub(1));
        self.focus = Home::Pane(focus);
        self.last_pane = focus;
        self.tidy();
    }

    /// Which pane each tab is in, in tab order, for the project's state file.
    pub fn panes_of_tabs(&self) -> Vec<usize> {
        self.files.iter().map(|file| file.home.pane().unwrap_or(0)).collect()
    }

    /// Add an empty pane after `pane`, dividing that pane's share of the width in half.
    ///
    /// Half of the pane being split rather than an equal share of everything, because the panes
    /// either side of it have no reason to move when the third of four is split.
    fn add_pane_after(&mut self, pane: usize) {
        for file in &mut self.files {
            if let Home::Pane(at) = file.home {
                if at > pane {
                    file.home = Home::Pane(at + 1);
                }
            }
        }
        let share = self.widths.get(pane).copied().unwrap_or(1.0) / 2.0;
        if pane < self.widths.len() {
            self.widths[pane] = share;
        }
        self.widths.insert((pane + 1).min(self.widths.len()), share);
        self.panes += 1;
    }

    /// Keep the two invariants: panes numbered without gaps, and no pane empty.
    ///
    /// Called after everything that moves a tab between panes or takes one away. A pane that is
    /// removed gives its share of the width to the pane that takes its place in the row, so the
    /// widths still sum to one and the panes either side of it do not jump.
    fn tidy(&mut self) {
        if self.files.is_empty() {
            self.panes = 1;
            self.focus = Home::Pane(0);
            self.last_pane = 0;
            self.widths = vec![1.0];
            return;
        }
        if self.widths.len() != self.panes {
            self.widths = vec![1.0 / self.panes.max(1) as f32; self.panes.max(1)];
        }
        // Old pane number to new, keeping only the panes that still hold a tab.
        let mut renumbered: Vec<Option<usize>> = vec![None; self.panes];
        let mut widths: Vec<f32> = Vec::new();
        let mut carried = 0.0;
        for (pane, slot) in renumbered.iter_mut().enumerate() {
            let width = self.widths.get(pane).copied().unwrap_or(0.0);
            if self.files.iter().any(|file| file.home == Home::Pane(pane)) {
                *slot = Some(widths.len());
                widths.push(width + carried);
                carried = 0.0;
            } else {
                carried += width;
            }
        }
        if widths.is_empty() {
            // Every pane number on every tab is out of range, which a hand edited state file could
            // ask for. One pane holding everything is the answer that cannot be wrong.
            for file in &mut self.files {
                if file.home.pane().is_some() {
                    file.home = Home::Pane(0);
                }
            }
            self.panes = 1;
            self.focus = Home::Pane(0);
            self.last_pane = 0;
            self.widths = vec![1.0];
            return;
        }
        if carried > 0.0 {
            let last = widths.len() - 1;
            widths[last] += carried;
        }
        for file in &mut self.files {
            if let Home::Pane(pane) = file.home {
                file.home = Home::Pane(renumbered.get(pane).copied().flatten().unwrap_or(0));
            }
        }
        self.panes = widths.len();
        // The pane that had the keyboard may have gone, in which case the keyboard goes to the pane
        // that took its place, which is the one now standing where it stood.
        self.last_pane = renumbered
            .get(self.last_pane)
            .copied()
            .flatten()
            .unwrap_or_else(|| self.last_pane.min(self.panes - 1));
        // The keyboard follows only when it was in a pane. A keyboard living on a canvas node stays
        // there: nothing that happened to the panes is about it.
        if self.focus.pane().is_some() {
            self.focus = Home::Pane(self.last_pane);
        }
        let total: f32 = widths.iter().sum();
        self.widths = if total > 0.0 && total.is_finite() {
            widths.iter().map(|width| width / total).collect()
        } else {
            vec![1.0 / self.panes as f32; self.panes]
        };
    }

    /// This tab is the one showing in its pane from now on.
    fn stamp(&mut self, index: usize) {
        self.clock += 1;
        let clock = self.clock;
        if let Some(file) = self.files.get_mut(index) {
            file.shown_at = clock;
        }
    }

    // ------------------------------------------------------------------------------- the tabs

    /// Show the tab at `index`, if there is one there, putting the keyboard in its pane.
    pub fn show(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }
        let home = self.files[index].home;
        let displaced = self.showing_at(home).filter(|displaced| *displaced != index);
        self.focus = home;
        if let Home::Pane(pane) = home {
            self.last_pane = pane;
        }
        self.stamp(index);
        // The one moment a layout is both complete and cold: the tab that was showing in this pane
        // has just been put behind another, and will not be laid out again until it is shown. That
        // is the cache boundary `tasks/task-1813-performance-review-tdd.md` section 3 asks for, and
        // it is deliberately the *displaced* tab rather than a sweep of every hidden one - a sweep
        // walks every line of every hidden layout on every tab switch, for ever, to find nothing.
        if let Some(displaced) = displaced {
            self.files[displaced].cached.compact_layouts();
        }
    }

    /// Show the next tab **in the pane that has the keyboard**, wrapping round at the end, which is
    /// what Alt and an arrow key do. A pane's tabs are its own, so walking them never leaves it.
    pub fn next(&mut self) {
        self.step(true);
    }

    pub fn previous(&mut self) {
        self.step(false);
    }

    fn step(&mut self, forwards: bool) {
        // **Whichever home has the keyboard**, which since `task-1905` may be a File Editor node rather than
        // a pane. `last_pane` is where the keyboard was last in the *editing area*, so with a node focused
        // Next Tab walked an unrelated pane's tabs — the Codex Sol review of `task-1905` found it. `focus` is
        // the one value that says where the keyboard is, so it is the one thing asked.
        let tabs = match self.focus {
            Home::Node(node) => self.tabs_in_node(node),
            Home::Pane(_) => self.tabs_in(self.last_pane),
        };
        if tabs.is_empty() {
            return;
        }
        let showing = self.active_index();
        let at = tabs.iter().position(|index| *index == showing).unwrap_or(0);
        let next =
            if forwards { (at + 1) % tabs.len() } else { (at + tabs.len() - 1) % tabs.len() };
        self.show(tabs[next]);
    }

    /// This tab is no longer one a single click will reuse.
    pub fn make_permanent(&mut self, index: usize) {
        if let Some(file) = self.files.get_mut(index) {
            file.transient = false;
        }
    }

    /// Open `document`.
    ///
    /// A file that is already open is shown rather than opened twice, because two tabs on one file
    /// would be two documents over one path and saving either would throw the other away. When
    /// `permanent` is false and the pane with the keyboard has a transient tab, that tab's contents
    /// are replaced; otherwise a tab is added after the one that is showing, which is where a new
    /// tab belongs when it was opened from the one before it.
    ///
    /// Both the tab it reuses and the tab it adds are in the **pane that has the keyboard**, which
    /// is what makes a new pane useful: split, then open, and the file lands in the new pane.
    ///
    /// Returns the index of the tab the document ended up in.
    /// The tab holding the plugin tab named `key`, if one is open.
    pub fn index_of_plugin_tab(&self, key: &str) -> Option<usize> {
        self.files
            .iter()
            .position(|file| file.plugin.as_ref().is_some_and(|plugin| plugin.key == key))
    }

    /// Open a tab a plugin draws, in the pane that has the keyboard.
    ///
    /// Permanent rather than transient: a tab a person asked for from a menu is not the tab a single
    /// click in the explorer reuses, which is the same distinction `open` already makes.
    pub fn open_plugin_tab(&mut self, document: Document, plugin: PluginTab) -> usize {
        if let Some(index) = self.index_of_plugin_tab(&plugin.key) {
            self.show(index);
            return index;
        }
        let index = self.open(document, true);
        self.files[index].plugin = Some(plugin);
        index
    }

    pub fn open(&mut self, document: Document, permanent: bool) -> usize {
        if let Some(path) = document.path() {
            if let Some(index) = self.index_of(path) {
                self.show(index);
                if permanent {
                    self.files[index].transient = false;
                }
                return index;
            }
        }
        match self.reuse(permanent) {
            Some(index) => {
                let file = &mut self.files[index];
                file.document = document;
                file.view_mode = ViewMode::Raw;
                file.scroll = 0.0;
                file.preview_scroll = 0.0;
                file.transient = !permanent;
                file.picture = None;
                file.browser = None;
                file.forget_git();
                file.forget_what_was_worked_out();
                file.forget_where_it_was_being_read();
                self.show(index);
                index
            }
            None => {
                let mut file = OpenFile::new(document);
                file.transient = !permanent;
                self.insert_beside_the_open_tab(file)
            }
        }
    }

    /// Open a tab that is already built, which is how a picture gets one: it is not a document that
    /// was read, so it cannot come in through [`Self::open`].
    ///
    /// It follows exactly the same rules — a file that is already open is shown rather than opened
    /// twice, a transient tab is reused, a new tab lands beside the one it was opened from — because
    /// they are the rules about tabs rather than about text.
    pub fn open_file(&mut self, file: OpenFile, permanent: bool) -> usize {
        if let Some(path) = file.path().map(Path::to_path_buf) {
            if let Some(index) = self.index_of(&path) {
                self.show(index);
                if permanent {
                    self.files[index].transient = false;
                }
                return index;
            }
        }
        let mut file = file;
        file.transient = !permanent;
        match self.reuse(permanent) {
            Some(index) => {
                file.home = self.files[index].home;
                self.files[index] = file;
                self.show(index);
                index
            }
            None => self.insert_beside_the_open_tab(file),
        }
    }

    /// The tab a newly opened file should take over, if there is one: the transient tab in the pane
    /// with the keyboard, or an untitled tab in it that has never been touched.
    ///
    /// The untitled one is reused whether the file was asked for permanently or not, so opening the
    /// first file in a fresh window — or in a pane that has just been split off — does not leave an
    /// empty tab beside it.
    fn reuse(&self, permanent: bool) -> Option<usize> {
        // The pane the keyboard is in, or the one it was last in while the canvas holds it. A tab
        // living on a node is never reused for a file somebody opened: a node shows what it was given.
        let pane = Home::Pane(self.last_pane.min(self.panes.saturating_sub(1)));
        let mine = |file: &OpenFile| file.home == pane;
        let transient = self.files.iter().position(|file| mine(file) && file.transient);
        let empty = self.files.iter().position(|file| {
            mine(file)
                && file.path().is_none()
                && !file.is_browser()
                && file.document.text().is_empty()
                && !file.document.is_modified()
        });
        if permanent {
            empty
        } else {
            transient.or(empty)
        }
    }

    /// Put a tab into the pane that has the keyboard, after the tab showing in it.
    /// Put a tab into the pane that has the keyboard, after the tab showing in it.
    ///
    /// **Into a pane, never onto a canvas node.** A node holds one thing, put there deliberately by
    /// [`Self::move_to_node`]; a tab opened while the canvas has the keyboard is a tab somebody asked
    /// for in the editing area, and landing it on a node would hide it behind what the node was
    /// already showing with no way to reach it. Measured on a live window: `browser open` while a File
    /// Editor node had the keys put the page inside that node and drew neither.
    fn insert_beside_the_open_tab(&mut self, mut file: OpenFile) -> usize {
        let home = Home::Pane(self.last_pane.min(self.panes.saturating_sub(1)));
        file.home = home;
        let at = self
            .showing_at(home)
            .map(|index| index + 1)
            .unwrap_or(self.files.len())
            .min(self.files.len());
        self.files.insert(at, file);
        self.show(at);
        at
    }

    /// Close the tab at `index`.
    ///
    /// The tab that comes forward in its place is the one that was showing in that pane before it,
    /// which is what the reference editor does and what falls out of the stamps for nothing.
    ///
    /// A pane emptied by the close is removed and the panes after it move up. Closing the last tab
    /// of the last pane leaves a fresh untitled tab rather than no tabs, so there is never a window
    /// with nothing to type into and never a pane with nothing to draw.
    pub fn close(&mut self, index: usize) {
        if index >= self.files.len() {
            return;
        }
        let home = self.files[index].home;
        self.files.remove(index);
        // **Only the tabs in panes count**, because the window always has an editing area and a tab
        // living on a canvas node is not in it. Closing the last pane tab while a node still holds one
        // leaves a fresh untitled tab, exactly as closing the last tab of all always has.
        if !self.files.iter().any(|file| file.home.pane().is_some()) {
            self.files.push(OpenFile::new(Document::new()));
            self.panes = 1;
            self.focus = Home::Pane(0);
            self.last_pane = 0;
            self.widths = vec![1.0];
            self.tidy();
            return;
        }
        // The keyboard stays where the tab was closed while that place still has tabs; `tidy` moves it
        // along when the pane has gone.
        self.focus = home;
        if let Home::Pane(pane) = home {
            self.last_pane = pane;
        }
        self.tidy();
    }

    /// Forget what git said about every open file, which is what happens after an operation that
    /// could have changed any of them.
    pub fn forget_git(&mut self) {
        for file in &mut self.files {
            file.forget_git();
        }
    }

    /// Every open file's path, for a test and for the window's title.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.files.iter().filter_map(|file| file.path().map(Path::to_path_buf)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(name: &str) -> Document {
        let mut document = Document::from_text(&format!("in {name}\n"));
        let path = std::env::temp_dir().join("unluminous-open-files").join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("make the folder");
        std::fs::write(&path, format!("in {name}\n")).expect("write the file");
        document.save_as(&path).expect("save it");
        document
    }

    fn names(files: &OpenFiles) -> Vec<String> {
        files.iter().map(OpenFile::name).collect()
    }

    /// A tab opened to keep is never taken away by opening another file, and asking for one that is
    /// already open never turns it back into a preview.
    ///
    /// Written after a tab disappeared during a session driven through the MCP tools. The cause there
    /// was a `--permanent` flag being dropped on the way in, so the tab was a preview and the next
    /// file replaced it — which is correct behaviour for a preview and was not what had been asked
    /// for. This pins the rule underneath it, so the two can never be confused again: preview tabs are
    /// replaced, kept tabs are not, and `open` only ever upgrades.
    #[test]
    fn a_tab_opened_to_keep_is_not_replaced_by_the_next_file() {
        let mut files = OpenFiles::new(document("first.md"));
        files.open(document("kept.md"), true);
        files.open(document("preview.md"), false);
        assert_eq!(names(&files), ["first.md", "kept.md", "preview.md"]);

        // A preview is what the next preview replaces.
        files.open(document("another.md"), false);
        assert_eq!(
            names(&files),
            ["first.md", "kept.md", "another.md"],
            "the preview was reused and the kept tab was left alone"
        );

        // And asking for a file that is already open does not turn it into a preview.
        files.open(document("kept.md"), false);
        let kept =
            files.index_of(&std::env::temp_dir().join("unluminous-open-files").join("kept.md"));
        let kept = kept.expect("kept.md is still open");
        assert!(!files.at(kept).transient, "a kept tab stays kept");
        files.open(document("third.md"), false);
        assert!(
            names(&files).contains(&"kept.md".to_owned()),
            "and is still there afterwards: {:?}",
            names(&files)
        );
    }

    #[test]
    fn a_new_window_has_one_untitled_tab() {
        let files = OpenFiles::new(Document::new());
        assert_eq!(files.len(), 1);
        assert_eq!(files.active().name(), "untitled");
    }

    #[test]
    fn a_single_click_reuses_the_transient_tab_and_a_double_click_adds_one() {
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), false);
        assert_eq!(names(&files), vec!["one.md"], "the empty untitled tab was reused");
        files.open(document("two.md"), false);
        assert_eq!(names(&files), vec!["two.md"], "a second glance replaces the transient tab");
        files.open(document("three.md"), true);
        assert_eq!(names(&files), vec!["two.md", "three.md"], "a double click adds a tab");
        assert_eq!(files.active_index(), 1);
    }

    #[test]
    fn a_file_that_is_already_open_is_shown_rather_than_opened_twice() {
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), true);
        files.open(document("two.md"), true);
        assert_eq!(files.active_index(), 1);
        files.open(document("one.md"), true);
        assert_eq!(files.len(), 2, "two tabs on one file would be two documents over one path");
        assert_eq!(files.active_index(), 0);
    }

    #[test]
    fn typing_in_the_transient_tab_makes_it_permanent() {
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), false);
        assert!(files.active().transient);
        files.make_permanent(files.active_index());
        files.open(document("two.md"), false);
        assert_eq!(names(&files), vec!["one.md", "two.md"], "the tab that was typed into is kept");
    }

    #[test]
    fn a_new_tab_opens_beside_the_one_it_was_opened_from() {
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), true);
        files.open(document("two.md"), true);
        files.open(document("three.md"), true);
        files.show(0);
        files.open(document("four.md"), true);
        assert_eq!(names(&files), vec!["one.md", "four.md", "two.md", "three.md"]);
    }

    #[test]
    fn closing_the_last_tab_leaves_an_untitled_one() {
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), true);
        files.close(0);
        assert_eq!(files.len(), 1);
        assert_eq!(files.active().name(), "untitled");
    }

    #[test]
    fn closing_a_tab_shows_the_one_to_its_left() {
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), true);
        files.open(document("two.md"), true);
        files.open(document("three.md"), true);
        assert_eq!(files.active_index(), 2);
        files.close(2);
        assert_eq!(names(&files), vec!["one.md", "two.md"]);
        assert_eq!(files.active().name(), "two.md");
        files.close(0);
        assert_eq!(
            files.active().name(),
            "two.md",
            "closing a tab before the open one keeps it open"
        );
    }

    #[test]
    fn the_next_tab_wraps_round() {
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), true);
        files.open(document("two.md"), true);
        files.show(1);
        files.next();
        assert_eq!(files.active_index(), 0);
        files.previous();
        assert_eq!(files.active_index(), 1);
    }

    #[test]
    fn a_picture_takes_a_tab_by_the_same_rules_as_a_file_of_text() {
        let mut files = OpenFiles::new(Document::new());
        let path = std::env::temp_dir().join("unluminous-open-files").join("photo.png");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("make the folder");
        std::fs::write(&path, b"not really a png").expect("write it");

        files.open_file(OpenFile::picture(&path), true);
        assert_eq!(files.len(), 1, "the empty untitled tab was reused");
        assert!(files.active().is_picture());
        assert_eq!(files.active().name(), "photo.png");

        // Opening it again shows the tab it is already in rather than reading it twice.
        files.open(document("one.md"), true);
        files.open_file(OpenFile::picture(&path), true);
        assert_eq!(files.len(), 2);
        assert_eq!(files.active_index(), 0);
    }

    #[test]
    fn a_tab_that_held_a_picture_and_is_reused_for_text_stops_being_a_picture() {
        let mut files = OpenFiles::new(Document::new());
        let path = std::env::temp_dir().join("unluminous-open-files").join("reused.png");
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("make the folder");
        std::fs::write(&path, b"not really a png").expect("write it");
        files.open_file(OpenFile::picture(&path), false);
        assert!(files.active().is_picture());
        files.open(document("after.md"), false);
        assert!(!files.active().is_picture(), "the transient tab was reused for a file of text");
    }

    // ------------------------------------------------------------------------------- the panes

    /// Both invariants, asserted after every operation the pane tests do: the panes are numbered
    /// `0..pane_count` with no gaps, none of them is empty, the keyboard is in one that exists, and
    /// the widths are one a pane and sum to one.
    #[track_caller]
    fn invariants(files: &OpenFiles) {
        assert!(files.pane_count() >= 1, "there is always at least one pane");
        assert!(files.focused_pane() < files.pane_count(), "the keyboard is in a pane that exists");
        for pane in 0..files.pane_count() {
            assert!(!files.tabs_in(pane).is_empty(), "pane {pane} is empty");
        }
        for file in files.iter() {
            // A tab living on a canvas node is in no pane at all and is not counted by either
            // invariant - `task-1904`. That is what makes a File Editor node possible without a
            // second `OpenFiles`.
            let Some(pane) = file.home.pane() else { continue };
            assert!(pane < files.pane_count(), "a tab is in pane {pane} of {}", files.pane_count());
        }
        assert_eq!(files.pane_widths().len(), files.pane_count(), "one width a pane");
        let total: f32 = files.pane_widths().iter().sum();
        assert!((total - 1.0).abs() < 0.001, "the widths should sum to one, not {total}");
    }

    /// Two files open in one pane, which is where most of the pane tests start.
    fn two_open() -> OpenFiles {
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), true);
        files.open(document("two.md"), true);
        files
    }

    #[test]
    fn a_new_window_has_one_pane_holding_everything() {
        let files = two_open();
        assert_eq!(files.pane_count(), 1);
        assert_eq!(files.tabs_in(0), vec![0, 1]);
        invariants(&files);
    }

    #[test]
    fn splitting_moves_the_tab_that_is_showing_into_a_new_pane_on_the_right() {
        let mut files = two_open();
        assert_eq!(files.active().name(), "two.md");
        files.split_right();
        assert_eq!(files.pane_count(), 2);
        assert_eq!(files.focused_pane(), 1, "the keyboard follows the tab into the new pane");
        assert_eq!(names_in(&files, 0), vec!["one.md"]);
        assert_eq!(names_in(&files, 1), vec!["two.md"]);
        assert_eq!(files.active().name(), "two.md");
        invariants(&files);
    }

    #[test]
    fn splitting_a_pane_holding_one_tab_opens_an_empty_pane_beside_it() {
        // Taking the only tab out of a pane would empty the pane it came from and leave the window
        // looking exactly as it did, so the tab stays and the new pane starts empty.
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), true);
        files.split_right();
        assert_eq!(files.pane_count(), 2);
        assert_eq!(names_in(&files, 0), vec!["one.md"]);
        assert_eq!(names_in(&files, 1), vec!["untitled"]);
        assert_eq!(files.focused_pane(), 1);
        invariants(&files);
    }

    #[test]
    fn a_file_opened_after_a_split_lands_in_the_pane_with_the_keyboard() {
        let mut files = OpenFiles::new(Document::new());
        files.open(document("one.md"), true);
        files.split_right();
        // The new pane holds a fresh untitled tab, which the file takes over rather than sitting
        // beside.
        files.open(document("two.md"), true);
        assert_eq!(names_in(&files, 0), vec!["one.md"]);
        assert_eq!(names_in(&files, 1), vec!["two.md"]);
        invariants(&files);
    }

    #[test]
    fn splitting_halves_the_pane_that_was_split_and_leaves_the_others_alone() {
        let mut files = OpenFiles::new(Document::new());
        for name in ["one.md", "two.md", "three.md", "four.md"] {
            files.open(document(name), true);
        }
        files.split_right();
        files.show(0);
        files.split_right();
        // Two panes of a quarter each where the first was, and the half the second pane took stays
        // where it was.
        let widths = files.pane_widths().to_vec();
        assert_eq!(widths.len(), 3);
        assert!((widths[0] - 0.25).abs() < 0.001, "{widths:?}");
        assert!((widths[1] - 0.25).abs() < 0.001, "{widths:?}");
        assert!((widths[2] - 0.5).abs() < 0.001, "{widths:?}");
        invariants(&files);
    }

    /// Three files in one pane, which is what a rearrangement is done to.
    fn three_open() -> OpenFiles {
        let mut files = OpenFiles::new(Document::new());
        for name in ["one.md", "two.md", "three.md"] {
            files.open(document(name), true);
        }
        files
    }

    /// **A tab dragged along its own strip lands where the pointer left it.**
    #[test]
    fn a_tab_dragged_to_the_front_of_its_pane_goes_there() {
        let mut files = three_open();
        assert!(files.drag_tab(2, 0, 0));
        assert_eq!(names(&files), vec!["three.md", "one.md", "two.md"]);
        assert_eq!(files.active().name(), "three.md", "a tab dragged somewhere is shown there");
        invariants(&files);
    }

    /// **Dragging one to the right counts the tabs as they are on the screen.** Moving the first tab
    /// to position two means "past the second", which leaves it in the middle — not on the end,
    /// which is where a position not corrected for its own removal would put it.
    #[test]
    fn a_tab_dragged_along_its_own_strip_counts_the_tabs_it_passed() {
        let mut files = three_open();
        assert!(files.drag_tab(0, 0, 2));
        assert_eq!(names(&files), vec!["two.md", "one.md", "three.md"]);
        invariants(&files);
    }

    /// Past the end means the end.
    #[test]
    fn a_tab_dragged_past_the_last_one_goes_last() {
        let mut files = three_open();
        assert!(files.drag_tab(0, 0, 99));
        assert_eq!(names(&files), vec!["two.md", "three.md", "one.md"]);
        invariants(&files);
    }

    /// Dropped where it already was, nothing moves — but it is still shown, because a person who
    /// picked a tab up and put it back plainly means to be looking at it.
    #[test]
    fn a_tab_dropped_where_it_already_was_does_not_move() {
        let mut files = three_open();
        files.show(0);
        assert!(files.drag_tab(1, 0, 1));
        assert_eq!(names(&files), vec!["one.md", "two.md", "three.md"]);
        assert_eq!(files.active().name(), "two.md");
        invariants(&files);
    }

    /// **A tab dragged into another pane lands in it**, is shown there, and the keyboard follows.
    #[test]
    fn a_tab_dragged_into_another_pane_lands_in_it() {
        let mut files = three_open();
        files.split_right();
        assert_eq!(names_in(&files, 0), vec!["one.md", "two.md"]);
        assert_eq!(names_in(&files, 1), vec!["three.md"]);
        // one.md, which is tab zero, into the pane on the right, in front of three.md.
        assert!(files.drag_tab(0, 1, 0));
        assert_eq!(names_in(&files, 0), vec!["two.md"]);
        assert_eq!(names_in(&files, 1), vec!["one.md", "three.md"]);
        assert_eq!(files.focused_pane(), 1);
        assert_eq!(files.active().name(), "one.md");
        invariants(&files);
    }

    /// Dragging the last tab out of a pane takes the pane with it, exactly as `move_tab` does.
    #[test]
    fn dragging_the_last_tab_out_of_a_pane_takes_the_pane_with_it() {
        let mut files = two_open();
        files.split_right();
        assert_eq!(files.pane_count(), 2);
        assert!(files.drag_tab(1, 0, 0));
        assert_eq!(files.pane_count(), 1, "an emptied pane is removed");
        assert_eq!(names_in(&files, 0), vec!["two.md", "one.md"]);
        invariants(&files);
    }

    /// A tab that is not there, or a pane that is not there, is refused rather than clamped — the
    /// same rule `focus_pane` follows, so a command line that names a pane that is not there is told
    /// so instead of quietly doing something else.
    #[test]
    fn dragging_to_somewhere_that_is_not_there_is_refused() {
        let mut files = three_open();
        assert!(!files.drag_tab(9, 0, 0));
        assert!(!files.drag_tab(0, 4, 0));
        assert_eq!(names(&files), vec!["one.md", "two.md", "three.md"]);
        invariants(&files);
    }

    #[test]
    fn moving_the_last_tab_out_of_a_pane_takes_the_pane_with_it() {
        let mut files = two_open();
        files.split_right();
        assert_eq!(files.pane_count(), 2);
        // two.md is alone in the pane on the right, so moving it back leaves that pane empty.
        assert!(files.move_tab(false));
        assert_eq!(files.pane_count(), 1, "an emptied pane is removed");
        assert_eq!(names_in(&files, 0), vec!["one.md", "two.md"]);
        invariants(&files);
    }

    #[test]
    fn there_is_nothing_to_the_right_of_the_last_pane() {
        let mut files = two_open();
        assert!(!files.move_tab(true), "one pane has nothing beside it");
        assert!(!files.move_tab(false));
        invariants(&files);
    }

    #[test]
    fn unsplitting_folds_a_pane_into_the_one_on_its_left() {
        let mut files = two_open();
        files.open(document("three.md"), true);
        files.split_right();
        files.open(document("four.md"), true);
        assert_eq!(names_in(&files, 1), vec!["three.md", "four.md"]);
        assert!(files.unsplit());
        assert_eq!(files.pane_count(), 1);
        assert_eq!(names_in(&files, 0), vec!["one.md", "two.md", "three.md", "four.md"]);
        invariants(&files);
    }

    #[test]
    fn unsplitting_the_leftmost_pane_folds_it_into_the_one_on_its_right() {
        let mut files = two_open();
        files.split_right();
        files.focus_pane(0);
        assert!(files.unsplit());
        assert_eq!(files.pane_count(), 1);
        assert_eq!(names_in(&files, 0), vec!["one.md", "two.md"]);
        invariants(&files);
    }

    #[test]
    fn unsplit_all_puts_every_tab_back_in_one_pane() {
        let mut files = two_open();
        files.split_right();
        files.open(document("three.md"), true);
        files.split_right();
        assert_eq!(files.pane_count(), 3);
        assert!(files.unsplit_all());
        assert_eq!(files.pane_count(), 1);
        assert_eq!(files.len(), 3);
        assert!(!files.unsplit_all(), "there is nothing to unsplit with one pane");
        invariants(&files);
    }

    #[test]
    fn each_pane_walks_its_own_tabs() {
        let mut files = two_open();
        files.open(document("three.md"), true);
        files.split_right();
        files.open(document("four.md"), true);
        // The pane on the right holds three.md and four.md, and stepping through it never reaches
        // the two files in the pane on the left.
        assert_eq!(files.active().name(), "four.md");
        files.next();
        assert_eq!(files.active().name(), "three.md");
        files.next();
        assert_eq!(files.active().name(), "four.md");
        invariants(&files);
    }

    #[test]
    fn closing_the_tab_that_is_showing_brings_back_the_one_before_it() {
        let mut files = two_open();
        files.open(document("three.md"), true);
        files.show(0);
        files.show(2);
        // one.md was shown before three.md, so it is the one that comes forward.
        files.close(2);
        assert_eq!(files.active().name(), "one.md");
        invariants(&files);
    }

    #[test]
    fn closing_the_last_tab_in_a_pane_removes_the_pane_and_moves_the_keyboard() {
        let mut files = two_open();
        files.split_right();
        let showing = files.active_index();
        files.close(showing);
        assert_eq!(files.pane_count(), 1);
        assert_eq!(files.focused_pane(), 0);
        assert_eq!(files.active().name(), "one.md");
        invariants(&files);
    }

    #[test]
    fn the_keyboard_walks_the_panes_and_wraps_round() {
        let mut files = two_open();
        files.split_right();
        assert_eq!(files.focused_pane(), 1);
        files.next_pane();
        assert_eq!(files.focused_pane(), 0);
        files.previous_pane();
        assert_eq!(files.focused_pane(), 1);
        assert!(!files.focus_pane(9), "a pane that is not there is refused rather than clamped");
        invariants(&files);
    }

    #[test]
    fn showing_a_tab_puts_the_keyboard_in_its_pane() {
        let mut files = two_open();
        files.split_right();
        assert_eq!(files.focused_pane(), 1);
        let one = files.index_of(document("one.md").path().expect("a path")).expect("open");
        files.show(one);
        assert_eq!(files.focused_pane(), 0, "clicking a tab moves the keyboard to its pane");
        assert_eq!(files.active().name(), "one.md");
        invariants(&files);
    }

    #[test]
    fn a_divider_cannot_be_dragged_past_its_neighbour() {
        let mut files = two_open();
        files.split_right();
        files.move_divider(0, 5.0, 0.2);
        let widths = files.pane_widths().to_vec();
        assert!((widths[0] - 0.8).abs() < 0.001, "{widths:?}");
        assert!((widths[1] - 0.2).abs() < 0.001, "{widths:?}");
        invariants(&files);
    }

    #[test]
    fn a_state_file_that_asks_for_panes_that_would_be_empty_is_corrected() {
        let mut files = two_open();
        files.open(document("three.md"), true);
        // Pane 1 is named by nothing, so it cannot exist; what is asked for is two panes, not three.
        files.restore_panes(&[0, 2, 2], &[], 2);
        assert_eq!(files.pane_count(), 2);
        assert_eq!(names_in(&files, 0), vec!["one.md"]);
        assert_eq!(names_in(&files, 1), vec!["two.md", "three.md"]);
        invariants(&files);
    }

    #[test]
    fn a_pane_number_past_the_end_is_clamped_rather_than_refused() {
        let mut files = two_open();
        files.restore_panes(&[0, 1], &[0.3, 0.7], 7);
        assert_eq!(files.pane_count(), 2);
        assert_eq!(files.focused_pane(), 1);
        let widths = files.pane_widths().to_vec();
        assert!((widths[0] - 0.3).abs() < 0.001, "{widths:?}");
        invariants(&files);
    }

    #[test]
    fn what_the_panes_are_is_what_comes_back() {
        let mut files = two_open();
        files.split_right();
        assert_eq!(files.panes_of_tabs(), vec![0, 1]);
    }

    /// The names of the tabs in one pane, which is what the pane tests assert against.
    fn names_in(files: &OpenFiles, pane: usize) -> Vec<String> {
        files.tabs_in(pane).into_iter().map(|index| files.at(index).name()).collect()
    }
    /// Next Tab walks the tabs of whichever home has the keyboard, including a node's.
    ///
    /// Found by the Codex Sol review of `task-1905`: `step` read `last_pane`, which is where the keyboard was
    /// last in the *editing area* — so with a File Editor node focused, Next Tab moved an unrelated pane's
    /// tabs and the node's did not change.
    #[test]
    fn next_tab_walks_the_tabs_of_whichever_home_has_the_keyboard() {
        let mut files = OpenFiles::new(Document::new());
        files.open(Document::new(), true);
        // **Moved one at a time and looked up again**, because `move_to_node` can insert a fresh untitled
        // tab to keep the editing area's promise, and every index after it shifts.
        let one = files.open(Document::new(), true);
        files.move_to_node(one, 7);
        let two = files.open(Document::new(), true);
        files.move_to_node(two, 7);
        assert_eq!(files.tabs_in_node(7).len(), 2, "two tabs are on the node");

        // The keyboard in the node: Next Tab stays inside it.
        files.focus_node(7);
        let before = files.active_index();
        assert!(files.home_of(before).node() == Some(7), "the node's tab is showing");
        files.next();
        let after = files.active_index();
        assert_ne!(after, before, "it moved");
        assert_eq!(files.home_of(after).node(), Some(7), "and stayed on the node");

        // The keyboard in a pane: it walks that pane's, and never the node's.
        files.focus_pane(0);
        for _ in 0..4 {
            files.next();
            assert!(files.home_of(files.active_index()).pane().is_some(), "stayed in the panes");
        }
    }

    /// Four thousand random moves keep every invariant this file states.
    ///
    /// `task-1905` gave a tab a third kind of home — a File Editor node — and added
    /// [`OpenFiles::drag_tab_to_node`] beside `drag_tab`. Both remove from the vector and insert into it,
    /// which is where an index goes stale, and both can take the last tab out of a pane. The three
    /// invariants are the ones this file already promises: the editing area always has a tab, the panes are
    /// numbered `0..panes` with none empty, and `active_index` is in range.
    ///
    /// A walk rather than a case, because the interesting states are the ones nobody would think to write
    /// down: a node's last tab dragged into a pane that then has to be renumbered, a tab dropped where it
    /// already is, a close that empties a pane. The sequence is deterministic, so a failure is reproducible
    /// — which is `mermaid::layered`'s rule about anything a picture rests on.
    #[test]
    fn a_long_run_of_random_moves_keeps_every_invariant() {
        let mut files = OpenFiles::new(Document::new());
        for _ in 0..6 {
            files.open(Document::new(), true);
        }
        files.split_right();
        files.split_right();
        let mut seed: u64 = 0x1234_5678;
        let mut next = || {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as usize
        };
        for step in 0..4000 {
            let count = files.len();
            assert!(count > 0, "every tab was closed at step {step}");
            let index = next() % count;
            match next() % 4 {
                0 => {
                    let pane = next() % 4;
                    let position = next() % 4;
                    files.drag_tab(index, pane, position);
                }
                1 => {
                    let node = 100 + (next() % 3) as u64;
                    let position = next() % 4;
                    files.drag_tab_to_node(index, node, position);
                }
                2 => {
                    let node = 100 + (next() % 3) as u64;
                    files.move_to_node(index, node);
                }
                _ => files.close(index),
            }
            assert!(
                files.iter().any(|file| file.home.pane().is_some()),
                "the editing area was left with no tab at step {step}",
            );
            let panes = files.pane_count();
            for pane in 0..panes {
                assert!(!files.tabs_in(pane).is_empty(), "pane {pane} is empty at step {step}");
            }
            for file in files.iter() {
                if let Home::Pane(pane) = file.home {
                    assert!(pane < panes, "a tab is in pane {pane} of {panes} at step {step}");
                }
            }
            assert!(files.active_index() < files.len(), "nothing is showing at step {step}");
        }
    }
}
