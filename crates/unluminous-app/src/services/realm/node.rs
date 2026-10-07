//! What a node, an edge and a camera are.
//!
//! Plain values with no window behind them: a place in world points, a size, a kind and whatever that
//! kind needs written down. Nothing here draws and nothing here owns a process — `realm::live` is
//! where a terminal's session and a browser's tab are, keyed by the ids in this file, for the reason
//! `unluminous-core` holds no `egui`: the arithmetic that has to be right is tested with no window,
//! no graphics card and no fonts.

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Pos2, Vec2};

/// A node's identity: a random 32 bit number, written in a realm file as eight lower case hex digits.
///
/// **An id is never an index.** Nodes are deleted and realms are duplicated, and every index into a
/// list would shift underneath the edges that name it. `OpenFiles::clock` is the same decision about
/// the same problem.
///
/// **Random rather than counted**, since `task-2202`: a realm file is committed with the project, and two
/// branches that each add a node to one would both hand out the next number from a counter, so the merge
/// would hold two nodes with one id. See [`crate::services::realm::fresh_id`].
pub type NodeId = u64;
pub type EdgeId = u64;

/// What a node holds.
///
/// A new kind adds a variant here and the compiler names every place that has to answer for it,
/// which is the bargain `app::dock::Panel` and `app::ViewMode` already make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Terminal,
    Browser,
    Folder,
    Editor,
    /// The Agent-Chat pane, on the canvas, with a conversation of this node's own.
    ///
    /// **A node holds its own chat rather than a second view of the pane's**, which is the one thing
    /// `task-1914` needs that a shared one could not give: *"a node that is able to connect similar to our
    /// terminal with claude etc so the agent knows how to control/read/etc the nodes it's connected to."*
    /// Two chats on one canvas wired to different nodes are two agents; two views of one chat are one agent
    /// that cannot say which node it is. See `services::realm::live::Live::chat`.
    Chat,
    /// The Agent-Tasks board, on the canvas.
    ///
    /// **One board, drawn wherever it is asked for**, which is the opposite of [`Kind::Chat`] and for a
    /// reason rather than by omission: the board is one SQLite file with one watchdog behind it, and a
    /// second instance of it would be a second connection to the same tickets, each refreshing without the
    /// other. So a Tasks node draws the same provider the pane does, and two of them show the same board —
    /// which they should, because there is one.
    Tasks,
    /// A picture from the project, drawn fitted to the node. `task-2202`.
    Image,
    /// A sound from the project, with play, pause, seek and volume. `task-2202`.
    Audio,
    /// A video from the project, played in the window's one native web view. `task-2202`.
    Video,
    /// A Markdown file in the editor, with the raw, side by side and preview buttons. `task-2202`.
    Note,
    /// A node of a kind this build does not know, read from a file a newer Unluminous wrote.
    ///
    /// **Not in [`Kind::ALL`]**, so nothing offers to make one. It is drawn as a placeholder that can be
    /// moved, resized, wired and deleted, and every key it was written with is written back. That is rule 2
    /// in §5.2 of `tasks/task-2199-realm-tdd.md`: an older build must never lose a node it cannot draw.
    Unknown,
}

impl Kind {
    pub const ALL: [Kind; 10] = [
        Kind::Terminal,
        Kind::Browser,
        Kind::Folder,
        Kind::Editor,
        Kind::Chat,
        Kind::Tasks,
        Kind::Image,
        Kind::Audio,
        Kind::Video,
        Kind::Note,
    ];

