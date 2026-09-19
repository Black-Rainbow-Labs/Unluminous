//! The Unluminous window.
//!
//! Holds the open document, the file explorer, the terminal, the fonts and the settings, and lays the
//! window out: a title bar Unluminous draws itself, which carries the menus, the project's name and the text
//! tools; a thin rail of pane buttons down the far left; the explorer beside it; the editing area filling
//! the rest; the terminal along the bottom when it is showing; and the status bar.
//!
//! Transparency works because the background and the text are two separate paints. `clear_color` gives the
//! operating system compositor an alpha taken from the opacity setting, so the desktop shows through the
//! window. Every glyph is painted at full alpha, so the writing stays sharp at every setting. That is the
//! whole of it on macOS; on Windows the compositor has to be talked into honouring the alpha at all, which
//! is `services::windows_transparency` and is the one platform call this file makes.
//!
//! The window has no operating system title bar, because rounded corners and transparency need the
//! decorations turned off, so the bars at the top and bottom are painted here and the top one moves the
//! window when it is dragged.
//!
//! Two rules this file keeps to, and a later change should keep to as well.
//!
//! Where the panels actually are is a **value** rather than the run of `let`s this file used to
//! spell out — `app::dock::Layout`, which edge each of the four is docked to and where in that edge,
//! turned into rectangles by `app::dock::regions`. What is described above is that value's default.
//! A change to the layout belongs in that module, where it can be tested with no window; this file
//! draws what it is handed. See `tasks/task-1697-panel-docking-tdd.md`.
//!
//! Every pane is resized by dragging its edge, through `components::splitter`. The explorer, the split
//! between the source and the preview, and the terminal all use it, and a new pane must use it too rather
//! than growing a divider of its own.
//!
//! Everything a menu or a keyboard shortcut can ask for is an `actions::Action`, and [`UnluminousApp::run_action`]
//! is the only place an action turns into a change. The menu bar inside the window, the macOS menu bar and a
//! test all go down that one path.

pub mod action_names;
pub mod actions;
mod breakpoints;
mod browsing;
pub mod cli;
pub mod code_editing;
pub mod completion;
pub mod debug;
pub mod dock;
mod drags;
mod editing;
mod explorer;
pub mod files;
mod find;
pub mod folding;
mod frame;
pub mod git;
pub mod hover_value;
mod menus;
mod modals;
mod opening;
mod panels;
// The window's side of the UI plugins: which providers are open, and which of their panes are showing.
pub mod plugin_panes;
mod preview;
mod running;
mod settings_changes;
pub mod space;
pub mod symbols;
mod terminals;
mod zooming;

use std::collections::HashMap;

use crate::services::symbol_index::Indexer;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Color32, Pos2, Rect, Vec2};
use unluminous_core::{Command, Document, Layout, Rgba};

use crate::app::debug::{DebugState, PendingBuild};
use crate::components::about_dialog::About;
use crate::components::debug_dialogs::{BreakpointDialog, EvaluateDialog};
use crate::components::debug_panel::DebugPanel;
use crate::components::file_tabs::{self};
use crate::components::find_in_files::FindInFiles;
use crate::components::go_to_file::GoToFile;
use crate::components::prompt_dialog::Prompt;
use crate::components::references::References;
use crate::components::run_dialog::RunDialog;
use crate::components::run_panel::RunPanel;
use crate::components::run_widget;
use crate::components::settings_dialog::SettingsWindow;
use crate::components::status_bar;
use crate::components::terminal_panel::TerminalPanel;
use crate::components::text_menu;
use crate::components::title_bar::MenuPlacement;
use crate::services;
use crate::services::breakpoint_store::BreakpointStore;
use crate::services::browser::{BrowserHost, BrowserPlacement};
use crate::services::control;
use crate::services::debuggers;
use crate::services::file_clipboard::FileClipboard;
use crate::services::file_kind;
use crate::services::file_marks::FileMarks;
use crate::services::file_move;
use crate::services::file_tree::FileTree;
use crate::services::icons::Icons;
use crate::services::mermaid_scene::MermaidScenes;
use crate::services::native_menu::NativeMenu;
use crate::services::plugins::Plugins;
use crate::services::preview_images::PreviewImages;
use crate::services::project_state::{self, ProjectState};
use crate::services::run_configurations::{self, RunConfigurations};
use crate::services::store::Store;
use crate::services::text_renderer::TextRenderer;
use crate::settings::{self, Panes, Settings};
use crate::theme::{self, color};

use files::OpenFiles;
use git::GitState;

/// How opaque the background is when Unluminous starts.
pub const DEFAULT_OPACITY: f32 = settings::DEFAULT_OPACITY;

/// How much bigger a pinch has to ask for before the editor's font moves a size.
///
/// The smallest gap between two of the sizes `Edit -> Settings -> Appearance -> Font` offers, which
/// is 11 to 13. A gesture asking for that much gets one size; one asking for twice as much gets two.
/// A ratio rather than a number of points, because what one notch of a wheel is worth in points is
/// a platform's business — measured on this machine it is about 55, which is a third bigger than the
/// number egui's own default assumes — and the ratio a gesture is asking for is the same everywhere.
const ZOOM_STEP: f32 = 1.18;

/// How tall the panel is that stands in for a diagram that could not be drawn.
///
/// Enough for the reason, the line it was on, and a few lines of the source under it. A fixed height
/// rather than one worked out from the source, because a document being typed into would otherwise
/// jump about as the panel grew and shrank with every keystroke.
const PROBLEM_HEIGHT: f32 = 160.0;

/// How far a code panel is drawn outside the text it sits behind.
const PANEL_PADDING: f32 = 6.0;

/// How much air is left under a picture in the Markdown preview.
///
/// The same idea as the space between two paragraphs: a picture with the next line of prose against
/// its bottom edge reads as part of the picture.
const PICTURE_GAP: f32 = 14.0;

/// A question with two answers, and what to do when it is answered.
///
/// The dialog knows nothing about what it is asking. What is held is the [`Answer`] — the thing to
/// do when the button is pressed — so a seventh question can be added without the one confirmation
/// dialog learning a seventh thing.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirmation {
    pub title: String,
    pub note: String,
    /// The word on the button that does it.
    pub button: String,
    pub answer: Answer,
}

/// What confirming a question does.
///
/// Two, because everything Unluminous used to ask about first was something git could not undo, and
/// `task-1681` added the one thing that is not: throwing a file away.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// Send this to the git thread.
    Git(unluminous_git::worker::Request),
    /// Delete this path, wherever `services::recycle` puts a deleted file on this platform.
    Delete(PathBuf),
    /// Stop this run configuration and take it away, which is what `Remove` in the run dialog does
    /// to one that is still running. The question is asked because silently killing a server
    /// somebody is watching is worse than one extra click.
    RemoveRun(String),
}

/// Which of the three ways of looking at a Markdown file is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// The Markdown source, as it is on disk. This is the only mode that can be typed into.
    #[default]
    Raw,
    /// The source on the left and the preview on the right.
    SideBySide,
    /// The preview filling the editing area.
    Preview,
}

impl ViewMode {
    pub const ALL: [ViewMode; 3] = [ViewMode::Raw, ViewMode::SideBySide, ViewMode::Preview];

    /// The name a test asks for the button by, and what assistive technology reads out.
    ///
    /// Markdown's wording, which is what it has always been and what every existing test asks for.
    /// A file whose preview is a diagram uses [`Self::label_for`] instead.
    pub fn label(&self) -> &'static str {
        self.label_for(file_kind::PreviewKind::Markdown)
    }

    /// The name, by what kind of preview the open file has.
    ///
    /// `task-1660` gives a `.mmd` file the same three modes a `.md` file has, and a button over a
    /// Mermaid diagram that said `Markdown preview` would be a small wrongness a reader notices at
    /// once. `Side by side` is the same word in both, which is not two controls sharing a name:
    /// only one file is open at a time, so the two are never on the screen together.
    pub fn label_for(&self, kind: file_kind::PreviewKind) -> &'static str {
        match (self, kind) {
            (ViewMode::Raw, file_kind::PreviewKind::Markdown) => "Raw Markdown",
            (ViewMode::Raw, file_kind::PreviewKind::Mermaid) => "Raw Mermaid",
            (ViewMode::SideBySide, _) => "Side by side",
            (ViewMode::Preview, file_kind::PreviewKind::Markdown) => "Markdown preview",
            (ViewMode::Preview, file_kind::PreviewKind::Mermaid) => "Mermaid diagram",
        }
    }

    pub fn description(&self) -> &'static str {
        self.description_for(file_kind::PreviewKind::Markdown)
    }

    /// What the pointer resting on the button says, by what kind of preview the file has.
    pub fn description_for(&self, kind: file_kind::PreviewKind) -> &'static str {
        match (self, kind) {
            (ViewMode::Raw, file_kind::PreviewKind::Markdown) => {
                "Raw Markdown: the source as it is on disk"
            }
            (ViewMode::Raw, file_kind::PreviewKind::Mermaid) => {
                "Raw Mermaid: the source as it is on disk"
            }
            (ViewMode::SideBySide, file_kind::PreviewKind::Markdown) => {
                "Side by side: the source on the left, the preview on the right"
            }
            (ViewMode::SideBySide, file_kind::PreviewKind::Mermaid) => {
                "Side by side: the source on the left, the diagram on the right"
            }
            (ViewMode::Preview, file_kind::PreviewKind::Markdown) => {
                "Markdown preview: the rendered document"
            }
            (ViewMode::Preview, file_kind::PreviewKind::Mermaid) => {
                "Mermaid diagram: the drawn diagram"
            }
        }
    }

    /// True when the source is shown, which is the only time there is anything to type into.
    pub fn shows_source(&self) -> bool {
        matches!(self, ViewMode::Raw | ViewMode::SideBySide)
    }

    pub fn shows_preview(&self) -> bool {
        matches!(self, ViewMode::SideBySide | ViewMode::Preview)
    }
}

/// One picture in the Markdown preview, ready to draw.
///
/// The preview is laid out by the ordinary layout engine, which knows about glyphs and not about
/// pictures. So a picture is a paragraph with no text in it that has been asked to be at least as
/// tall as the picture is drawn, and this is what the window paints into that room. Held between
/// frames because a preview is redrawn sixty times a second and a photograph is decoded once.
pub struct PlacedPicture {
    /// Which paragraph of the preview it belongs to, which is what says where it goes.
    pub paragraph: usize,
    /// How large it is drawn, in points.
    pub size: Vec2,
    /// The picture, or `None` when it could not be read — in which case the alt text is drawn.
    pub texture: Option<egui::TextureHandle>,
    pub alt: String,
}

/// One diagram in the Markdown preview, laid out and ready to draw.
///
/// The same shape as [`PlacedPicture`] and for the same reason: `unluminous_core::markdown` says which
/// paragraph stands in for the diagram, and this says how large it came out and what it holds. Held
/// between frames because a preview is redrawn sixty times a second and a diagram is laid out once.
pub struct PlacedDiagram {
    /// Which paragraph of the preview it belongs to.
    pub paragraph: usize,
    /// How large it is drawn, in points.
    pub size: Vec2,
    /// The diagram, or the reason it could not be drawn — which is shown in its place, because a
    /// mistake in one diagram must not take the rest of the document away.
    pub laid: crate::services::mermaid_scene::Laid,
    /// What was between the fences, so a problem can show it under the reason.
    pub source: String,
}

/// How many frames the explorer scrolls to the file that is showing after it changes. See
/// `UnluminousApp::reveal_in_explorer` for why it is not one.
const REVEAL_FRAMES: u8 = 2;

