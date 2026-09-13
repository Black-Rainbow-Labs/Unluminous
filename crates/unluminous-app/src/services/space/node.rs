//! What a node, an edge and a camera are.
//!
//! Plain values with no window behind them: a place in world points, a size, a kind and whatever that
//! kind needs written down. Nothing here draws and nothing here owns a process — `space::live` is
//! where a terminal's session and a browser's tab are, keyed by the ids in this file, for the reason
//! `unluminous-core` holds no `egui`: the arithmetic that has to be right is tested with no window,
//! no graphics card and no fonts.

use egui::{Pos2, Vec2};

/// A node's identity, from the counter on [`crate::services::space::Space`].
///
/// **An id is never an index.** Nodes are deleted and views are duplicated, and every index into a
/// list would shift underneath the edges that name it. `OpenFiles::clock` is the same decision about
/// the same problem.
pub type NodeId = u64;
pub type EdgeId = u64;
pub type ViewId = u64;

/// What a node holds.
///
/// A fifth kind adds a variant here and the compiler names every place that has to answer for it,
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
    /// that cannot say which node it is. See `services::space::live::Live::chat`.
    Chat,
    /// The Agent-Tasks board, on the canvas.
    ///
    /// **One board, drawn wherever it is asked for**, which is the opposite of [`Kind::Chat`] and for a
    /// reason rather than by omission: the board is one SQLite file with one watchdog behind it, and a
    /// second instance of it would be a second connection to the same tickets, each refreshing without the
    /// other. So a Tasks node draws the same provider the pane does, and two of them show the same board —
    /// which they should, because there is one.
    Tasks,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Terminal,
        Kind::Browser,
        Kind::Folder,
        Kind::Editor,
        Kind::Chat,
        Kind::Tasks,
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
        }
    }

    /// One line in the add modal, under the name.
    pub fn summary(self) -> &'static str {
        match self {
            Kind::Terminal => "A real terminal running this machine's own shell, or a program you name.",
            Kind::Browser => "A web page, with back, forward, reload and an address.",
            Kind::Folder => "A folder tree, with the explorer's own rows, icons and right click menu.",
            Kind::Editor => "A file, with the editing area's gutter, folding, breakpoints and find.",
            Kind::Chat => {
                "An agent, with its own conversation, that can drive the nodes it is wired to."
            }
            Kind::Tasks => "The Agent-Tasks board: the lanes, the backlog, the epics and a ticket.",
        }
    }

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
        }
    }

    /// The smallest it may be dragged to.
    ///
    /// A terminal below about twenty columns has stopped being a terminal and an editor with four
    /// lines in it has stopped being an editor, so each kind answers for itself rather than one
    /// number standing for all four.
    pub fn smallest(self) -> Vec2 {
        match self {
            Kind::Terminal => Vec2::new(200.0, 120.0),
            Kind::Browser => Vec2::new(260.0, 160.0),
            Kind::Folder => Vec2::new(180.0, 120.0),
            Kind::Editor => Vec2::new(240.0, 140.0),
            // Under about this the chat is a header and a composer with no room between them, which is
            // `agent_chat::surface`'s own floor: it draws nothing at all below 40 points of card.
            Kind::Chat => Vec2::new(260.0, 220.0),
            Kind::Tasks => Vec2::new(320.0, 240.0),
        }
    }
}

/// What a node of each kind has written down about itself.
///
/// One enum rather than four optional fields on [`Node`], so a browser node cannot be asked what
/// shell it runs and a terminal node cannot be given an address.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Terminal(Terminal),
    Browser(Browser),
    Folder(Folder),
    Editor(Editor),
    Chat(Chat),
    Tasks(Tasks),
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
    /// session id it is given and Codex names its own.
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
    /// drawn — `components::space::is_showing` sees to that on every pan — and a half-typed address must
    /// not go with it.
    ///
    /// **And deliberately not written to `space.conf`.** What a project comes back with is the address the
    /// node is *on*, which is [`Browser::url`]; a half-typed address is not state worth restoring, and
    /// `store::save` skips it.
    pub typed: String,
    /// Whether what is in the bar is the person's rather than the page's.
    ///
    /// Not written to `space.conf` either: a project comes back showing where the page is.
    pub editing: bool,
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
    /// same way: one line, relative to the project wherever the path is inside it.
    pub paths: Vec<std::path::PathBuf>,
    /// Which of them was showing, as an index into [`Editor::paths`].
    ///
    /// Out of range is treated as the first, which is what a hand edited file can ask for.
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
        self.paths.get(self.showing.min(self.paths.len().saturating_sub(1))).map(|path| path.as_path())
    }
}

/// An Agent-Chat node: which conversation it is holding, and how big it draws.
#[derive(Debug, Clone, PartialEq)]
pub struct Chat {
    /// The conversation this node is on, as `services::agent_chat::Store` names one.
    ///
    /// Empty on a node that has not been drawn yet; filled in from the chat the first time it opens, and
    /// written to `space.conf`, so a canvas comes back with each agent on the conversation it was left on
    /// rather than every one of them on the newest — which is what the pane does, because the pane is one.
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

/// One node on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: NodeId,
    /// The top left corner, in world points.
    pub at: Pos2,
    pub size: Vec2,
    /// What the header says. Empty means "call it after what it holds", which is the rule a terminal
    /// tab's name already keeps — so there is one way to undo a rename rather than a second command
    /// meaning "forget the name I gave".
    pub title: String,
    pub state: State,
}

impl Node {
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
/// moment two nodes were wired would be a canvas nobody wires anything on. See `space::pipe` for the
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub pipe: Pipe,
}

/// Where the canvas is being looked at from.
///
/// `at` is the world point drawn at the top left corner of the pane and `zoom` is how many screen
/// points one world point is. Both are written down per view, which is the ticket's "zoom level …
/// retained and restored".
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
    }

    #[test]
    fn a_node_opens_no_smaller_than_it_may_be_dragged_to() {
        // A kind whose opening size was under its own smallest would open already clamped, which
        // reads as a node that came up the wrong size.
        for kind in Kind::ALL {
            let opens = kind.opens_at();
            let smallest = kind.smallest();
            assert!(opens.x >= smallest.x && opens.y >= smallest.y, "{}", kind.name());
        }
    }

    #[test]
    fn the_ports_are_the_middles_of_the_two_side_edges() {
        let node = Node {
            id: 1,
            at: Pos2::new(100.0, 50.0),
            size: Vec2::new(200.0, 80.0),
            title: String::new(),
            state: State::new(Kind::Terminal, None),
        };
        assert_eq!(node.input_port(), Pos2::new(100.0, 90.0));
        assert_eq!(node.output_port(), Pos2::new(300.0, 90.0));
    }
}