    /// The name the command line and the file on disk use: lower case, one word.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Terminal => "terminal",
            Kind::Browser => "browser",
            Kind::Folder => "folder",
            Kind::Editor => "editor",
            Kind::Chat => "chat",
            Kind::Tasks => "tasks",
            Kind::Image => "image",
            Kind::Audio => "audio",
            Kind::Video => "video",
            Kind::Note => "note",
            Kind::Unknown => "unknown",
        }
    }

    /// What a person reads in the add modal and on a node's header.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Terminal => "Terminal",
            Kind::Browser => "Web Browser",
            Kind::Folder => "Folder View",
            Kind::Editor => "File Editor",
            Kind::Chat => "Agent Chat",
            Kind::Tasks => "Agent Tasks",
            Kind::Image => "Image",
            Kind::Audio => "Audio",
            Kind::Video => "Video",
            Kind::Note => "Note",
            Kind::Unknown => "Unknown",
        }
    }

    /// One line in the add modal, under the name.
    pub fn summary(self) -> &'static str {
        match self {
            Kind::Terminal => {
                "A real terminal running this machine's own shell, or a program you name."
            }
            Kind::Browser => "A web page, with back, forward, reload and an address.",
            Kind::Folder => {
                "A folder tree, with the explorer's own rows, icons and right click menu."
            }
            Kind::Editor => {
                "A file, with the editing area's gutter, folding, breakpoints and find."
            }
            Kind::Chat => {
                "An agent, with its own conversation, that can drive the nodes it is wired to."
            }
            Kind::Tasks => "The Agent-Tasks board: the lanes, the backlog, the epics and a ticket.",
            Kind::Image => "A picture from this project, fitted to the node.",
            Kind::Audio => "A sound from this project, with play, pause, a seek bar and a volume.",
            Kind::Video => "A video from this project, with play, pause, a seek bar and a volume.",
            Kind::Note => "A Markdown note, written in the editor and read as a preview.",
            Kind::Unknown => {
                "A node a newer Unluminous made. It is kept exactly as it was written."
            }
        }
    }

    /// The kind called `name`. Never [`Kind::Unknown`], which nothing can ask for by name.
    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The size a node of this kind opens at, in world points.
    pub fn opens_at(self) -> Vec2 {
        match self {
            // About eighty columns and twenty four rows of the monospaced face at its default size,
            // which is the shape a shell expects and what `TERMINAL_WIDTH`'s own comment measures.
            Kind::Terminal => Vec2::new(620.0, 380.0),
            Kind::Browser => Vec2::new(900.0, 620.0),
            Kind::Folder => Vec2::new(320.0, 420.0),
            Kind::Editor => Vec2::new(760.0, 520.0),
            // The chat pane's own column with room for a few exchanges in it, which is about what the
            // panel is when somebody drags it wider than the 420 points it opens at.
            Kind::Chat => Vec2::new(480.0, 620.0),
            // Wide, because the board is lanes side by side and a narrow one shows one lane.
            Kind::Tasks => Vec2::new(1020.0, 640.0),
            Kind::Image => Vec2::new(420.0, 300.0),
            // A transport bar and the file's name, which is all a sound has to show.
            Kind::Audio => Vec2::new(420.0, 120.0),
            // Sixteen by nine under the header.
            Kind::Video => Vec2::new(640.0, 386.0),
            Kind::Note => Vec2::new(520.0, 420.0),
            Kind::Unknown => Vec2::new(320.0, 200.0),
        }
    }

    /// The smallest it may be dragged to.
    ///
    /// A terminal below about twenty columns has stopped being a terminal and an editor with four
    /// lines in it has stopped being an editor, so each kind answers for itself rather than one
    /// number standing for all of them.
    pub fn smallest(self) -> Vec2 {
        match self {
            Kind::Terminal => Vec2::new(200.0, 120.0),
            Kind::Browser => Vec2::new(260.0, 160.0),
            Kind::Folder => Vec2::new(180.0, 120.0),
            Kind::Editor => Vec2::new(240.0, 140.0),
            // Under about this the chat is a header and a composer with no room between them, which is
            // `agent_chat::surface`'s own floor: it draws nothing at all below 40 points of card.
            Kind::Chat => Vec2::new(300.0, 280.0),
            // **Wide and tall enough for the board to lay itself out**, which `task-1914`'s sweep found
            // 320 by 240 was not: the rail, the sprint name and the Add Task button filled the whole node
            // and the first lane's heading was drawn over its own cards. The board is one rail, one lane
            // and one card at the very least, and each of those has a size of its own. A floor is the
            // honest place to say so — the alternative is a board that can be dragged to a size at which
            // it draws something nobody can read.
            Kind::Tasks => Vec2::new(480.0, 360.0),
            Kind::Image => Vec2::new(120.0, 90.0),
            // The play button, the time and a seek bar wide enough to aim at.
            Kind::Audio => Vec2::new(280.0, 96.0),
            Kind::Video => Vec2::new(240.0, 160.0),
            Kind::Note => Vec2::new(240.0, 140.0),
            Kind::Unknown => Vec2::new(160.0, 90.0),
        }
    }

    /// Whether this kind shows a file from the project, named by its `file` key.
    pub fn holds_a_file(self) -> bool {
        matches!(self, Kind::Image | Kind::Audio | Kind::Video | Kind::Note)
    }
}