/// What the keyboard is talking to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    /// The document. Typing edits the file.
    #[default]
    Editor,
    /// The explorer. The arrow keys walk the tree and `Delete` throws a file away.
    ///
    /// `task-1681` added it, because `Delete` cannot mean "throw this file away" while the editing
    /// area has the keys — there it means "take away the letter in front of the caret". A single
    /// click on a row leaves the keyboard here, which is what VS Code does and what makes `Down`
    /// `Down` `Down` a way to look through a folder; a double click hands it to the editor.
    Explorer,
    /// The terminal. Typing goes to the program running in it, and Tab and Escape go with it.
    Terminal,
    /// The Base of Infinite Space. Typing goes to whichever node is chosen - `task-1904`.
    ///
    /// A sixth holder rather than a flag on the canvas, for the reason `Focus::Plugin` is a fifth: `Focus`
    /// is the one value that says who has the keyboard, and a canvas that kept its own would leave the
    /// editing area holding the keys as well, so one press would reach both.
    Space,
    /// A plugin's own pane or tab. Typing goes to whatever it has that takes keys.
    ///
    /// A fifth holder rather than a flag on the plugin, because `Focus` is the one value that says who has the
    /// keyboard: a plugin that kept its own left the editing area holding the keys as well, so one press reached
    /// both. `Request::TakeTheKeyboard` is what asks for it.
    Plugin,
}

/// Whether one pane is filling the window, and what to put back when it stops.
///
/// `task-1771`: *"I should be able to double click anywhere in the top of a pane to get it to maximize,
/// then Esc or double click to put it back to the size it was."*
///
/// **Maximising is putting everything else away, and that is the whole of it.** Unluminous already has a way for
/// a panel not to be showing, and `dock::regions` already gives the room to whatever is left — hiding the
/// editing area gives it to the panels, and one panel on one side fills the window. So a maximised pane is
/// not a fifth kind of layout with its own arithmetic to get wrong; it is the layout there already is, with
/// one thing switched on and the rest switched off, and everything downstream of that — the dividers, the
/// rail's lit buttons, `panel list`, the drop bands — reports the truth without being told.
///
/// What has to be remembered is therefore what was showing, so that Escape can put it back. It is not
/// written to the settings file: a window that opened maximised with no memory of what it was hiding would
/// be a window somebody has to reassemble by hand.
///
/// Two fields on `UnluminousApp` are about the maximise and are deliberately **not** states of it.
/// `maximise_wanted` is a request from a tab strip's double click, and it arrives while a pane is already
/// filling the window — that is how a second double click restores — so a variant for it would lose the
/// arrangement `Filling` is holding. `settling_the_maximise` is a re-entrancy guard, and what it really
/// guards is written down beside it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Maximise {
    /// Nothing is filling the window.
    No,
    /// One pane is, and this is the arrangement `Escape` puts back.
    Filling {
        /// The pane that is showing. `None` means the editing area, which is not a panel.
        pane: Option<dock::Panel>,
        /// What was showing before, in `dock::Panel::index` order, and whether the editing area was.
        editor: bool,
        panels: [bool; dock::SLOTS],
        /// And who had the keyboard, because putting a panel back **takes** it: `show_the_terminal_tile`
        /// hands the keys to the terminal whenever it is shown, which is right when somebody presses the
        /// terminal's own button and wrong when a restore happens to bring it back. Without this, maximising
        /// the explorer and pressing Escape left the caret in a terminal nobody had asked for.
        focus: Focus,
    },
}

/// Who has this frame's zoom gesture.
///
/// A gesture belongs to the window rather than to a pane, because the size is one setting for the whole
/// window, and the pointer says which pane it is about. Every pane used to ask the same `zoom_delta` for
/// itself, so with the editing area split the size stepped once for each pane: one notch of the wheel took
/// sixteen points to thirty two.
///
/// It was two flags, and they spelled four combinations of which one could not mean anything: a pane with
/// the keyboard offers, a later pane finds the pointer and takes, and the only reader asked whether the
/// gesture was offered **and** unclaimed. Three states are what can happen.
///
/// `zoom_pending` is not part of this. That is the fractional accumulator, and it deliberately survives the
/// frame — a slow wheel is worth a whole step eventually.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZoomClaim {
    /// Nobody has claimed it.
    Nobody,
    /// The pane with the keyboard is willing to take it but has no pointer over it, which is settled at
    /// the end of the frame — a pinch with the pointer over the explorer or the terminal still zooms the
    /// pane a person is typing into.
    OfferedToTheKeyboard,
    /// A pane has it, or a modal took it before anything could claim it.
    Taken,
}

/// Something being carried by the pointer, until the frame settles where it landed.
///
/// `task-1673` asks for the reference editor's two tab gestures: rearranging the tabs in a pane, and dragging a
/// tab from one pane into another. Both are one gesture, and where it ends is not a question a strip of
/// tabs can answer — each pane draws its own strip and has never heard of the others, while the pointer
/// wanders freely between them. So the strip reports **that** a tab is being carried and where the pointer
/// is, and `UnluminousApp::settle_the_tab_drag` decides where it landed once every pane has said where its own
/// tabs are. A file carried out of the explorer or a Folder node, and a whole panel carried to another edge
/// of the window, are the same shape for the same reason.
///
/// `at` is always in the **window's** own points, converted by whichever list reported it.
///
/// One type for all three, where there were three structs differing only in what they carry. What it makes
/// unrepresentable is nothing in the air and yet let go: an `Option` around a struct with a `dropped` flag
/// in it spells four states and three can happen.
///
/// **There are still three fields, and that is measured rather than tidy.** They are cleared at three
/// different points in one frame — the panel drag at the top, the tab drag before the pane loop, the file
/// drag when it is settled — and two of them can be in the air on the same frame: the explorer reports a
/// row being carried and a header being grabbed out of the same block, and a Folder node reports a row
/// after a plugin pane has reported a header. One field would drop one of them.
#[derive(Debug, Clone, PartialEq)]
enum Drag<T> {
    /// Nothing is in the air.
    Nothing,
    /// Being carried, and the pointer is here.
    Carrying { what: T, at: Pos2 },
    /// Let go on this frame, here.
    LetGo { what: T, at: Pos2 },
}

impl<T: Clone> Drag<T> {
    /// What is in the air: what it is, where the pointer is, and whether it was let go on this frame.
    ///
    /// Hands back an **owned** payload rather than a borrow, so a reader is free to call a `&mut self`
    /// method with the answer still in hand — which every one of them does, because settling a drag is
    /// moving a tab, a file or a panel. A `PathBuf` cloned once a frame while a row is being carried costs
    /// nothing worth measuring.
    fn in_the_air(&self) -> Option<(T, Pos2, bool)> {
        match self {
            Drag::Nothing => None,
            Drag::Carrying { what, at } => Some((what.clone(), *at, false)),
            Drag::LetGo { what, at } => Some((what.clone(), *at, true)),
        }
    }

    /// The same answer, leaving nothing behind. For the one reader that clears as it reads.
    fn take(&mut self) -> Option<(T, Pos2, bool)> {
        let was = std::mem::replace(self, Drag::Nothing);
        was.in_the_air()
    }
}

impl<T> Drag<T> {
    /// What a list reported: in the air at `at`, and let go if the pointer let go on this frame.
    pub(crate) fn carrying(what: T, at: Pos2, dropped: bool) -> Self {
        match dropped {
            true => Drag::LetGo { what, at },
            false => Drag::Carrying { what, at },
        }
    }
}

/// True when one of the window's text boxes has the keyboard: the explorer's filter, the commit
/// message, the rename prompt, the plugin search or the settings search.
///
/// This is the other half of what [`Focus`] means. `Focus` says whether the editing area or the
/// terminal is the one being typed into; this says whether either of them is being typed into at
/// all. Both have to be asked, because egui does **not** take the events a `TextEdit` consumed out
/// of `input.events` — the list is the frame's input and every reader sees all of it. The editing
/// area and the terminal read it directly, so without this question they take the same key presses
/// the box has just taken, and typing a filter also types into the file behind it.
///
/// `text_edit_focused` is asked rather than `egui_wants_keyboard_input`, which is
/// `memory.focused().is_some()` and is true of **any** focusable widget. Every control Unluminous draws
/// with `Sense::click` is focusable, so the broader question would stop the document being typed
/// into after a button was reached with Tab. Only a box that takes text should take the keyboard
/// away, and in egui only `TextEdit` and `DragValue` ask for focus when they are clicked.
pub fn text_box_has_the_keyboard(ctx: &egui::Context) -> bool {
    ctx.text_edit_focused()
}

/// True while a modal is open, which is what makes a key press the modal's rather than a pane's.
///
/// The other half of the same problem [`text_box_has_the_keyboard`] answers, and it needed its own
/// answer once `task-1682` gave Enter a meaning in every modal: a confirmation, an about box and
/// most of the git dialogs have no field in them, so nothing had the focus and the editing area,
/// the terminal and the explorer all went on reading the frame's keys behind the dialog. `Enter` in
/// the delete confirmation would then have deleted the file **and** opened the row under the
/// explorer's cursor.
///
/// egui's own modal layer is what is asked, rather than a list of Unluminous's dialogs, so a modal added
/// later is covered without being added anywhere. It is the layer as it stood at the **end of the
/// last frame**, which is the honest answer at the point in a frame these three read the keyboard —
/// before anything is drawn, and so before this frame's modal has said it is there.
pub fn a_modal_has_the_keyboard(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| memory.top_modal_layer().is_some())
}

/// The same question, asked by something that is **itself inside a modal**.
///
/// A character grid drawn inside a dialog — the ticket modal's terminal — has to take the keys, and
/// [`a_modal_has_the_keyboard`] said no to it because a modal was open: its own. The layer settles it. Anything
/// outside the top modal still stands aside, which is what keeps the editing area and the terminal tile from
/// reading a key press aimed at a dialog on top of them.
pub fn another_modal_has_the_keyboard(ctx: &egui::Context, mine: egui::LayerId) -> bool {
    ctx.memory(|memory| match memory.top_modal_layer() {
        Some(top) => top != mine,
        None => false,
    })
}

/// The name of the widget that holds egui's keyboard focus while a pane of Unluminous's own is being typed
/// into.
pub const KEYBOARD_HOLDER: &str = "unluminous-keyboard-holder";

/// How long the window will sleep before it wakes itself up, whether or not anything has asked it to.
///
/// Unluminous draws only when something happens: a key press, the pointer, a program writing to a
/// terminal, a thread finishing its work. When none of those is happening it asks for no frame and
/// the operating system puts the main thread to sleep until an event arrives. Everything that wakes
/// it from another thread — the command line, the git worker, the symbol indexer, every terminal —
/// does so through egui's `request_repaint`, which on macOS signals a source on the main run loop.
///
/// A window was found in the state where that wake had stopped arriving. It was visible on the
/// screen, it was using no processor time, its main thread was asleep waiting for an event, no lock
/// was held by anybody and nothing was deadlocked, a command from the command line was queued with
/// its connection still waiting for the answer, and not one frame had been drawn in three seconds.
/// It could not be typed into and it could not be dragged. It drew a single frame each time the
/// operating system pushed something at it, such as being activated or the desktop it was on being
/// switched to. Nothing had crashed; the window was asleep and the wake never came.
///
/// The window therefore no longer depends on being woken. Every frame asks for another one half a
/// second later, so there is always a timer pending, and a wake that goes missing costs half a second
/// instead of the window. The cost of that is two frames a second while nothing is happening.
///
/// The wake this schedules is a different mechanism from the one that went missing. Asking for a frame
/// after a delay makes winit wait on a timer, which the run loop fires from inside itself; waking from
/// another thread signals a source and hopes the sleep breaks. The timer is the one that can be
/// counted on, which is why the heartbeat is a delay rather than a thread calling `request_repaint`.
pub const HEARTBEAT: std::time::Duration = std::time::Duration::from_millis(500);

/// How often an open plugin is given a turn on the clock.
///
/// Two minutes, which is what the board being replaced runs its watchdog on, and it is the number that
/// decides how long an agent that has stopped waits to be nudged. Nothing happens in between: a frame
/// inside the interval costs one comparison.
pub const PLUGIN_TICK: std::time::Duration = std::time::Duration::from_secs(120);

/// How long an adapter search is believed before it is done again.
///
/// It exists because the commonest thing to happen next, when there is no adapter, is an install
/// running in the tile beside the message that offered it — so the message has to notice. Five
/// seconds is short enough that a finished install is reflected while somebody is still looking at
/// it, and long enough that a few directory reads are not part of drawing a frame.
pub const ADAPTER_SEARCH_TTL: std::time::Duration = std::time::Duration::from_secs(5);

