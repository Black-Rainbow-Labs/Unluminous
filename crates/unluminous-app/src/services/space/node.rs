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
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Terminal, Kind::Browser, Kind::Folder, Kind::Editor];

    /// The name the command line and the file on disk use: lower case, one word.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Terminal => "terminal",
            Kind::Browser => "browser",
            Kind::Folder => "folder",
            Kind::Editor => "editor",
        }
    }

    /// What a person reads in the add modal and on a node's header.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Terminal => "Terminal",
            Kind::Browser => "Web Browser",
            Kind::Folder => "Folder View",
            Kind::Editor => "File Editor",
        }
    }

    /// One line in the add modal, under the name.
    pub fn summary(self) -> &'static str {
        match self {
            Kind::Terminal => "A real terminal running this machine's own shell, or a program you name.",
            Kind::Browser => "A web page, with back, forward, reload and an address.",
            Kind::Folder => "A folder tree, with the explorer's own rows, icons and right click menu.",
            Kind::Editor => "A file, with the editing area's gutter, folding, breakpoints and find.",
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
}

impl State {
    pub fn kind(&self) -> Kind {
        match self {
            State::Terminal(_) => Kind::Terminal,
            State::Browser(_) => Kind::Browser,
            State::Folder(_) => Kind::Folder,
            State::Editor(_) => Kind::Editor,
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
            }),
            Kind::Browser => State::Browser(Browser { url: String::new() }),
            Kind::Folder => State::Folder(Folder {
                root: project.map(std::path::Path::to_path_buf),
                expanded: Vec::new(),
                filter: String::new(),
            }),
            Kind::Editor => State::Editor(Editor { path: None, caret: 0, scroll: 0.0 }),
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
}

/// A browser node: the address it is on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Browser {
    pub url: String,
}

/// A folder node: which folder, which folders inside it are open, and what is in its filter box.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Folder {
    /// Empty for the project folder.
    pub root: Option<std::path::PathBuf>,
    pub expanded: Vec<std::path::PathBuf>,
    pub filter: String,
}

/// An editor node: which file, and where it was being read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Editor {
    pub path: Option<std::path::PathBuf>,
    /// Where the caret was, as a byte offset, which is what every offset in Unluminous is.
    pub caret: usize,
    pub scroll: f32,
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