/// What a node of each kind has written down about itself.
///
/// One enum rather than optional fields on [`Node`], so a browser node cannot be asked what shell it
/// runs and a terminal node cannot be given an address.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Terminal(Terminal),
    Browser(Browser),
    Folder(Folder),
    Editor(Editor),
    Chat(Chat),
    Tasks(Tasks),
    Image(Image),
    Audio(Audio),
    Video(Video),
    Note(Note),
    Unknown(Unknown),
}

impl State {
    pub fn kind(&self) -> Kind {
        match self {
            State::Terminal(_) => Kind::Terminal,
            State::Browser(_) => Kind::Browser,
            State::Folder(_) => Kind::Folder,
            State::Editor(_) => Kind::Editor,
            State::Chat(_) => Kind::Chat,
            State::Tasks(_) => Kind::Tasks,
            State::Image(_) => Kind::Image,
            State::Audio(_) => Kind::Audio,
            State::Video(_) => Kind::Video,
            State::Note(_) => Kind::Note,
            State::Unknown(_) => Kind::Unknown,
        }
    }

    /// The name written in `node.<id>.kind`. For a node of a kind this build does not know, that is the name
    /// the file gave it rather than `unknown`.
    pub fn kind_name(&self) -> &str {
        match self {
            State::Unknown(unknown) => &unknown.kind,
            other => other.kind().name(),
        }
    }

    /// The file a picture, sound, video or note node shows, when it has one.
    pub fn file(&self) -> Option<&std::path::Path> {
        match self {
            State::Image(held) => held.file.as_deref(),
            State::Audio(held) => held.file.as_deref(),
            State::Video(held) => held.file.as_deref(),
            State::Note(held) => held.file.as_deref(),
            _ => None,
        }
    }

    /// Point a picture, sound, video or note node at another file. Nothing for any other kind.
    pub fn set_file(&mut self, to: PathBuf) {
        match self {
            State::Image(held) => held.file = Some(to),
            State::Audio(held) => held.file = Some(to),
            State::Video(held) => held.file = Some(to),
            State::Note(held) => held.file = Some(to),
            _ => {}
        }
    }

    /// The state a new node of `kind` starts with, in `project`.
    pub fn new(kind: Kind, project: Option<&std::path::Path>) -> State {
        match kind {
            Kind::Terminal => State::Terminal(Terminal {
                command: String::new(),
                folder: project.map(std::path::Path::to_path_buf),
                font_size: 0.0,
                session: String::new(),
                running: String::new(),
            }),
            Kind::Browser => State::Browser(Browser::default()),
            Kind::Folder => State::Folder(Folder {
                root: project.map(std::path::Path::to_path_buf),
                ..Folder::default()
            }),
            Kind::Editor => State::Editor(Editor::default()),
            Kind::Chat => State::Chat(Chat::default()),
            Kind::Tasks => State::Tasks(Tasks::default()),
            Kind::Image => State::Image(Image::default()),
            Kind::Audio => State::Audio(Audio::default()),
            Kind::Video => State::Video(Video::default()),
            Kind::Note => State::Note(Note::default()),
            Kind::Unknown => State::Unknown(Unknown::default()),
        }
    }
}

/// A terminal node: what it runs, where, how big its letters are, and what session it may resume.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Terminal {
    /// Empty for the machine's own shell, which is what `Settings::shell()` answers.
    pub command: String,
    /// Empty for the project folder.
    pub folder: Option<std::path::PathBuf>,
    /// `0.0` for the terminal's own setting, which is `terminal.font.size`.
    ///
    /// The ticket asks for the size to be changed per node, so a node that has been changed carries
    /// its own number and one that has not follows the setting — the rule a terminal tab's **name**
    /// already keeps, where an empty one means "call it after its program".
    pub font_size: f32,
    /// The agent conversation this node's program named, when it named one.
    ///
    /// Written down so the node can offer `Resume session` after the window has been closed and
    /// opened. Empty when the program is not an agent or has not said. See
    /// `services::agent_tasks::agent` for the same field and the same limitation: Claude takes a
    /// session id it is given and Codex names its own. It is in the realm's sidecar, because it is a
    /// conversation on this machine.
    pub session: String,
    /// The program that was in the foreground of this terminal when the canvas was last written down.
    ///
    /// **What was running, which is not [`Self::command`].** `command` is what the node was *given*, and what
    /// a person does is add a plain terminal node and then type `claude` into the shell — so a canvas that
    /// recorded only the command came back as a shell whatever had been running in it, which is what
    /// `task-1907` reports. Read from the pseudoterminal by `unluminous_terminal::Session::foreground`.
    ///
    /// **A program name, not a command line**, so what a restored node does with it is *offer* it rather than
    /// run it: the arguments, any `cd` somebody did and anything typed after the program are all gone, and
    /// running `claude` when what was running was `claude --model opus -p …` is running a different thing and
    /// calling it the same. Empty on a node whose terminal is at a prompt, on a node whose program has ended,
    /// and on Windows, where a ConPTY has no foreground group to ask for.
    pub running: String,
}