/// How often the folders that are showing in the explorer are asked whether they have changed.
///
/// `task-1693` reported that a file an agent made never appeared. Three quarters of a second is
/// quick enough that a file written by a program in the terminal tile below is in the tree before
/// anybody has looked away, and slow enough that the handful of `metadata` calls it costs are a few
/// dozen a second. See `FileTree::changed_on_disk`, which is where the argument for asking rather
/// than watching is written down.
///
/// It is asked at the top of a frame like everything else, and `HEARTBEAT` is what guarantees there
/// is a frame: an idle window still draws twice a second.
pub const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(750);

/// Keep egui's keyboard focus on a widget of Unluminous's own, so that pressing `Tab` or an arrow key
/// cannot hand the keyboard to a button.
///
/// **This is what a report of Unluminous crashing while somebody typed turned out to be.** egui moves
/// keyboard focus when a bare `Tab` or a bare arrow key is pressed, and it moves it to the next
/// widget that can take focus — every control Unluminous draws with `Sense::click` can. The editing area,
/// the explorer and the terminal are not egui widgets and never held egui's focus, so the focus
/// walked out of the document and onto the first button in the window, which is `Close` in the title
/// bar. A button with the keyboard is pressed by `Space` and by `Enter`. One arrow key and a space
/// therefore closed the window, or minimised it if the focus had reached `Minimise` instead. Nothing
/// panicked and nothing was written down, because the window was asked to close in the ordinary way.
///
/// egui's answer to this is a focused widget that says which keys are its own, which is how a
/// `TextEdit` keeps `Tab` from moving the focus out of a text box. Unluminous has three surfaces that read
/// the keyboard themselves rather than through a widget, so one focusable widget stands for all three:
/// while it holds the focus, `Tab` and the arrows are claimed and egui moves the focus nowhere.
///
/// It senses no clicks, so `Space` and `Enter` cannot press it, and it has no size, so there is
/// nothing to draw and nothing to click on.
///
/// It gives the focus up while a text box or a modal has the keyboard, because there the keys really
/// are the widget's: a `Tab` in the commit message is the box's, and a modal's buttons are the only
/// things a key should reach while it is open. Anything else holding the focus — a button in the
/// toolbar, a row in the explorer, a file tab — is a focus that arrived by wandering, and the holder
/// takes it back on the frame after. That leaves the one frame between the `Tab` that moved the focus
/// and the holder taking it back, and a person's next key press is frames later.
///
/// A click on a text box is never stolen: on the frame it is clicked the holder already has the focus
/// and so asks for nothing, and the box's own request is the one that takes effect.
pub fn hold_the_keyboard(ui: &mut egui::Ui) {
    // A field whose own padding was pressed last frame takes the keyboard now.
    //
    // It cannot be given the keyboard on the frame of the press, and that is egui's rule rather than
    // a choice: egui surrenders a widget's focus when a press lands anywhere that widget is not
    // hovered, and the box inside a field is not hovered when the press was in the margin round it —
    // so a focus handed over at the moment of the press was taken straight back a few lines later,
    // when the box itself was created. `controls::claim_the_field` records the id and this is where
    // it is acted on, because this runs before anything is drawn and is already the one place the
    // window decides who holds the keys. `task-1795`.
    let slot = crate::components::controls::wants_the_keyboard();
    let wanted = ui.ctx().data_mut(|data| {
        let found = data.get_temp::<egui::Id>(slot);
        data.remove::<egui::Id>(slot);
        found
    });
    if let Some(wanted) = wanted {
        ui.memory_mut(|memory| memory.request_focus(wanted));
    }
    let id = egui::Id::new(KEYBOARD_HOLDER);
    if text_box_has_the_keyboard(ui.ctx()) || a_modal_has_the_keyboard(ui.ctx()) {
        ui.memory_mut(|memory| memory.surrender_focus(id));
        return;
    }
    let response = ui.interact(Rect::ZERO, id, egui::Sense::focusable_noninteractive());
    if ui.memory(|memory| memory.focused()) != Some(id) {
        response.request_focus();
    }
    // Claimed every frame rather than once: egui only lets a widget claim keys on a frame where it
    // both had the focus last frame and holds it now, so the frame it takes the focus on is a frame it
    // cannot yet claim anything on.
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            id,
            egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                // Escape is left alone: it hands the keyboard back, and the focus is taken again on
                // the frame after by the request above.
                escape: false,
            },
        );
    });
}

/// The project's definitions, and whether what is indexed is still what is on the disk.
///
/// Nothing is started until something asks a question about a symbol, so a window a unit test builds has
/// no thread reading a folder behind it.
///
/// Three fields before, and two of them could not disagree: an `Indexer` with nothing recorded about what
/// it was asked for, and a record of what it was asked for with no indexer behind it. Both are written in
/// the same two lines of `keep_the_symbol_index_fresh` and neither can happen.
pub(crate) enum SymbolIndexState {
    /// Nothing has asked, so no thread has been started.
    NotStarted,
    /// The index, and the project it was read for.
    Reading {
        /// Boxed because it is far bigger than the variant beside it, and an enum whose two
        /// variants differ that much is one every holder pays for. One allocation, the first time
        /// anything asks a question about a symbol.
        indexer: Box<Indexer>,
        /// What the index was last asked about: the project, how many files were in it, and how many
        /// plugins were switched on. A change to any of the three is what asks for another read.
        asked: (PathBuf, usize, usize),
        /// Set when a file on the disk changed under the index — a save, a rename, a reload.
        stale: bool,
    },
}

impl SymbolIndexState {
    /// The index, once something has asked for one.
    pub(crate) fn indexer(&self) -> Option<&Indexer> {
        match self {
            SymbolIndexState::NotStarted => None,
            SymbolIndexState::Reading { indexer, .. } => Some(indexer),
        }
    }

    /// Whether the disk has moved since the index was read.
    ///
    /// An index nobody has started is never stale: there is nothing to be stale, and the next question
    /// rebuilds it anyway because it has no record of having been asked.
    pub(crate) fn is_stale(&self) -> bool {
        matches!(self, SymbolIndexState::Reading { stale: true, .. })
    }
}

/// Where each part of the window is on this frame.
///
/// Worked out once, by [`UnluminousApp::lay_the_frame_out`], and handed to every phase after it. The
/// phases are methods rather than a run of statements in one function, and a rectangle each of them
/// measured for itself would be a second answer to a question that has one.
///
/// The fields are named for the locals they were, so a phase binds the two or three it needs at the
/// top of its body and the rest of the body is what it always was.
struct FramePlaces {
    /// The whole window, rounded corners and all.
    full: Rect,
    /// The title bar across the top, and the three things drawn over its right hand end.
    title_rect: Rect,
    tools_rect: Rect,
    run_rect: Rect,
    tools_width: f32,
    run_width: f32,
    /// What the run widget draws itself from. Worked out before the rectangles, because how much
    /// room it wants is what the text tools are measured back from.
    run_state: run_widget::WidgetState,
    /// The status bar along the bottom and the rail of pane buttons down the left.
    status_rect: Rect,
    rail_rect: Rect,
    /// Everything between the rail and the window's edges, which is what `dock::regions` divides up.
    panes: Rect,
    /// What it divided it into. A tile's rectangle is recorded whether or not it is showing - see
    /// `run_grid_size` for what that is worth.
    explorer_rect: Rect,
    terminal_rect: Rect,
    run_rect_tile: Rect,
    debug_rect: Rect,
    editing_area: Rect,
}