/// A browser node: the address it is on, and the address being typed into its bar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Browser {
    pub url: String,
    /// What is in the toolbar's address field right now.
    ///
    /// **On the node rather than in `egui`'s memory**, because a node scrolled off the canvas stops being
    /// drawn — `components::realm::is_showing` sees to that on every pan — and a half-typed address must
    /// not go with it.
    ///
    /// **And deliberately not written down.** What a project comes back with is the address the node is
    /// *on*, which is [`Browser::url`]; a half-typed address is not state worth restoring.
    pub typed: String,
    /// Whether what is in the bar is the person's rather than the page's.
    ///
    /// Not written down either: a realm comes back showing where the page is.
    pub editing: bool,
    /// Every address the node's page has been at, oldest first, and which of them it is on.
    ///
    /// **Written to the sidecar**, beside the camera, because where one person had been is not part of what
    /// the realm is. Empty for a node that has never had a page, and for one read from a sidecar written
    /// before `task-2203`, which comes back with only its address.
    pub history: Vec<String>,
    pub position: usize,
    /// The one element of the page this node shows, when it shows only one. `task-2203`.
    ///
    /// **In the realm file**, because a node that shows one element of a page is part of what the realm is.
    pub pin: Option<Pin>,
}

/// One element of one page, shown alone in a browser node.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pin {
    /// The address it was picked on. The pin applies while the page is there, ignoring the fragment.
    pub url: String,
    /// A CSS selector that finds exactly that element.
    pub selector: String,
}

impl Pin {
    /// Whether the page at `url` is the one this pin was picked on.
    ///
    /// **A local page is compared by its path in the project**, because its address carries the id of the tab
    /// it was opened in, `unluminous://tab-3/site/index.html`, and the id is new every run. Measured on
    /// `task-2203`: a pin made on `tab-3` was not shown after a restart that opened the page as `tab-2`. The
    /// fragment and a trailing `/` are ignored too.
    pub fn applies_to(&self, url: &str) -> bool {
        !self.selector.trim().is_empty() && the_page(&self.url) == the_page(url)
    }
}

/// An address with its fragment and trailing `/` taken off, and a local page's tab id taken out.
fn the_page(url: &str) -> String {
    let url = url.split('#').next().unwrap_or_default().trim_end_matches('/');
    for origin in ["unluminous://tab-", "http://unluminous.tab-", "https://unluminous.tab-"] {
        if let Some(rest) = url.strip_prefix(origin) {
            if let Some((id, path)) = rest.split_once('/') {
                if id.chars().all(|c| c.is_ascii_digit()) {
                    return format!("unluminous://tab/{path}");
                }
            }
        }
    }
    url.to_owned()
}

/// A folder node: which folder, which folders inside it are open, and what is in its filter box.
#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    /// Empty for the project folder.
    pub root: Option<std::path::PathBuf>,
    pub expanded: Vec<std::path::PathBuf>,
    pub filter: String,
    /// How far down its rows are scrolled, in the node's own points.
    ///
    /// **Written down, unlike the panel's**, because `task-1906` asks for a canvas to come back as it was and
    /// a list scrolled somewhere else is a list that moved while nobody was looking. `Live::scrolls` is where
    /// it lives while the window is open — a value that is read back off the `ScrollArea` every frame — and
    /// this is what survives the window closing.
    pub scroll: f32,
    /// How much bigger or smaller than its usual size this node draws its rows.
    ///
    /// A multiplier rather than a point size, because the explorer has none of its own: its rows, its
    /// indents and its lettering are the style guide's numbers, and one multiplier reaches every one of
    /// them through `explorer::View::at`. That is `task-1771`'s answer for the panel, kept per node —
    /// `task-1905` asks that the modifier wheel over a node zoom **that node**.
    pub zoom: f32,
}

impl Default for Folder {
    /// Written by hand for the zoom, which is 1.0 rather than nothing.
    fn default() -> Self {
        Self { root: None, expanded: Vec::new(), filter: String::new(), scroll: 0.0, zoom: 1.0 }
    }
}

/// An editor node: which files, which one was showing, and where it was being read.
#[derive(Debug, Clone, PartialEq)]
pub struct Editor {
    /// Every file open in this node, in the order the tabs are drawn.
    ///
    /// **A list since `task-1906`**, because `task-1905` gave a node a strip of tabs and one path brought
    /// back one of them. It is the shape `open-files.txt` already has for the panes, and it is written the
    /// same way: relative to the project wherever the path is inside it.
    pub paths: Vec<std::path::PathBuf>,
    /// Which of them was showing, as an index into [`Editor::paths`].
    ///
    /// Out of range is treated as the last, which is what a hand edited file can ask for.
    pub showing: usize,
    /// Where the caret was in the file that was showing, as a byte offset, which is what every offset in
    /// Unluminous is.
    ///
    /// **The file that was showing and not one per tab.** `open-files.txt` already holds a caret per tab for
    /// the panes, and a second list here would be a second thing to keep in step — see §7 of
    /// `tasks/task-1906-space-state-and-manager-tdd.md`.
    pub caret: usize,
    /// How far the file that was showing was scrolled.
    pub scroll: f32,
    /// How big this node's letters are, or `0.0` to follow `appearance.font.size`.
    ///
    /// **A size of its own, and the alternative is worse.** The editor's font is one setting for the whole
    /// window — `set_the_font_everywhere` exists because of it — so a node walking that number would
    /// resize every other tab, which is the fault `task-1657` fixed in the other direction. The `0.0`
    /// meaning "follow the setting" is the convention `Terminal::font_size` already uses. `task-1905`.
    pub font_size: f32,
}

impl Default for Editor {
    fn default() -> Self {
        Self { paths: Vec::new(), showing: 0, caret: 0, scroll: 0.0, font_size: 0.0 }
    }
}

impl Editor {
    /// The file that was showing, when this node had one.
    pub fn showing(&self) -> Option<&std::path::Path> {
        self.paths
            .get(self.showing.min(self.paths.len().saturating_sub(1)))
            .map(|path| path.as_path())
    }
}

/// An Agent-Chat node: which conversation it is holding, and how big it draws.
#[derive(Debug, Clone, PartialEq)]
pub struct Chat {
    /// The conversation this node is on, as `services::agent_chat::Store` names one.
    ///
    /// Empty on a node that has not been drawn yet; filled in from the chat the first time it opens, and
    /// written down, so a canvas comes back with each agent on the conversation it was left on rather than
    /// every one of them on the newest — which is what the pane does, because the pane is one.
    pub conversation: String,
    /// How much bigger or smaller than its usual size this node draws.
    ///
    /// A multiplier, for the reason [`Folder::zoom`] is one: a chat pane has no point size of its own, and
    /// `Look::zoomed_by` is what reaches every measurement in it. `task-1905`'s rule, kept for a fifth kind.
    pub zoom: f32,
}

impl Default for Chat {
    /// Written by hand for the zoom, which is 1.0 rather than nothing.
    fn default() -> Self {
        Self { conversation: String::new(), zoom: 1.0 }
    }
}

/// An Agent-Tasks node. It holds only how big it draws, because the board it shows is the window's one.
#[derive(Debug, Clone, PartialEq)]
pub struct Tasks {
    pub zoom: f32,
}

impl Default for Tasks {
    fn default() -> Self {
        Self { zoom: 1.0 }
    }
}

/// How a picture node fits its picture into the node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// The whole picture, as large as fits, with the rest of the node left empty.
    #[default]
    Contain,
    /// The node filled, with whatever of the picture does not fit cut off.
    Cover,
    /// One pixel of the picture to one point, from the top left corner.
    Actual,
}

impl Fit {
    pub const ALL: [Fit; 3] = [Fit::Contain, Fit::Cover, Fit::Actual];