pub struct UnluminousApp {
    /// The files that are open, one to a tab, and which of them is showing.
    pub files: OpenFiles,
    /// How many times a file has actually been laid out, which is the one way to tell a tab switch
    /// that reused a cached layout from one that rebuilt it.
    ///
    /// `refresh_layout` returns early whenever the text, the folds and the width are all unchanged,
    /// so switching back to a tab that has already been laid out costs nothing - but that is a thing
    /// no state in the window could otherwise be asked about, and
    /// `tasks/task-1813-performance-review-tdd.md` makes it an acceptance threshold. A counter is
    /// what turns it into a test, which is the shape `DebugState::reads` already has.
    layouts_built: u64,
    /// Set when a plugin asked for another frame, which a pane with a terminal in it does while that
    /// terminal is printing. Read and cleared once at the end of the frame.
    pub(crate) plugin_wants_a_repaint: bool,
    /// What a plugin asked to have put on the clipboard, which its terminal's copy does. Read and cleared
    /// once at the end of the frame, because the context is not at hand where the request is acted on.
    pub(crate) plugin_wants_copied: Option<String>,
    /// When the plugins were last given a turn on the clock. `None` until the first frame.
    plugins_ticked_at: Option<std::time::Instant>,
    /// The plugins that draw: which providers are open, and which of their panes are showing.
    ///
    /// One value rather than a provider held per contribution, so the rail, the dock, the tab strip, the
    /// menus and the Settings window all ask the same thing what is contributed.
    pub plugin_ui: plugin_panes::PluginUi,
    /// The last project and open file every provider was told about, so it is told only when it
    /// changes. See `tell_the_plugins_what_is_showing`.
    told_the_plugins: Option<(Option<PathBuf>, Option<PathBuf>)>,
    /// The Base of Infinite Space: the canvas, what is running on it, and what is being dragged.
    ///
    /// `task-1904`. Core rather than a plugin because three of its four node kinds need `OpenFiles`, a
    /// `Document` and the one native browser child, and a provider can reach none of the three.
    pub space: space::SpaceState,
    /// One canvas per plugin surface that draws decoration `egui` cannot.
    ///
    /// The soft shadows, inset shadows and gradients of `services::vello_canvas`, rasterised only on the
    /// frame a board changes and painted as one texture behind the pane's own widgets. Empty until a
    /// plugin asks for it, so a window with no such plugin carries no pixmap.
    pub canvases: crate::services::vello_canvas::Canvases,
    /// Native browser views and the shared WebView2 or WKWebView environment behind rendered tabs.
    pub browser: BrowserHost,
    /// Browser child rectangles reported by the panes in this frame.
    browser_placements: Vec<BrowserPlacement>,
    /// Where the native browser views were asked to go last frame, for a test.
    ///
    /// **A native child is a real window and nothing Unluminous draws**, so no screenshot holds one and no
    /// state the window reports said where it had been put. `task-1905` reported a node's page drawn up and
    /// to the left of its node, and this is the only thing a test can assert on: the rectangle the view was
    /// handed, in the window's own points.
    pub tree: FileTree,
    pub renderer: TextRenderer,
    /// The font and the background, as chosen in `Edit -> Settings`.
    pub settings: Settings,
    /// Where the draggable dividers were left.
    pub panes: Panes,
    /// The text in the explorer's filter box.
    pub filter: String,
    /// False when the explorer has been hidden.
    pub explorer_visible: bool,
    /// False when the editing area — the pane to the right of the explorer, holding the tabs — has been hidden.
    ///
    /// `task-28`. Hiding it gives the whole width to the panels that are showing. It can never leave an empty
    /// window: see `Action::ToggleEditor` in `run_action`.
    pub editor_visible: bool,
    /// The Settings modal.
    pub settings_window: SettingsWindow,
    /// The terminal along the bottom.
    pub terminal: TerminalPanel,
    /// The run tile along the bottom, which is the terminal tile's sibling: the window shows one of
    /// the two and never both, because two grids stacked take the editing area below the fold.
    pub run: RunPanel,
    /// The project's run configurations, as `.unluminous/run-configurations.conf` holds them plus
    /// whatever has been run without being kept. See `services::run_configurations`.
    pub run_configurations: RunConfigurations,
    /// The name the run widget has chosen, which is what `Run` with no name means everywhere.
    ///
    /// `None` until something is chosen or a project that remembered one is opened. It is
    /// per-person, so it lives in `workspace.conf` beside the terminal's flags rather than in the
    /// file the project shares.
    pub run_selected: Option<String>,
    /// The `Run Configurations` modal.
    pub run_dialog: RunDialog,
    /// Set when a configuration was added, edited or removed and the file has not been written yet.
    /// Written on the same terms as the settings: once the pointer is up.
    unsaved_run_configurations: bool,
    /// Where this window's menus are drawn.
    pub menu_placement: MenuPlacement,
    /// The projects that have been open, newest first.
    pub recent: Vec<PathBuf>,
    /// What the keyboard is talking to.
    pub focus: Focus,
    /// Something to say in the status bar, such as what version this is.
    pub message: Option<String>,
    /// The dismissible notices, over the bottom right of the window.
    ///
    /// Beside `message` rather than inside a plugin, because every provider reports through the same
    /// `Request` and a notice has to outlive the pane that raised it. `components::toast` says why.
    pub toasts: crate::components::toast::Toasts,
    /// Set when the window has been asked to close, which a test can check instead of the window going.
    pub closing: bool,
    /// Where the settings are kept. Absent until [`Self::load_settings`] is called, which the released
    /// binary does and the tests do not, so a test never reads or writes the settings of the person running
    /// it.
    store: Option<Store>,
    /// Set when a setting or a pane size changed and has not been written yet.
    unsaved_settings: bool,
    /// What was last written to the project's own `.unluminous` folder, so it is written again only when
    /// something has changed. `None` while this window is not remembering the project at all, which is
    /// every window a test builds: the released binary turns it on by calling
    /// [`UnluminousApp::restore_project`], so a test neither reads nor writes a `.unluminous` folder.
    written_project: Option<ProjectState>,
    /// The shells this project was left with, waiting to be started once a frame has been drawn.
    ///
    /// One entry per tab, holding the name a person gave it or an empty string. See
    /// [`UnluminousApp::start_the_restored_terminals`] for why they are not started with the rest of
    /// the project.
    terminals_to_restore: Vec<project_state::RememberedTerminal>,
    /// How many frames this window has drawn.
    ///
    /// Only one thing reads it, and it needs to: eframe keeps the window hidden until it has
    /// painted once, so "the window is on the screen" is "more than nought frames have been drawn"
    /// and there is no other way to ask.
    frames: u64,
    /// The macOS menu bar, once it has been installed.
    native_menu: Option<NativeMenu>,
    /// What each pane has asked for the explorer to scroll to, and what it last scrolled to.
    ///
    /// The window remembers the path it last revealed and compares it against the file showing in
    /// the pane with the keyboard, rather than each of the eleven places a tab can change calling a
    /// reveal. The twelfth, added next month, would be the one that forgot. See
    /// `Self::follow_the_open_file`.
    revealed: Option<PathBuf>,
    /// How many more frames the explorer should scroll to the file that is showing.
    ///
    /// Two rather than one, and the reason is worth keeping. Revealing a file usually **opens folders
    /// out in the same frame**, so the list can grow by forty rows between one frame and the next —
    /// and egui clamps a scroll target against the content size it measured on the *previous* frame.
    /// The first frame therefore scrolls as far as the old, shorter list allowed and stops short of
    /// the row; the second, by which time the list has been measured, reaches it. Measured on a real
    /// window: opening a file three folders down left its row just below the fold until a second
    /// frame was drawn.
    reveal_in_explorer: u8,
    /// The rectangle the editing area last occupied, so a test can measure the document's own text without
    /// also measuring the bars round it.
    editor_area: Rect,
    /// What a pinch has asked for that has not been given to it yet.
    ///
    /// A pinch arrives as a great many small multipliers, one a frame. Multiplying the size by each
    /// one and rounding to a whole point would round every one of them away and nothing would ever
    /// move, so what the gesture has asked for is kept here between frames and the setting changes
    /// only when it has asked for a whole point.
    zoom_pending: f32,
    /// Who has this frame's zoom gesture. See [`ZoomClaim`].
    zoom: ZoomClaim,
    /// How far down its rows the explorer was left, and where to put it back on the next frame.
    ///
    /// `task-1771` makes every pane zoomable, and a zoom that does not keep the row under the pointer
    /// still is a zoom you have to scroll back from - which is the complaint `task-1672` answered for the
    /// editing area. A list is easier than a document: its content scales with its zoom, so the point at
    /// `offset + above` lands at `(offset + above) * ratio`. The correction is worked out here, because the
    /// window is what knows where the pointer was and what the zoom was before it changed, and it is applied
    /// on the **next** frame for the reason the editing area's own anchor is: the rows have not been laid
    /// out at the new size yet.
    explorer_scroll: f32,
    explorer_scroll_to: Option<f32>,
    /// Which of the three tiles took the keyboard, while `focus` is `Focus::Terminal`.
    ///
    /// The terminal tile and the run tile both set `Focus::Terminal`, because the keys go to the program in
    /// whichever grid was clicked and the two are the same emulator. That is fine for the zoom — all three
    /// tiles are drawn at `terminal.font.size`, so they answer with the same setting — and wrong for
    /// **maximising**, which asked to fill the window with the pane in front of somebody and filled it with
    /// the terminal instead. Found by the `task-1771` review.
    tile_with_the_keyboard: dock::Panel,
    /// Which plugin asked for the keyboard, while `focus` is `Focus::Plugin`.
    ///
    /// `Focus` says a plugin has the keys and cannot say **which**, because a plugin's pane is not a
    /// variant of it. `task-1771` needs the difference: `Ctrl` and plus zooms the pane a person is working
    /// in, and with two contributed panes open there is no other way to tell them apart.
    plugin_with_the_keyboard: Option<String>,
    /// The pane filling the window, and what was showing before it did. See [`Maximise`].
    maximised: Maximise,
    /// True while [`Self::toggle_maximised`] is putting panels away or back.
    ///
    /// Every function that shows or hides a panel ends a maximise first — see
    /// [`Self::leave_the_maximised_pane`] — and the maximise itself shows and hides panels, so without this
    /// it would end itself on its first call.
    ///
    /// **As the code stands it cannot change an answer, and that is written down rather than acted on.**
    /// `settle_the_maximise` empties `maximised` before it shows or hides anything and fills it again on
    /// its last statement, so on every route from inside it back to `leave_the_maximised_pane` the
    /// `Maximise::No` half of that test is already true and this half decides nothing. It is kept because
    /// what it guards is the **ordering** — take first, fill last — and a later edit that filled the field
    /// earlier would be silently recursive without it. `task-1922` measured this; do not take it out on the
    /// grounds that nothing reads it.
    settling_the_maximise: bool,
    /// Set by a double click on the empty part of a tab strip, acted on once the pane loop is over.
    ///
    /// After the loop for the reason every other decision about the panes is: maximising changes what is
    /// showing, and a pane changing that while the row of them is being walked would be the loop editing
    /// what it is walking.
    maximise_wanted: bool,
    /// True while the Markdown preview holds the selection that `Copy` means.
    ///
    /// The preview must never take the keyboard: in the side-by-side view the source is being typed
    /// into and the preview is being read, and a click in the preview that stopped the caret working
    /// would be worse than having no selection at all. So `Focus` is left alone and this one flag
    /// says which of the two a copy is about — set by a press in a preview and cleared by a press in
    /// an editing area, which is what "the pane the pointer last pressed in" means.
    reading_preview: bool,
    /// The address a `Ctrl/Cmd+Click` in the preview asked for, acted on at the end of the frame.
    link_to_open: Option<String>,
    /// The pictures in the preview, decoded and kept between frames.
    ///
    /// One of the three caches that stay on the window rather than moving onto the tab with the rest
    /// of `OpenFile::cached`: it is keyed on a path rather than on a document, so panes drawing two
    /// files share it correctly and it costs nothing to keep shared.
    preview_images: PreviewImages,
    /// Every diagram that has been laid out, kept so a preview lays each one out once. Keyed on the
    /// source text, so it is shared between panes for the same reason.
    mermaid_scenes: MermaidScenes,
    /// Set when the theme has been applied, which has to happen once the context exists.
    themed: bool,
    /// The interface scale egui was last told about, so [`Self::apply_the_theme`] can tell a settings
    /// change that moved it from one that did not. egui keeps its own copy of the text sizes, so this is
    /// the only way to know whether they are already right.
    interface_scale: f32,
    /// The family the toolbar uses for its bold B. It is the real bold face once [`Self::prepare`] has
    /// installed it, and the ordinary one before that, because asking egui for a family it has not been
    /// given panics.
    bold_family: egui::FontFamily,
    /// A context to wake the window with when the terminal has something new to draw.
    context: Option<egui::Context>,
    /// What had the keyboard on the last frame, so that the terminal can be told when it gains or loses it.
    last_focus: Focus,
    /// The plugins that are installed, which is what decides how a file is coloured and what icon
    /// it has.
    pub plugins: Plugins,
    /// The plugins' icons, decoded once each.
    icons: Icons,
    /// The debug tile along the bottom, which is the run tile's sibling as the run tile is the
    /// terminal's: the window shows **one** of the three and never two, because two grids stacked
    /// take the editing area below the fold.
    pub debug_panel: DebugPanel,
    /// The session that is running, when one is. **One at most**: The reference editor runs several and the
    /// first version of this does not, which is what keeps every pane of the tile free of a session
    /// chooser above it. See `app::debug`.
    pub debug: Option<DebugState>,
    /// The build that has to finish before a session can start, when there is one. `task-1692`:
    /// `cargo run` names a build tool, so Debug asks cargo what it built before it debugs anything.
    pub debug_build: Option<PendingBuild>,
    /// What was found the last time each adapter was looked for, and when. The search reads
    /// directories and a frame may not, so it is cached here — and because an install running in the
    /// tile beside it changes the answer, the cache **goes stale on its own** after
    /// [`ADAPTER_SEARCH_TTL`] rather than waiting to be told.
    pub debug_adapters: HashMap<String, (std::time::Instant, debuggers::Report)>,
    /// What was said about debugging while there was no session to hold it — which in practice is a
    /// failed build's compiler errors. `debug output` reads it before the session's own output, so
    /// the reason a session never started is in the place somebody would look for it.
    pub debug_output: Vec<String>,
    /// The project's breakpoints, as `.unluminous/breakpoints.conf` holds them. The authority for every
    /// file that is **not** open; a file that is open is owned by its document and pushed in here
    /// whenever it changes — the highlights' rule, unchanged. See `services::breakpoint_store`.
    pub breakpoints: BreakpointStore,
    /// The `Evaluate Expression` modal, when it is open.
    pub evaluate: Option<EvaluateDialog>,
    /// The `Edit Breakpoint` modal, when it is open.
    pub breakpoint_dialog: Option<BreakpointDialog>,
    /// The repository the project is in, when it is in one, and the thread that runs git.
    pub git: Option<GitState>,
    /// Set once the folder has been looked at, so a folder that is not in a repository is not
    /// looked at again on every frame.
    git_looked: bool,
    /// A question with two answers, and the request to send when it is answered.
    ///
    /// Everything that asks first is something git cannot undo — a rollback, a hard reset, dropping
    /// a stash — so what is held is the request, and confirming sends it.
    pub confirmation: Option<Confirmation>,
    /// What was cut or copied in the explorer, waiting to be pasted.
    pub clipboard: FileClipboard,
    /// Where the explorer's own menu is open, what it is about, and whether it was aimed at a row.
    ///
    /// The last of the four is `actions::Aim`: a right click in the empty space below the rows opens
    /// the project folder's menu with everything that is about a particular file dimmed.
    pub explorer_menu: Option<(Pos2, PathBuf, bool, actions::Aim)>,
    /// The row the explorer's own cursor is on, which is what `Delete` is about.
    ///
    /// Separate from the file that is showing, because the two are different questions: a click
    /// selects a row here and opens it there, and the arrow keys move this one without opening
    /// anything at all.
    pub selected: Option<PathBuf>,
    /// How many more frames the explorer should scroll to its own selection.
    reveal_selection: u8,
    /// When the folders that are showing were last asked whether they had changed on disk.
    ///
    /// See [`WATCH_INTERVAL`] and `UnluminousApp::notice_what_changed_on_disk`.
    last_watched: std::time::Instant,
    /// True while a row in the explorer is being carried, which is the one moment the tree must not
    /// be read again underneath the drag.
    dragging_a_row: bool,
    /// Where the window is now, read from egui once a frame.
    ///
    /// `None` until a frame has been drawn, so a window that has not been on the screen yet never
    /// writes a geometry over the one it was opened with. See `project_state`.
    window_place: Option<project_state::WindowPlace>,
    /// The text prompt, when one is open.
    pub prompt: Option<Prompt>,
    /// The `Go to File` modal, when it is open.
    pub go_to_file: Option<GoToFile>,
    /// The `Find Action` palette, when it is open — `task-1922` WP4.
    ///
    /// It holds the menu entries as they stood when it opened rather than asking every frame: the
    /// menus are rebuilt out of `MenuState` each time they are asked for, and a list that changed
    /// under the arrow keys would move the row somebody was about to press Enter on.
    pub palette: Option<crate::components::command_palette::CommandPalette>,
    /// The tabs that have been closed, oldest first, so the newest can be opened again.
    ///
    /// Travel history rather than state: bounded at
    /// [`code_editing::CLOSED_TABS_KEPT`] and not written to disk, which is the line `back` and
    /// `forward` already draw.
    pub(crate) closed_tabs: Vec<code_editing::ClosedTab>,
    /// A check for a newer release, while one is running. `task-1804` §6.
    ///
    /// `None` unless somebody asked, which is the whole of the design: see `services::update`.
    pub(crate) update: Option<crate::services::update::Check>,
    /// What the last check came to, so the About box can say it without asking again.
    pub(crate) update_answer: Option<crate::services::update::Answer>,
    /// The Find bar over the file that is showing, when it is open. `task-1804` §3.1.
    ///
    /// A bar rather than a modal, and it holds no thread: the file being searched is already in
    /// memory, so the matches are worked out on the drawing thread in a `str::find` per line.
    /// `Find in Files` needs a thread because it reads the disk; this does not.
    pub find: Option<crate::services::find::Find>,
    /// The `Find in Files` modal, when it is open. It holds the thread the searching runs on, so
    /// shutting the modal is what stops that thread.
    pub find_in_files: Option<FindInFiles>,
    /// The references, candidates or rename modal, when one is open. Like `Find in Files` it holds
    /// the thread it searches on, so shutting it is what stops that thread.
    pub references: Option<References>,
    /// The project's definitions, the thread they are read on, and whether what is indexed is still
    /// what is on the disk. See [`SymbolIndexState`] and `app::symbols`.
    pub(crate) symbol_index: SymbolIndexState,
    /// The word under the pointer while the modifier is held, and where a click on it would go.
    /// Cached against the text revision and the word, so a resting pointer costs one comparison.
    pub(crate) hover: Option<symbols::Hover>,
    /// What the rename modal was asked about, while one is open. See [`symbols::RenameInProgress`].
    pub(crate) rename: Option<symbols::RenameInProgress>,
    /// Where the caret has been, so `Navigate Back` can go there. Travel history rather than state:
    /// bounded, and not written to disk.
    pub(crate) back: Vec<symbols::Place>,
    /// The mirror stack, pushed by `Navigate Back` and cleared by any new jump.
    pub(crate) forward: Vec<symbols::Place>,
    /// The About box, when it is open. It holds the version and the build date as text rather than
    /// reading them when it draws, so that a screenshot test can fix them; `About::current` is what
    /// `Action::About` puts here.
    pub about: Option<About>,
    /// Set when something outside the editing area moved the caret — opening a `Find in Files`
    /// result is the only one — so the next frame scrolls the file to show it.
    reveal_caret: bool,
    /// Where the gutter's own menu is open, when it is. Held here rather than in egui's memory so
    /// that a test can open it: a screenshot test cannot press the right mouse button.
    pub gutter_menu: Option<Pos2>,
    /// Which paragraph that menu was opened over, so its breakpoint entries are about the row under
    /// the pointer rather than about the caret — the rule the text menu and the terminal tab menu
    /// already follow. `None` means the caret's line, which is what the keyboard and the command
    /// line mean.
    pub gutter_menu_line: Option<usize>,
    /// The values painted at the ends of lines, worked out once per stop. See
    /// [`UnluminousApp::inline_values`].
    pub(crate) inline_cache: Option<InlineValues>,
    /// The temporary breakpoint `Run to Cursor` made, and where. Taken away at the next stop.
    ///
    /// Held here rather than on the session because it is a **breakpoint**, and every breakpoint in
    /// Unluminous lives where the text is — this is only the note that says which one to take back.
    pub(crate) run_to: Option<(PathBuf, usize)>,
    /// Where a tab's own menu is open, and which pane's strip it was opened on. Held here for the
    /// same reason the gutter's is.
    ///
    /// The entries in it all act on "the tab that is showing", which is what makes them ordinary
    /// parameterless actions the View menu and the command line can ask for too — so opening the
    /// menu shows the tab it was opened on first. The editing area's own menu already sets that
    /// precedent: a right click outside the selection puts the caret there before opening.
    pub tab_menu: Option<(Pos2, usize)>,
    /// Where a terminal tab's own menu is open, and which tab it was opened on. Held here for the
    /// same reason the other three are: a screenshot test cannot press the right mouse button.
    pub terminal_menu: Option<(Pos2, usize)>,
    /// The tab being carried by the pointer, while one is. Frame local: the strip reports the drag
    /// every frame it is held and the window settles it once every pane has drawn, because a tab
    /// picked up in one pane is very often dropped on another and no one strip can see them all.
    tab_drag: Drag<usize>,
    /// The **panel** being carried to another edge of the window, while one is — `task-1697`.
    ///
    /// Frame local for the same reason the tab drag is: each panel's header reports that it is in
    /// the air, and `settle_the_panel_drag` decides where it landed once every panel has been drawn,
    /// which is the first moment anything knows where all of them are.
    panel_drag: Drag<dock::Panel>,
    /// The rectangle the panels are laid out inside: the body, less the rail down the far left.
    ///
    /// Recorded so that [`Self::panel_area`] can work out the rectangle a panel that is **not**
    /// showing would have, which is what a run or a debug session started while its tile is put away
    /// has to know — see `run_grid_size`.
    pub(crate) panes_area: Rect,
    /// Where each panel was drawn this frame, and what was left for the document.
    ///
    /// The one answer to "how big is the terminal", which `run_grid_size` and
    /// `terminal_grid_size` read:
    /// a tile on the right is as tall as the body and as wide as its column, and neither of those is
    /// `panes.terminal_height`.
    pub(crate) panel_rects: dock::Regions,
    /// A panel's own menu, when it is open, and which panel it is about.
    ///
    /// Held here for the same reason the other four context menus are: a screenshot test cannot
    /// press the right mouse button, so it sets this and the menu is drawn.
    pub panel_menu: Option<(Pos2, dock::Panel)>,
    /// Where each pane's strip of tabs drew itself and its tabs, in pane order. Frame local, and
    /// rebuilt by the pane loop, which is the only thing that can know it.
    tab_strips: Vec<file_tabs::Strip>,
    /// Where each File Editor **node**'s own strip drew itself, and the rectangle of the node it belongs
    /// to, in screen points.
    ///
    /// **A node is a third kind of place a tab can be dropped**, so it joins the list `settle_the_tab_drag`
    /// reads: that function exists because a tab picked up in one place is dropped in another as often as
    /// not, and a node's strip left out of it is a tab that cannot be dragged out of a node at all.
    /// `task-1905`.
    node_tab_strips: Vec<(crate::services::space::NodeId, Rect, file_tabs::Strip)>,
    /// A file being carried out of a list, until the frame settles where it landed. See [`Drag`].
    file_drag: Drag<PathBuf>,
    /// Input that was asked for down the command line and has not reached a frame yet.
    ///
    /// **The one way to click or type in a window that is not in front.** Synthetic operating system
    /// input goes to the foreground window, so a script had to activate Unluminous to drive it — and on
    /// Windows activating a window on another virtual desktop switches the desktop with it, which is what
    /// `task-1914` reports. See `services::input` and
    /// `tasks/task-1914-testing-without-stealing-focus-tdd.md`.
    pub(crate) input: crate::services::input::Queue,
    /// The editing area's own menu, when it is open. Held here for the same reason the gutter's is,
    /// and it carries the colour wheel with it.
    pub text_menu: Option<text_menu::TextMenu>,
    /// The colour the wheel was last left on, so opening it again starts where it was left rather
    /// than back at the first block. In memory only: it is a habit within one sitting rather than a
    /// setting, and the four blocks are what a lasting preference looks like.
    pub last_highlight: Rgba,
    /// The passages marked in every file of this project. The authority for every file that is not
    /// open; a file that **is** open is owned by its document and pushed in here whenever it
    /// changes. See `services::file_marks`.
    pub marks: FileMarks,
    /// The command channel, once it has been opened. `None` in every window a test builds, and in a
    /// released Unluminous started with `--control off`: a window with no channel is an ordinary window.
    pub(crate) control: Option<control::Server>,
    /// The MCP endpoint this window hosts, when `mcp.enabled` is on. `None` in every window a test
    /// builds, exactly as the command channel is: a test must not open a port either.
    pub(crate) mcp: Option<services::mcp::Hosted>,
    /// Commands that have been accepted and are waiting for something — a painted frame, a shell, a
    /// search, git. See `app::cli`.
    pub(crate) cli_waiting: Vec<(control::Pending, cli::Waiting)>,
    /// The completion popup, when one is open. One at most, because it belongs to the pane with the
    /// keyboard — the same reasoning as the one `hover` and the one `references` modal. See
    /// `app::completion`.
    pub(crate) completion: Option<completion::CompletionState>,
    /// Where the popup hangs. Frame local, recorded by the pane that has the keyboard as it draws
    /// its caret, because the pane loop borrows the focus and no one else can know where that caret
    /// ended up on the screen. Read after the loop, which is where the popup is drawn.
    completion_anchor: Option<completion::CompletionAnchor>,
    /// The value tooltip, when one is open. One at most, for the completion popup's reason: it
    /// belongs to the pane the pointer is in, and the pointer is only ever in one. See
    /// `app::hover_value`.
    pub value_tooltip: Option<hover_value::ValueTooltipState>,
    /// Which stop's execution point has already been jumped to, so the jump happens once a stop
    /// rather than once a frame. See [`UnluminousApp::follow_the_execution_point`].
    followed_stop: Option<(u64, PathBuf, usize)>,
    /// The expression the pointer is resting on and since when, which is all the delay needs.
    pub(crate) hover_rest: Option<hover_value::Resting>,
    /// True while a popup `Debug -> Show Value` asked for is waiting to be told where the caret
    /// ended up on the screen — `completion_anchor`'s problem, solved in the same place.
    pub(crate) caret_tooltip: bool,
}

impl UnluminousApp {
    /// A new window showing `folder` in the explorer and an empty document.
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        let folder = folder.into();
        let renderer = TextRenderer::new();
        crate::services::frame_trace::mark("fonts");
        let mut document = Document::new();
        let mut settings = Settings::new();
        settings.font_family = renderer.default_family();
        // Start in a family this system actually has, so the first thing typed is visible.
        document.set_base_style(settings.as_style_change());
        // Read here rather than in the struct below so that starting up can be measured: the
        // plugins are read from disk and the icons are decoded, and both are worth a mark of their
        // own. See `services::frame_trace`.
        let plugins = Plugins::load(None).0;
        crate::services::frame_trace::mark("plugins");
        let tree = FileTree::new(&folder);
        crate::services::frame_trace::mark("file-tree");
        Self {
            layouts_built: 0,
            plugin_wants_a_repaint: false,
            plugin_wants_copied: None,
            plugins_ticked_at: None,
            plugin_ui: plugin_panes::PluginUi::default(),
            space: space::SpaceState::default(),
            told_the_plugins: None,
            canvases: crate::services::vello_canvas::Canvases::default(),
            files: OpenFiles::new(document),
            browser: BrowserHost::new(),
            browser_placements: Vec::new(),
            tree,
            renderer,
            settings,
            panes: Panes::new(),
            filter: String::new(),
            explorer_visible: true,
            editor_visible: true,
            settings_window: SettingsWindow::default(),
            terminal: TerminalPanel::new(Some(folder)),
            run: RunPanel::new(),
            run_configurations: RunConfigurations::new(),
            run_selected: None,
            run_dialog: RunDialog::default(),
            unsaved_run_configurations: false,
            debug_panel: DebugPanel::new(),
            debug: None,
            debug_build: None,
            debug_output: Vec::new(),
            debug_adapters: HashMap::new(),
            breakpoints: BreakpointStore::new(),
            evaluate: None,
            breakpoint_dialog: None,
            menu_placement: MenuPlacement::for_this_platform(),
            recent: Vec::new(),
            focus: Focus::Editor,
            message: None,
            toasts: crate::components::toast::Toasts::default(),
            closing: false,
            store: None,
            unsaved_settings: false,
            written_project: None,
            terminals_to_restore: Vec::new(),
            frames: 0,
            native_menu: None,
            revealed: None,
            selected: None,
            reveal_selection: 0,
            last_watched: std::time::Instant::now(),
            dragging_a_row: false,
            window_place: None,
            reveal_in_explorer: 0,
            editor_area: Rect::ZERO,
            zoom_pending: 1.0,
            zoom: ZoomClaim::Nobody,
            explorer_scroll: 0.0,
            explorer_scroll_to: None,
            plugin_with_the_keyboard: None,
            tile_with_the_keyboard: dock::Panel::Terminal,
            maximised: Maximise::No,
            settling_the_maximise: false,
            maximise_wanted: false,
            reading_preview: false,
            link_to_open: None,
            preview_images: PreviewImages::new(),
            mermaid_scenes: MermaidScenes::new(),
            themed: false,
            interface_scale: 1.0,
            bold_family: egui::FontFamily::Proportional,
            context: None,
            last_focus: Focus::Editor,
            plugins,
            icons: Icons::new(),
            git: None,
            git_looked: false,
            confirmation: None,
            clipboard: FileClipboard::new(),
            explorer_menu: None,
            prompt: None,
            go_to_file: None,
            palette: None,
            closed_tabs: Vec::new(),
            update: None,
            update_answer: None,
            find: None,
            find_in_files: None,
            references: None,
            symbol_index: SymbolIndexState::NotStarted,
            hover: None,
            rename: None,
            back: Vec::new(),
            forward: Vec::new(),
            about: None,
            reveal_caret: false,
            gutter_menu: None,
            gutter_menu_line: None,
            inline_cache: None,
            run_to: None,
            text_menu: None,
            tab_menu: None,
            terminal_menu: None,
            tab_drag: Drag::Nothing,
            tab_strips: Vec::new(),
            node_tab_strips: Vec::new(),
            file_drag: Drag::Nothing,
            input: crate::services::input::Queue::default(),
            panel_drag: Drag::Nothing,
            panes_area: Rect::ZERO,
            panel_rects: dock::Regions { panels: [Rect::ZERO; dock::SLOTS], editor: Rect::ZERO },
            panel_menu: None,
            last_highlight: theme::color::HIGHLIGHT_YELLOW,
            marks: FileMarks::new(),
            control: None,
            mcp: None,
            cli_waiting: Vec::new(),
            completion: None,
            completion_anchor: None,
            value_tooltip: None,
            followed_stop: None,
            hover_rest: None,
            caret_tooltip: false,
        }
    }

    /// A window whose document already holds `text`, which the screenshot tests use to set a scene.
    pub fn with_text(folder: impl Into<PathBuf>, text: &str) -> Self {
        let mut app = Self::new(folder);
        app.document_mut().apply(Command::Insert(text.to_owned()));
        app.document_mut().apply(Command::MoveDocumentStart { extend: false });
        app
    }

    /// Open the command channel, so that `unluminous-cli` can drive this window.
    ///
    /// Called from `main.rs` and nowhere else, exactly as [`Self::load_settings`] and
    /// [`Self::restore_project`] are, and for the same reason: a test must not open a port, write an
    /// instance file into the person's settings folder, or leave a listener behind when it ends. A
    /// window with no channel is an ordinary window in every other respect.
    pub fn open_control_channel(&mut self, ctx: &egui::Context) {
        let folder = self.tree.root().to_path_buf();
        // The context rather than `thread_waker`, because this is called before the first frame and
        // the window has not yet been given one to wake.
        let context = ctx.clone();
        self.control = control::Server::start(folder, Arc::new(move || context.request_repaint()));
        if let Some(server) = &self.control {
            self.message = Some(format!(
                "Unluminous {} \u{00B7} the command line is listening on port {}",
                crate::build_info::VERSION,
                server.port()
            ));
        }
        // The MCP endpoint is opened from here for the same reason the command channel is: a test
        // must not open a port or leave a listener behind when it ends. A window that never calls
        // this has no endpoint and is an ordinary window in every other respect.
        self.mcp = Some(services::mcp::Hosted::new());
        self.reconcile_mcp();
    }

    /// Draw a plugin's decoration the same way on every machine.
    ///
    /// `vello_cpu` picks the widest SIMD the processor has, and whether two levels are bit-identical is not
    /// something it promises — so a board screenshot accepted on the machine that took it could fail on a
    /// machine with a different processor, for a reason that is not a fault in Unluminous. The screenshot tests
    /// call this and the released binary does not, because in the window the fastest is what is wanted.
    pub fn draw_deterministically(&mut self) {
        self.canvases = crate::services::vello_canvas::Canvases::deterministic();
    }
    /// Set the window's look up before the first frame is drawn.
    ///
    /// This has to happen before the first frame rather than during it, because `Context::set_fonts` takes
    /// effect at the start of the next frame, and asking for a font family that has not been bound yet
    /// panics inside egui.
    pub fn prepare(&mut self, ctx: &egui::Context) {
        // The active theme is this **thread's**, and cargo reuses a thread for the next test, so a window
        // starts from Unluminous's own rather than from whatever the last window on this thread was painted
        // in. In the released binary this is a window's first frame and there is nothing to reset;
        // `use_store` reads the settings and applies the chosen theme a moment later.
        theme::activate(theme::Theme::unluminous_dark());
        theme::apply(ctx);
        self.themed = true;
        // What the plugins contribute, read from their manifests. Here rather than only in
        // `load_settings`, because a window a test builds has no settings folder and still has to draw
        // the rail button, the menu and the Settings page a plugin adds — the drawing is Unluminous's own
        // code, so there is nothing about it that needs a folder. A provider with no folder opens its
        // board in memory, which is what stops a test touching the one somebody is using.
        self.refresh_the_plugins();
        self.context = Some(ctx.clone());
        let family = self.renderer.default_family();
        let regular = self.renderer.face_bytes(&family, false);
        let bold = self.renderer.face_bytes(&family, true);
        let has_bold = bold.is_some();
        theme::install_fonts(ctx, &family, regular, bold);
        if has_bold {
            self.bold_family = egui::FontFamily::Name(theme::BOLD_FAMILY.into());
        }
    }

    /// The same, against a named folder, which is what a test that wants to check the settings uses.
    pub fn use_store(&mut self, store: Store) {
        self.browser.set_profile(store.folder().join("browser"));
        let (settings, panes) = settings::load(&store);
        // A settings file written before this system had the family in it, or with no family at all, falls
        // back to one this system has.
        self.settings = settings;
        if self.settings.font_family.is_empty()
            || !self.renderer.families().contains(&self.settings.font_family)
        {
            self.settings.font_family = self.renderer.default_family();
        }
        self.panes = panes;
        // What this project leaves out of the searchable list, which reloads the tree when it moved.
        // `task-1804` §7.3.
        self.tree.set_exclude(&self.settings.exclude);
        // And whether there is a newer Unluminous, **only if the settings say to ask**. This is the
        // one place anything is sent without somebody pressing something, and it happens because
        // they turned it on. `task-1804` §6; see `services::update` for the whole of the rule.
        if self.settings.update_check.at_start() {
            self.check_for_updates();
        }
        store.remember_project(self.tree.root());
        // And that this project has a window open, which is what `task-1693` asks Unluminous to bring
        // back next time. A window that is already in the list writes nothing — `Store::open_windows`
        // records why.
        //
        // **Which window, and whether any other one is still running.** `task-1912`: a window opening while
        // nothing else is running begins a new session and the file becomes that one row, which is what stops
        // the list being every project there has ever been. A *listed instance* rather than a bare process id,
        // because an id the operating system has handed to something else would read as a session still going.
        store.remember_open_window(self.tree.root(), std::process::id(), &|pid| {
            unluminous_cli::instances::listed().iter().any(|instance| instance.pid == pid)
                && unluminous_cli::instances::is_running(pid)
        });
        self.recent = store.recent_projects();
        let (plugins, problems) = Plugins::load(Some(&store));
        self.plugins = plugins;
        // A plugin that will not parse is skipped and said out loud, rather than stopping Unluminous.
        // The same rule the settings file already keeps for a line it cannot read.
        if let Some(first) = problems.first() {
            self.message = Some(format!("A plugin could not be read \u{2014} {first}"));
        }
        self.store = Some(store);
        self.refresh_the_plugins();
        // Read the pane sizes and sides again now that the contributed panes have names: the first read
        // happened before the manifests were, so it could not know what to look for.
        if let Some(store) = self.store.as_ref() {
            let (_, panes) = settings::load_with(store, &self.plugin_ui.pane_keys());
            self.panes = panes;
        }
        self.refresh_the_plugins();
        let store = self.store.take().expect("just set");
        // On the first run there is no settings file. One is written straight away, holding the defaults,
        // so that a person looking for it finds it and can see what its names are rather than having to
        // change a setting first to make it appear.
        let first_run = !store.settings_path().is_file();
        self.store = Some(store);
        if first_run {
            self.write_settings();
        }
        self.set_the_font_everywhere();
        // The theme after the plugins, because the theme this file names may be one of theirs, and after
        // the font because the interface font falls back to the editor's family.
        self.apply_the_theme();
        self.install_the_interface_font();
    }

    /// Read what was left open in this project, put it back, and remember it from then on.
    ///
    /// Called by the released binary only, and for the same reason [`Self::load_settings`] is: a test
    /// must not read or write anything belonging to the person running it, and a `.unluminous` folder written
    /// into a test's own sample project would change what the explorer draws in the middle of a
    /// screenshot test.
    ///
    /// The files are opened permanently rather than transiently, because a tab that was there when the
    /// window closed is not a file somebody is glancing at.
    pub fn restore_project(&mut self) {
        // Read before the files are opened, so each tab comes up with its passages already marked
        // rather than having them appear a frame later.
        self.marks = FileMarks::load(self.tree.root());
        // And where the program is to stop, on exactly the same terms and for the same reason: a
        // breakpoint that appeared a frame after its file did would be a dot that arrived late.
        self.breakpoints = BreakpointStore::load(self.tree.root());
        let state = project_state::load(self.tree.root());
        for folder in &state.expanded_folders {
            self.tree.expand(folder);
        }
        // The tabs a plugin drew, reopened before the files so they sit where they did: a plugin tab is
        // opened from a menu rather than by opening a file, so nothing else would bring it back.
        for key in state.plugin_tabs.clone() {
            if self.plugin_ui.surfaces().tab(&key).is_some() {
                self.open_the_plugin_tab(&key);
            }
        }
        for path in &state.open_files {
            // A file that was open last time and has gone or changed since is a message in the
            // status bar rather than a reason to stop restoring the rest of them.
            let _ = self.open_path_permanently(path);
        }
        // The panes after the tabs, because a tab has to exist before it can be put in one. The tabs
        // are opened in the order they were written, so the list lines up with them — and anything in
        // it that would break an invariant is corrected rather than refused. See
        // `OpenFiles::restore_panes`.
        if !state.file_panes.is_empty() {
            self.files.restore_panes(&state.file_panes, &state.pane_widths, state.active_pane);
        }
        // Where each tab was left. After the panes, because a tab has to be in its pane before the
        // place it was being read at means anything, and before the tab that was showing is chosen,
        // because showing one throws away what was laid out for it.
        //
        // A caret past the end of a file that has changed since is **clamped rather than refused**,
        // which is the rule this whole feature keeps: a project that opens with part of its state is
        // better than one that will not open.
        for (index, path) in state.open_files.iter().enumerate() {
            let Some(tab) = self.files.index_of(path) else {
                continue;
            };
            let caret = state.file_carets.get(index).copied().unwrap_or(0);
            let scroll = state.file_scrolls.get(index).copied().unwrap_or(0.0);
            let file = self.files.at_mut(tab);
            let end = file.document.text().len_bytes();
            file.document.apply(Command::PlaceCaret { offset: caret.min(end), extend: false });
            file.scroll = scroll.max(0.0);
        }
        if let Some(path) = state.open_files.get(state.active_file) {
            if let Some(index) = self.files.index_of(path) {
                self.show_tab(index);
            }
        }
        self.explorer_visible = state.explorer_visible;
        self.editor_visible = state.editor_visible;
        // The project's run configurations, and which of them the widget had chosen. **No run is
        // started**: unlike a terminal, which is a place to type, a run is something that was
        // started deliberately, and restarting somebody's dev server because they closed the window
        // would be a surprise. The tile comes back up holding nothing, which is what it says.
        self.run_configurations = run_configurations::load(self.tree.root());
        self.run.visible = state.run_visible;
        // The remembered choice is only adopted if something still answers to it. A temporary is
        // not written down, so the commonest thing a project remembers having chosen is a name
        // that has gone with the window — and a widget offering to run something that is not there
        // is worse than one offering nothing. The detectors count, which is why this is asked
        // after the plugins have been read.
        if !state.run_selected.is_empty()
            && self.configuration_named(Some(&state.run_selected)).is_some()
        {
            self.run_selected = Some(state.run_selected.clone());
        }
        // The shells themselves cannot be brought back, so the same number of fresh ones are started in
        // the project's own folder, which is what a person means by "my terminals were there".
        if state.terminal_visible && state.terminal_tabs > 0 {
            self.terminal.visible = true;
            self.run.visible = false;
            // Written down rather than started. See [`UnluminousApp::start_the_restored_terminals`]:
            // starting a shell is the one thing this function does that the first frame does not
            // need, and it was a fifth of the time before the window appeared.
            self.terminals_to_restore = (0..state.terminal_tabs)
                .map(|index| state.terminals.get(index).cloned().unwrap_or_default())
                .collect();
        }
        // The canvases this project was left with - `task-1904`. Read here rather than at startup,
        // so a test neither reads nor writes a person's `.unluminous` folder; `restore_project` is
        // called from `main.rs` and by nothing else.
        self.restore_the_space();
        self.space.visible = state.space_visible;
        // **The keyboard goes to a surface that is on the screen.** `Focus::Editor` is what a window
        // starts on, and a project whose canvas fills the window has no editing area for the keys to
        // reach — so every key press went to a pane nobody could see, which `task-1914` reported as
        // *"In base of infinite space, i cant type in a terminal."*
        //
        // The rule is the narrow one: where **both** are showing the editing area keeps the keyboard,
        // which is what a text editor should do and what every existing test asserts.
        if self.space.visible && !self.editor_visible {
            self.focus = Focus::Space;
        }
        self.written_project = Some(self.project_state());
    }

    /// What is open in this project now.
    fn project_state(&self) -> ProjectState {
        // One pass over the tabs that have a path, so the four parallel lists are the same length
        // by construction. They were built from two different walks before — `paths()`, which drops
        // a tab that has never been saved, and `panes_of_tabs()`, which does not — so an untitled
        // tab slid every pane number along by one.
        let mut open_files = Vec::new();
        let mut file_panes = Vec::new();
        let mut file_scrolls = Vec::new();
        let mut file_carets = Vec::new();
        let mut plugin_tabs = Vec::new();
        for file in self.files.iter() {
            // A tab a plugin drew is remembered by its own name, in a list of its own: every list beside
            // `open_files` is indexed with it, and none of them means anything for a tab with no document.
            if let Some(plugin) = &file.plugin {
                plugin_tabs.push(plugin.key.clone());
                continue;
            }
            let Some(path) = file.path() else {
                continue;
            };
            open_files.push(path.to_path_buf());
            file_panes.push(file.home.pane().unwrap_or(0));
            // Where each tab was left, so a project opens at the line it was being read at rather
            // than at the top of every file — `task-1693`.
            file_scrolls.push(file.scroll);
            file_carets.push(file.document.selection().head);
        }
        let active = self
            .files
            .active()
            .path()
            .and_then(|path| open_files.iter().position(|known| known == path));
        ProjectState {
            open_files,
            plugin_tabs,
            active_file: active.unwrap_or(0),
            file_panes,
            file_scrolls,
            file_carets,
            pane_widths: self.files.pane_widths().to_vec(),
            active_pane: self.files.focused_pane(),
            expanded_folders: self.tree.expanded_folders(),
            // **What was showing before a pane was maximised, when one is.** Maximising works by putting
            // every other panel away, so the visibility flags while it is on are not an arrangement anybody
            // chose — and this is written to the project's `.unluminous` every frame. Unluminous closed with the
            // explorer maximised would have opened next time with the editing area and the terminal hidden
            // and nothing left to say why. Found by the `task-1771` review.
            explorer_visible: self.was_showing(dock::Panel::Explorer, self.explorer_visible),
            editor_visible: match self.maximised {
                Maximise::Filling { editor, .. } => editor,
                Maximise::No => self.editor_visible,
            },
            terminal_visible: self.was_showing(dock::Panel::Terminal, self.terminal.visible),
            space_visible: self.was_showing(dock::Panel::Space, self.space.visible),
            terminal_tabs: self.terminal.tabs.count(),
            // The names a person typed, and nothing else: `Tabs::names` would give back
            // `powershell.exe 2` for a tab nobody has named, which is a name the next run would
            // restore as though somebody had chosen it.
            terminal_tab_names: self
                .terminal
                .tabs
                .sessions()
                .iter()
                .map(|session| session.given_name().unwrap_or_default().to_owned())
                .collect(),
            // **And where each tab's shell had got to** - `task-1945`. `Session::folder` reads the
            // current directory off the shell process itself rather than answering with the one it was
            // spawned in, because a person types `cd` and nothing about that reaches the pseudoterminal.
            // A platform that will not say answers `None` and the tab reopens in the project's own root,
            // which is what every tab did before this.
            terminals: self
                .terminal
                .tabs
                .sessions()
                .iter()
                .map(|session| project_state::RememberedTerminal {
                    name: session.given_name().unwrap_or_default().to_owned(),
                    folder: session
                        .folder()
                        .map(|path| path.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                })
                .collect(),
            run_visible: self.was_showing(dock::Panel::Run, self.run.visible),
            run_selected: self.run_selected.clone().unwrap_or_default(),
            // Where the window is, which is the other half of "in the same location and state". It
            // is read from egui once a frame and is `None` until there has been one, so a window
            // that has not drawn yet never writes a geometry over the one it was opened with.
            window: self.window_place,
        }
    }

    /// What the project's `.unluminous` would record about which panels are showing.
    ///
    /// **Not what is showing**, while a pane is maximised: see [`Self::was_showing`]. Public for a test, for
    /// the reason [`Self::panel_rect_for_tests`] is — the difference between the two is the whole of the
    /// fault this exists to pin, and it is invisible from outside without it.
    pub fn remembered_panels_for_tests(&self) -> (bool, Vec<bool>) {
        let editor = match self.maximised {
            Maximise::Filling { editor, .. } => editor,
            Maximise::No => self.editor_visible,
        };
        let panels = dock::Panel::all(self.plugin_ui.pane_count())
            .into_iter()
            .map(|panel| self.was_showing(panel, self.panels_showing()[panel.index()]))
            .collect();
        (editor, panels)
    }

    /// Write what is open down, if it has changed since it was last written.
    ///
    /// Called every frame and writes almost never: the comparison is against what is on disk, so
    /// nothing is written until a tab, a folder or a pane actually changed.
    fn remember_the_project(&mut self) {
        if self.written_project.is_none() {
            return;
        }
        let now = self.project_state();
        if self.written_project.as_ref() == Some(&now) {
            return;
        }
        project_state::save(self.tree.root(), &now);
        self.written_project = Some(now);
    }

    /// Build the macOS menu bar. Called by the released binary only: a test has no application to attach a
    /// menu bar to, and the bar drawn inside the window is what the tests exercise.
    pub fn install_native_menu(&mut self) {
        let menus = actions::menus(&self.menu_state());
        self.native_menu = Some(NativeMenu::install(&menus, self.context.as_ref()));
    }

    /// The document in the tab that is showing.
    ///
    /// A method rather than a field, because which document is the open one is a property of the
    /// tabs. Everything that used to reach for `app.document` now goes through here, so there is one
    /// answer to what "the open file" means.
    pub fn document(&self) -> &Document {
        &self.files.active().document
    }

    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.files.active_mut().document
    }

    /// Which of the three ways of looking at the open file is showing.
    pub fn view_mode(&self) -> ViewMode {
        self.files.active().view_mode
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.files.active_mut().view_mode = mode;
    }

    /// The layout as it was last painted, which the tests assert against.
    pub fn layout(&self) -> &Layout {
        &self.files.active().cached.layout
    }

    /// The rectangle the editing area last occupied.
    pub fn editor_area(&self) -> Rect {
        self.editor_area
    }

    /// How opaque the window background is.
    pub fn opacity(&self) -> f32 {
        self.settings.opacity
    }

    /// Where the caret is, as the status bar reports it.
    pub fn caret_position(&self) -> status_bar::Position {
        status_bar::position_of(self.document().text(), self.document().selection().head)
    }

    /// What the status bar says the open file was on disk: `CRLF`, or `CRLF · Latin-1`.
    ///
    /// Nothing at all for a tab with no file behind it -- a picture, a plugin's pane, a document
    /// nobody has saved yet -- because there is no file whose bytes could be described. `task-1804`
    /// §7.1: before this, whether saving would rewrite every line in the file was not shown
    /// anywhere.
    pub fn line_ending_label(&self) -> Option<String> {
        let file = self.files.active();
        if file.is_picture() || file.plugin.is_some() || file.is_browser() {
            return None;
        }
        let document = self.document();
        document.path()?;
        let ending = self.settings.line_endings.applied_to(document.line_ending());
        let encoding = document.encoding();
        Some(if encoding == unluminous_core::Encoding::Utf8 {
            ending.name().to_owned()
        } else {
            format!("{} · {}", ending.name(), encoding.name())
        })
    }

    /// Run a command, as the toolbar or a test would.
    pub fn command(&mut self, command: Command) {
        self.document_mut().apply(command);
    }

    /// The colour of the window background at the current opacity setting.
    ///
    /// The alpha is what makes the desktop visible through the window. It is applied to the background
    /// only; text is painted separately at full alpha.
    pub fn background(&self) -> Color32 {
        theme::faded(color::editor(), self.settings.opacity)
    }

    /// True when this window is the one allowed to read and write the project's own `.unluminous` folder.
    ///
    /// Which is the released binary and nothing else: `restore_project` is what turns it on, and a
    /// test neither reads nor writes a person's files.
    fn remembers_this_project(&self) -> bool {
        self.written_project.is_some()
    }

    /// Where one pane's strip of tabs drew itself on the last frame.
    ///
    /// For a test that has to press the part of it no tab wanted, which is where two presses fill the
    /// window with the editing area. `Rect::NOTHING` for a pane that is not there.
    pub fn tab_strip_for_tests(&self, pane: usize) -> Rect {
        self.tab_strips.get(pane).map(|strip| strip.area).unwrap_or(Rect::NOTHING)
    }

    /// The File Editor node strips this frame recorded, for a test.
    ///
    /// **What it proves is an ordering.** `settle_the_tab_drag` reads this list, and it has to run after the
    /// canvas has been drawn rather than after the panes alone — settled in the wrong place the list is
    /// empty every time it is read, and a tab can neither be dragged onto a node nor off one. A synthesised
    /// pointer cannot check that, because a node's strip is drawn into a transformed sublayer.
    pub fn node_tab_strips_were_recorded(&self) -> Vec<(crate::services::space::NodeId, Rect)> {
        self.node_tab_strips.iter().map(|(node, rect, _)| (*node, *rect)).collect()
    }

    /// The next frame's worth of queued input, for a test.
    ///
    /// **A harness has no `raw_input_hook`.** `egui_kittest` runs an `eframe::App` by calling `logic` and
    /// `ui` and nothing else, so the one line in `raw_input_hook` that feeds the queue never runs there —
    /// and a test that wanted to drive the window through `unluminous-cli input` would wait for frames
    /// that never carried anything. This is that line, for a test to call, so what is exercised is the
    /// queue and the shape of each gesture rather than a second copy of either.
    ///
    /// The events go into `Harness::input_mut().events`, which is `RawInput` before the pass — the same
    /// place `raw_input_hook` puts them and for the same reason. See `services::input`.
    pub fn take_the_next_input_frame(&mut self) -> Option<Vec<egui::Event>> {
        self.input.next_frame()
    }

    /// How many times any file has been laid out since the window opened.
    ///
    /// Read by a test to prove that showing a tab which has already been laid out does not lay it
    /// out again. See the field.
    pub fn layouts_built(&self) -> u64 {
        self.layouts_built
    }
}