    pub fn name(self) -> &'static str {
        match self {
            Fit::Contain => "contain",
            Fit::Cover => "cover",
            Fit::Actual => "actual",
        }
    }

    pub fn from_name(name: &str) -> Option<Fit> {
        Fit::ALL.into_iter().find(|fit| fit.name() == name.trim())
    }
}

/// A picture node. `task-2202`.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// The picture, inside the project. `None` when the file named a place outside it, which is refused.
    pub file: Option<PathBuf>,
    pub fit: Fit,
    /// How far the picture is zoomed and scrolled at [`Fit::Actual`]. In the sidecar, because two people
    /// looking at one picture do not want each other's scroll.
    pub zoom: f32,
    pub scroll: Vec2,
}

impl Default for Image {
    fn default() -> Self {
        Self { file: None, fit: Fit::Contain, zoom: 1.0, scroll: Vec2::ZERO }
    }
}

/// A sound node. `task-2202`.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub file: Option<PathBuf>,
    /// From 0 to 1.
    pub volume: f32,
    pub looping: bool,
    /// Where it was paused, in seconds. In the sidecar.
    pub position: f32,
}

impl Default for Audio {
    fn default() -> Self {
        Self { file: None, volume: 1.0, looping: false, position: 0.0 }
    }
}

/// A video node. `task-2202`.
#[derive(Debug, Clone, PartialEq)]
pub struct Video {
    pub file: Option<PathBuf>,
    pub volume: f32,
    pub looping: bool,
    pub muted: bool,
    /// Where it was, in seconds. In the sidecar.
    pub position: f32,
}

impl Default for Video {
    fn default() -> Self {
        Self { file: None, volume: 1.0, looping: false, muted: false, position: 0.0 }
    }
}

/// How a note is shown: the source, the source beside its preview, or the preview alone.
///
/// The same three modes a Markdown tab has, and a note node drives its tab's `OpenFile::view_mode` with
/// it. It is written in the realm file rather than the sidecar, because it is how the note is meant to be
/// read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteView {
    #[default]
    Raw,
    Side,
    Preview,
}

impl NoteView {
    pub const ALL: [NoteView; 3] = [NoteView::Raw, NoteView::Side, NoteView::Preview];

    pub fn name(self) -> &'static str {
        match self {
            NoteView::Raw => "raw",
            NoteView::Side => "side",
            NoteView::Preview => "preview",
        }
    }

    pub fn from_name(name: &str) -> Option<NoteView> {
        NoteView::ALL.into_iter().find(|view| view.name() == name.trim())
    }
}

/// A note node: a Markdown file open in the editor, inside the node. `task-2202`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Note {
    pub file: Option<PathBuf>,
    pub view: NoteView,
    /// `0.0` to follow `appearance.font.size`, which is [`Editor::font_size`]'s convention.
    pub font_size: f32,
    /// Where the caret and the scroll were. In the sidecar.
    pub caret: usize,
    pub scroll: f32,
}

/// A node of a kind this build does not know.
///
/// `keys` is every key the file had under the node apart from the seven the app owns (`kind`, `x`, `y`,
/// `width`, `height`, `z` and `title`), named by the part after `node.<id>.`, and they are written back
/// exactly as they were read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Unknown {
    pub kind: String,
    pub keys: BTreeMap<String, String>,
}

/// One node on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: NodeId,
    /// The top left corner, in world points.
    pub at: Pos2,
    pub size: Vec2,
    /// Where it is in the drawing order: higher is drawn later, so on top.
    ///
    /// **Sparse and written per node**, since `task-2202`. `space.conf` wrote a node's place in the list
    /// as part of every one of its keys, so raising one node renumbered every node above it and changed
    /// dozens of lines of a file that is now committed. Raising sets this to one more than the largest,
    /// which is one line.
    pub z: u32,
    /// What the header says. Empty means "call it after what it holds", which is the rule a terminal
    /// tab's name already keeps — so there is one way to undo a rename rather than a second command
    /// meaning "forget the name I gave".
    pub title: String,
    pub state: State,
    /// Every key under this node that this build did not read, written back after the ones it did.
    ///
    /// Rule 3 in §5.2 of the design: a newer Unluminous may add a key to a kind this one knows, and an
    /// older one that dropped it on save would quietly undo the newer one's work.
    pub extra: BTreeMap<String, String>,
    /// Why a path this node names was not believed, when one was not. Never written down: the key itself
    /// is kept in [`Node::extra`], so the file is unchanged.
    pub refused: Option<String>,
}