/// Move a file or a folder on the disk.
///
/// A rename first, which is one operation and keeps the file's own history where the platform has
/// one; a copy and a delete when that fails, which is what happens across volumes and is what
/// `services::file_clipboard` already does for a paste.
fn move_the_bytes(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_the_folder(from, to)?;
        std::fs::remove_dir_all(from)
    } else {
        std::fs::copy(from, to)?;
        std::fs::remove_file(from)
    }
}

/// Copy a folder and everything under it.
fn copy_the_folder(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let here = entry.path();
        let there = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_the_folder(&here, &there)?;
        } else {
            std::fs::copy(&here, &there)?;
        }
    }
    Ok(())
}

/// Write a closed file's share of a move, having checked it is still the file the plan was made
/// against.
///
/// The check is what makes this safe on a syntactic tier: every range is compared against the
/// length of the text it is supposed to be inside, and a file that has changed since the plan was
/// made is refused whole rather than patched on faith. Bytes outside the ranges are untouched, so
/// encodings, line endings and trailing whitespace survive byte for byte.
fn write_the_edits(path: &Path, edits: &[(std::ops::Range<usize>, String)]) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|problem| problem.to_string())?;
    if edits.iter().any(|(range, _)| range.end > text.len() || !text.is_char_boundary(range.start))
    {
        return Err("it has changed since the move was worked out".to_owned());
    }
    let after = file_move::applied(&text, edits);
    // Through a temporary and a rename (`task-1984` A10): a crash or a full disk part way through a
    // rename across forty files would otherwise leave one of them at zero length, with no buffer
    // anywhere to build it back from.
    crate::services::store::write_a_source_file(path, after.as_bytes())
        .map_err(|problem| problem.to_string())
}

/// The colour a file is drawn in for what git thinks of it.
///
/// The same three colours the change bars in the gutter and the markers in the commit panel use, so
/// a modified file is one colour wherever it is shown.
/// The inline values worked out for one file at one stop, and what they were worked out from.
///
/// `symbols::Hover`'s key made once more: a frame in which the text has not changed and the frame
/// that is showing has not changed costs two comparisons rather than a walk of the file's words.
pub(crate) struct InlineValues {
    revision: u64,
    frame: Option<i64>,
    /// How many answers the debugger had given when these were worked out.
    reads: u64,
    path: PathBuf,
    values: Vec<(usize, String)>,
}

/// True when a value is the debugger saying it has nothing to say.
///
/// Adapters spell this a dozen ways — `<variable not available>`, `<optimized out>`, `<not
/// available>`, `<error: ...>` — and what they share is the angle brackets: a value a program really
/// holds is a number, a string or a structure, and none of those is written that way. So the shape is
/// the test rather than a list of an adapter's own wordings, which would be a list that is wrong for
/// the next adapter.
fn is_unreadable(value: &str) -> bool {
    let value = value.trim();
    value.starts_with('<') && value.ends_with('>')
}

/// A value painted at the end of a line, cut to something a line can hold.
///
/// Far shorter than the tile's own limit: a value at the end of a line of code is a glance, and one
/// that ran off the edge of the pane would be worse than none. The tile shows the whole of it.
fn elide_value(value: &str) -> String {
    const LIMIT: usize = 48;
    let flat = value.replace('\n', " ");
    match flat.chars().count() > LIMIT {
        true => format!("{}\u{2026}", flat.chars().take(LIMIT).collect::<String>()),
        false => flat,
    }
}