impl Node {
    /// A node of `kind` at `at`, at the size that kind opens at.
    pub fn new(id: NodeId, kind: Kind, at: Pos2, project: Option<&std::path::Path>) -> Node {
        Node {
            id,
            at,
            size: kind.opens_at(),
            z: 0,
            title: String::new(),
            state: State::new(kind, project),
            extra: BTreeMap::new(),
            refused: None,
        }
    }

    pub fn kind(&self) -> Kind {
        self.state.kind()
    }

    /// The rectangle this node covers, in world points.
    pub fn rect(&self) -> egui::Rect {
        egui::Rect::from_min_size(self.at, self.size)
    }

    /// Where the output port sits: the middle of the right hand edge, in world points.
    pub fn output_port(&self) -> Pos2 {
        Pos2::new(self.at.x + self.size.x, self.at.y + self.size.y / 2.0)
    }

    /// Where the input port sits: the middle of the left hand edge.
    pub fn input_port(&self) -> Pos2 {
        Pos2::new(self.at.x, self.at.y + self.size.y / 2.0)
    }
}

/// Whether an edge carries text as well as permission.
///
/// **Off unless somebody says so.** A shell's output is its prompt, its escape sequences and its echo
/// as well as its answers, and a canvas that started shovelling all of it into another shell the
/// moment two nodes were wired would be a canvas nobody wires anything on. See `realm::pipe` for the
/// rule that stops two terminals wired both ways looping for ever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pipe {
    #[default]
    Off,
    Lines,
}

impl Pipe {
    pub fn name(self) -> &'static str {
        match self {
            Pipe::Off => "off",
            Pipe::Lines => "lines",
        }
    }

    pub fn from_name(name: &str) -> Option<Pipe> {
        match name {
            "off" => Some(Pipe::Off),
            "lines" => Some(Pipe::Lines),
            _ => None,
        }
    }
}

/// One connection, from a node's output port to another node's input port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub pipe: Pipe,
    /// Keys under this edge this build did not read, kept for the reason [`Node::extra`] is.
    pub extra: BTreeMap<String, String>,
}

impl Edge {
    pub fn new(id: EdgeId, from: NodeId, to: NodeId, pipe: Pipe) -> Edge {
        Edge { id, from, to, pipe, extra: BTreeMap::new() }
    }
}

/// Where the canvas is being looked at from.
///
/// `at` is the world point drawn at the top left corner of the pane and `zoom` is how many screen
/// points one world point is. Both are written to the realm's sidecar, which is the ticket's "zoom
/// level … retained and restored", and not to the realm file, because every pan would otherwise be a
/// change to a file somebody commits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub at: Pos2,
    pub zoom: f32,
}

/// The ends of the zoom, which are Chordical's own numbers.
pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 2.5;

impl Default for Camera {
    fn default() -> Self {
        Camera { at: Pos2::ZERO, zoom: 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_named_once_and_reads_back() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_name(kind.name()), Some(kind));
            assert!(!kind.label().is_empty());
            assert!(!kind.summary().is_empty());
        }
        assert_eq!(Kind::from_name("nothing-like-this"), None);
        assert_eq!(Kind::from_name("unknown"), None, "nothing can ask for a placeholder by name");
    }

    #[test]
    fn a_node_opens_no_smaller_than_it_may_be_dragged_to() {
        // A kind whose opening size was under its own smallest would open already clamped, which
        // reads as a node that came up the wrong size.
        for kind in Kind::ALL.into_iter().chain([Kind::Unknown]) {
            let opens = kind.opens_at();
            let smallest = kind.smallest();
            assert!(opens.x >= smallest.x && opens.y >= smallest.y, "{}", kind.name());
        }
    }

    #[test]
    fn the_ports_are_the_middles_of_the_two_side_edges() {
        let mut node = Node::new(1, Kind::Terminal, Pos2::new(100.0, 50.0), None);
        node.size = Vec2::new(200.0, 80.0);
        assert_eq!(node.input_port(), Pos2::new(100.0, 90.0));
        assert_eq!(node.output_port(), Pos2::new(300.0, 90.0));
    }
}