/// The two halves of the debug tile's state, borrowed apart.
///
/// A component takes its own state mutably and what it draws immutably, which is the shape every
/// component in Unluminous has — and the borrow checker will not hand out both halves of one `&mut self`
/// through two method calls. One struct with two fields is what makes the split explicit rather than
/// something the caller has to spell out at each site.
struct DebugSplit<'a> {
    panel: &'a mut DebugPanel,
    debug: Option<&'a DebugState>,
}

fn split_the_debug(app: &mut UnluminousApp) -> DebugSplit<'_> {
    DebugSplit { panel: &mut app.debug_panel, debug: app.debug.as_ref() }
}

/// Whether two paths name the same file.
///
/// An adapter answers with whatever spelling the debug information holds, which on Windows is very
/// often a different case from the one the explorer opened — and a comparison that missed that would
/// leave the execution point invisible in a file that is plainly open. So the names are compared
/// without case there and exactly everywhere else, which is what each platform's file system means.
/// `canonicalize` is deliberately not used: it touches the disk, this is asked at every stop, and a
/// verbatim path is a thing `paths::plain` exists to stop travelling.
fn same_file(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    // Through `paths::plain` first, because a path Unluminous wrote down may be **verbatim** —
    // `\\?\C:\jason\dev\unluminous` — while the one an adapter answers with never is. That module records
    // where a verbatim path comes from and why one must not be allowed to travel; this is the second
    // place two of them have to be compared.
    let left = unluminous_terminal::paths::plain(left);
    let right = unluminous_terminal::paths::plain(right);
    if left == right {
        return true;
    }
    match cfg!(windows) {
        true => {
            let flatten = |path: &Path| path.to_string_lossy().to_lowercase().replace('/', "\\");
            flatten(&left) == flatten(&right)
        }
        false => false,
    }
}

/// Which **one-based** line an offset is on in a file that is not open.
///
/// The ownership rule's disk half: a file that is open is owned by its `Document` and every other
/// file is owned by the store, so a closed file's line numbers come from its own bytes, read at the
/// moment of use rather than watched — which is what `open_the_match` already does before jumping
/// into one. A file that cannot be read answers line one, which the adapter will then decline to
/// bind and say so about.
fn line_number_in_file(path: &Path, offset: usize) -> usize {
    // Read as a `Document` reads it. An offset is a byte into the text **a document holds**, which
    // has Windows line breaks normalised away; counting them in the raw bytes instead is off by one
    // byte per line before the offset, which for line 50 is a different line entirely. `task-1794`.
    let Ok(text) = unluminous_core::document::read_to_normalised_string(path) else {
        return 1;
    };
    text.as_bytes()[..offset.min(text.len())].iter().filter(|byte| **byte == b'\n').count() + 1
}

fn git_colour(state: unluminous_git::State) -> Color32 {
    match state {
        unluminous_git::State::Untracked => color::git_untracked(),
        unluminous_git::State::Added | unluminous_git::State::Copied => color::git_added(),
        unluminous_git::State::Unmerged => color::close(),
        unluminous_git::State::Ignored | unluminous_git::State::Unchanged => {
            color::text_faint().gamma_multiply(0.7)
        }
        _ => color::git_modified(),
    }
}

impl eframe::App for UnluminousApp {
    /// The window background. eframe asks for this every frame, so changing the opacity takes effect at
    /// once.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Fully transparent, because the window's own rounded rectangle is painted in `ui`. Anything
        // painted here would show outside the rounded corners.
        egui::Rgba::TRANSPARENT.to_array()
    }

    /// Settle the native child views, before the egui pass rather than inside it.
    ///
    /// The work is [`UnluminousApp::settle_the_native_views_before_the_pass`], in `app::frame`, because it
    /// is part of what a frame does. A trait method cannot live in another file, so this one is the
    /// hook and that one is the frame's.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.settle_the_native_views_before_the_pass(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Nothing on macOS: the compositor there takes the surface's alpha on its own.
        #[cfg(windows)]
        crate::services::windows_transparency::keep_transparent(_frame);
        // The one thing reconciling needs from the frame, taken while there is a frame to ask.
        self.browser.remember_window(_frame);
        UnluminousApp::ui(self, ui);
    }

    /// Write the settings, the pane sizes and what was open in the project before the window goes,
    /// and stop everything that is running.
    ///
    /// Nothing ever orphans a child on purpose: `Session`'s own drop shuts a pseudoterminal down,
    /// and this is the same path taken deliberately so it happens while the window is still here to
    /// wait for it.
    fn on_exit(&mut self) {
        // **What each node holds, asked one last time before anything is killed.** It is read from the
        // pseudoterminal, the native view and the chat's own thread, all three of which answer nothing once
        // they have been stopped. `space::Reading::OnTheWayOut` says which of the six values this moment can
        // answer for and why the other two are left out.
        self.note_the_live_state_into_the_nodes(space::Reading::OnTheWayOut);
        // **Before the sessions are killed**, because a screen is read out of a live terminal and a killed one
        // has nothing to read. `task-1908` for the canvas's terminals, `task-1945` for the tile's.
        self.write_the_screens_down();
        self.write_the_tab_screens_down();
        self.run.kill_everything();
        // Every program a node started, killed rather than dropped - `Live::forget`'s own note, and
        // `task-1769`'s 119 orphaned shells.
        self.space.live.stop_everything();
        // **Every modified tab, as a last resort** (`task-1984` A2). The two ways a person closes the
        // window ask `may_the_window_close` and stay open when a save failed, so by the time this runs
        // there is usually nothing left to write. What reaches here is the operating system closing
        // the window over Unluminous's head -- a log off, a shutdown -- where there is nobody to show a
        // notice to and nothing to be gained by refusing. So the failures are logged rather than
        // shown, and the text is written wherever it can be.
        for problem in self.save_every_modified_tab() {
            eprintln!("{problem}");
        }
        // `f64::MAX` so a write that failed a moment ago is still tried: this is the last chance
        // there is, and the two second wait exists for a window that is still drawing.
        self.write_the_space_if_it_changed(f64::MAX);
        self.write_settings();
        self.remember_the_project();
    }
}

/// Colouring a fenced code block with the grammar a plugin already supplies.
///
/// `unluminous-core` holds no plugin registry and must not learn about one, so it asks a question through
/// `CodeHighlighter` and this answers it — with exactly the two calls `colour_the_file` makes for a
/// source file, so a fence of Rust inside a document is coloured as a `.rs` file is. A language
/// nothing claims answers with nothing, and the block keeps the one code colour it always had.
struct PluginHighlighter<'a> {
    plugins: &'a crate::services::plugins::Plugins,
}

impl unluminous_core::CodeHighlighter for PluginHighlighter<'_> {
    fn colour(
        &self,
        language: &str,
        code: &str,
    ) -> Vec<(std::ops::Range<usize>, unluminous_core::Color)> {
        let Some(plugin) = self.plugins.for_language(language) else {
            return Vec::new();
        };
        let scheme = crate::services::plugins::scheme_of(plugin);
        if !plugin.grammar.markup {
            return unluminous_core::syntax::highlight(code, &plugin.grammar)
                .into_iter()
                .filter_map(|(range, token)| scheme.colour(token).map(|colour| (range, colour)))
                .collect();
        }
        // A fence of HTML is coloured the same way a `.html` file is: the outer pass reads the
        // markup and names the raw text elements, and the plugin that claims each one's language
        // colours it. The same two consequences as for a file hold here: a language nothing
        // claims answers with nothing, and the embedded list is one level deep.
        let mut spans: Vec<(std::ops::Range<usize>, unluminous_core::Color)> = Vec::new();
        let mut embedded: Vec<unluminous_core::syntax::Embedded> = Vec::new();
        unluminous_core::syntax::scan_with_embedded(
            code,
            &plugin.grammar,
            &mut embedded,
            |range, token| {
                if token != unluminous_core::Token::Text {
                    if let Some(colour) = scheme.colour(token) {
                        spans.push((range, colour));
                    }
                }
            },
        );
        for region in &embedded {
            let Some(inside) = self.plugins.for_language(&region.language) else {
                continue;
            };
            let inside_theme = crate::services::plugins::scheme_of(inside);
            let start = region.range.start;
            unluminous_core::syntax::scan(
                &code[region.range.clone()],
                &inside.grammar,
                |range, token| {
                    if token != unluminous_core::Token::Text {
                        if let Some(colour) = inside_theme.colour(token) {
                            spans.push((range.start + start..range.end + start, colour));
                        }
                    }
                },
            );
        }
        // The fence's reader walks the spans in order and stops at the first past its line.
        spans.sort_by_key(|(range, _)| range.start);
        spans
    }
}
