//! The Realm: the canvas of nodes `task-1904` asks for, kept since `task-2202` as `.realm` files in the
//! project.
//!
//! `tasks/task-1904-base-of-infinite-space-tdd.md` is the canvas and `tasks/task-2199-realm-tdd.md` is the
//! file. This module is the **model** — what nodes are on one realm, where they sit and what is wired to
//! what — and it holds no window and no process. `realm::live` owns the terminals, the browser tabs and the
//! file trees, keyed by the ids here; `realm::store` reads and writes the file and its sidecar;
//! `components::realm` draws it.
//!
//! That split is `unluminous-core`'s, applied inside one module: everything in this file is a unit
//! test with no window, no graphics card and no fonts, which is where the arithmetic that has to be
//! right belongs.
//!
//! ## One realm is one file, and one is open at a time
//!
//! A project holds any number of `.realm` files, by default under `.realm-files/`, and the panel shows one
//! of them. What used to be the views inside `space.conf` are separate files now, because a file is the unit
//! a person can commit, copy, rename and send to somebody. The window's `RealmState` holds the one that is
//! open; switching writes it and reads the next.
//!
//! ## Four rules the model keeps
//!
//! **An id is never an index**, and since `task-2202` it is random rather than counted. See [`fresh_id`].
//!
//! **`tidy` runs after every change.** An edge whose node has gone and a chosen node that is no longer
//! there are each repaired in one place rather than at each of the places that could cause them, which is
//! `OpenFiles::tidy`'s own arrangement and its own reason.
//!
//! **A change marks the realm dirty and nothing else writes.** The ticket asks for a canvas "saved on
//! edit", and a file written on every frame would be a file written sixty times a second while somebody
//! drags a node. The window writes at the end of a frame on which something changed.
//!
//! **A realm a newer Unluminous wrote may be read only**, and then nothing about what it holds can be
//! changed. See [`Access`].

pub mod geometry;
pub mod launch;
pub mod live;
pub mod node;
pub mod pipe;
pub mod player;
pub mod store;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{Pos2, Rect, Vec2};

pub use geometry::Grip;
pub use node::{Camera, Edge, EdgeId, Fit, Kind, Node, NodeId, NoteView, Pipe, State};

/// The realm file format this build writes. Bumped only when a reader of the previous format could
/// misread a file, which is when a key changes meaning. A new kind or a new key never bumps it. See the
/// table in §5.2 of `tasks/task-2199-realm-tdd.md`.
pub const FORMAT: u32 = 1;

/// The newest format this build can read at all.
pub const READS_UP_TO: u32 = 1;

/// The features this build implements, which a file can ask for in `realm.needs` before it may be saved.
///
/// Empty in the first version. A feature name is added the day a key arrives whose absence an older writer
/// would corrupt — a group that moves its children is the example the design gives.
pub const FEATURES: &[&str] = &[];

/// What a realm file says about the version of Unluminous that may read and write it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Format {
    /// The format the file was written in.
    pub format: u32,
    /// The smallest format a build must understand to open the file at all.
    pub reader: u32,
    /// The smallest format a build must understand to save it.
    pub writer: u32,
    /// Features a build must implement to save it.
    pub needs: Vec<String>,
}

impl Default for Format {
    fn default() -> Self {
        Format { format: FORMAT, reader: 1, writer: 1, needs: Vec::new() }
    }
}

impl Format {
    /// Whether this build may change a file written in this format, and why not when it may not.
    pub fn access(&self) -> Access {
        if self.writer > FORMAT {
            return Access::ReadOnly(format!(
                "Written by a newer Unluminous, which saves realm format {}. Open for reading only.",
                self.writer
            ));
        }
        let missing: Vec<&str> =
            self.needs.iter().map(String::as_str).filter(|need| !FEATURES.contains(need)).collect();
        if !missing.is_empty() {
            return Access::ReadOnly(format!(
                "Written by a newer Unluminous that uses {}. Open for reading only.",
                missing.join(", ")
            ));
        }
        Access::Edit
    }
}

/// Whether this realm may be changed.
///
/// **Reading a newer file is almost always fine; writing one is what can do damage.** An older build that
/// saved a file whose new keys it did not understand could undo what the newer build meant by them, so a
/// file can ask for a newer writer than it does a reader. SQLite's read and write version bytes and Delta's
/// `minReaderVersion` and `minWriterVersion` are the same idea.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Edit,
    /// Shown on the panel's banner, and every change is refused. Where the camera is and which node is
    /// chosen still change, because those are written to the sidecar rather than to the file.
    ReadOnly(String),
}

/// Where [`Realm::arrange`] moves a node in the stacking order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrange {
    /// In front of every other node.
    Front,
    /// One place nearer the front.
    Forward,
    /// One place nearer the back.
    Backward,
    /// Behind every other node.
    Back,
}

impl Arrange {
    /// Every way, in the order the menu lists them.
    pub const ALL: [Arrange; 4] =
        [Arrange::Front, Arrange::Forward, Arrange::Backward, Arrange::Back];

    /// The word the command line uses, `realm arrange <node> <how>`.
    pub fn name(self) -> &'static str {
        match self {
            Arrange::Front => "front",
            Arrange::Forward => "forward",
            Arrange::Backward => "backward",
            Arrange::Back => "back",
        }
    }

    /// The way a command line word names, if it names one.
    pub fn from_name(name: &str) -> Option<Arrange> {
        Arrange::ALL.into_iter().find(|how| how.name() == name)
    }

    /// What the menu row says.
    pub fn label(self) -> &'static str {
        match self {
            Arrange::Front => "Bring to Front",
            Arrange::Forward => "Bring Forward",
            Arrange::Backward => "Send Backward",
            Arrange::Back => "Send to Back",
        }
    }
}

/// What was last read from or written to disk for one realm, so a save that would write the same bytes
/// writes nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OnDisk {
    pub realm: Option<String>,
    pub sidecar: Option<String>,
}

/// A new id, eight hex digits' worth, that nothing in `taken` uses.
///
/// **Random, so two branches that each add a node do not both make the same one.** It is drawn from the
/// standard library's own randomly keyed hasher with a counter and the clock fed through it, which is
/// enough randomness for one person's file and needs no crate. Zero is never handed out, because a file
/// that names node `00000000` reads like a mistake.
pub fn fresh_id(taken: impl Fn(u64) -> bool) -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    loop {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos() as u64)
            .unwrap_or(0);
        hasher.write_u64(now);
        let id = hasher.finish() & 0xffff_ffff;
        if id != 0 && !taken(id) {
            return id;
        }
    }
}

/// One realm: a canvas, the nodes on it, the wires between them, and where it is being looked at from.
#[derive(Debug, Clone, PartialEq)]
pub struct Realm {
    /// Where the file is, relative to the project and with `/` separators.
    pub path: PathBuf,
    /// `realm.name`. The realm bar names a realm by its file name; this is written at creation and kept in
    /// step by a rename, so a file read on its own still says what it is.
    pub name: String,
    /// In drawing order: the last is on top. Sorted by [`Node::z`] when a file is read.
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub camera: Camera,
    /// Which node the keyboard and the commands are about, when one has been chosen.
    pub chosen: Option<NodeId>,
    pub format: Format,
    /// Every top level key this build did not read, written back exactly as it was. §5.2 rule 3.
    pub extra: BTreeMap<String, String>,
    pub access: Access,
    dirty: bool,
    pub on_disk: OnDisk,
}

impl Default for Realm {
    fn default() -> Self {
        Realm::new(Path::new(store::FIRST), "main")
    }
}

impl Realm {
    /// An empty realm that will be written to `path`.
    pub fn new(path: &Path, name: &str) -> Realm {
        Realm {
            path: path.to_path_buf(),
            name: name.to_owned(),
            nodes: Vec::new(),
            edges: Vec::new(),
            camera: Camera::default(),
            chosen: None,
            format: Format::default(),
            extra: BTreeMap::new(),
            access: Access::Edit,
            dirty: false,
            on_disk: OnDisk::default(),
        }
    }

    /// What the realm bar calls it: the file name without `.realm`.
    pub fn title(&self) -> String {
        title_of(&self.path)
    }

    /// Whether what it holds may be changed. See [`Access`].
    pub fn editable(&self) -> bool {
        self.access == Access::Edit
    }

    /// Why it is read only, when it is.
    pub fn read_only_because(&self) -> Option<&str> {
        match &self.access {
            Access::Edit => None,
            Access::ReadOnly(why) => Some(why),
        }
    }

    /// The kinds on it this build does not know, which is what `realm info` reports.
    pub fn unknown_kinds(&self) -> Vec<String> {
        let mut kinds: Vec<String> = self
            .nodes
            .iter()
            .filter_map(|node| match &node.state {
                State::Unknown(unknown) => Some(unknown.kind.clone()),
                _ => None,
            })
            .collect();
        kinds.sort();
        kinds.dedup();
        kinds
    }

    /// A new id no node and no edge on this realm uses.
    pub fn fresh_id(&self) -> u64 {
        fresh_id(|id| {
            self.nodes.iter().any(|node| node.id == id)
                || self.edges.iter().any(|edge| edge.id == id)
        })
    }

    // ------------------------------------------------------------------------------- reading

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    /// Every node this one has an edge **to**, which is what it is allowed to act on.
    pub fn reaches(&self, from: NodeId) -> Vec<NodeId> {
        self.edges.iter().filter(|edge| edge.from == from).map(|edge| edge.to).collect()
    }

    /// Every node with an edge **to** this one.
    pub fn reached_by(&self, to: NodeId) -> Vec<NodeId> {
        self.edges.iter().filter(|edge| edge.to == to).map(|edge| edge.from).collect()
    }

    /// The edge from one node to another, when there is one.
    pub fn edge_between(&self, from: NodeId, to: NodeId) -> Option<&Edge> {
        self.edges.iter().find(|edge| edge.from == from && edge.to == to)
    }

    /// The smallest rectangle holding every node, in world points. `None` on an empty canvas.
    pub fn bounds(&self) -> Option<Rect> {
        let mut found: Option<Rect> = None;
        for node in &self.nodes {
            found = Some(match found {
                Some(so_far) => so_far.union(node.rect()),
                None => node.rect(),
            });
        }
        found
    }

    /// The topmost node under a world point.
    ///
    /// Read from the end, because the list is the drawing order and the last one drawn is the one on
    /// top — which is also what [`Realm::raise`] relies on.
    pub fn node_at(&self, world: Pos2) -> Option<NodeId> {
        self.nodes.iter().rev().find(|node| node.rect().contains(world)).map(|node| node.id)
    }

    // ------------------------------------------------------------------------------- the nodes

    /// Put a node of `kind` with its top left corner at `at`, on top of everything else.
    ///
    /// On a read only realm nothing is added and the answer is `0`, which no node is.
    pub fn add_node(&mut self, kind: Kind, at: Pos2, project: Option<&Path>) -> NodeId {
        if !self.editable() {
            return 0;
        }
        let id = self.fresh_id();
        let mut node = Node::new(id, kind, at, project);
        node.z = self.top_z().map(|top| top + 1).unwrap_or(0);
        self.nodes.push(node);
        self.chosen = Some(id);
        self.dirty = true;
        id
    }

    /// The largest `z` on the realm.
    fn top_z(&self) -> Option<u32> {
        self.nodes.iter().map(|node| node.z).max()
    }

    /// Take a node off, with every edge that named it.
    pub fn remove_node(&mut self, id: NodeId) -> bool {
        if !self.editable() {
            return false;
        }
        let Some(at) = self.nodes.iter().position(|node| node.id == id) else { return false };
        self.nodes.remove(at);
        self.tidy();
        self.dirty = true;
        true
    }

    pub fn move_node(&mut self, id: NodeId, to: Pos2) -> bool {
        if !self.editable() {
            return false;
        }
        let Some(node) = self.node_mut(id) else { return false };
        if node.at == to {
            return true;
        }
        node.at = to;
        self.dirty = true;
        true
    }

    /// Resize a node, never below its kind's smallest.
    pub fn resize_node(&mut self, id: NodeId, size: Vec2) -> bool {
        if !self.editable() {
            return false;
        }
        let Some(node) = self.node_mut(id) else { return false };
        let smallest = node.kind().smallest();
        let size = Vec2::new(size.x.max(smallest.x), size.y.max(smallest.y));
        if node.size == size {
            return true;
        }
        node.size = size;
        self.dirty = true;
        true
    }

    /// Set a node's rectangle outright, which is what a resize drag produces.
    pub fn place_node(&mut self, id: NodeId, rect: Rect) -> bool {
        let moved = self.move_node(id, rect.min);
        let sized = self.resize_node(id, rect.size());
        moved && sized
    }

    /// Rename a node. An empty name puts it back to being called after what it holds.
    pub fn title_node(&mut self, id: NodeId, title: &str) -> bool {
        if !self.editable() {
            return false;
        }
        let Some(node) = self.node_mut(id) else { return false };
        if node.title == title {
            return true;
        }
        node.title = title.to_owned();
        self.dirty = true;
        true
    }

    /// Move a node up or down the stacking order, which is the node menu's `Arrange` submenu.
    ///
    /// The list is the drawing order, so a step forward swaps the node with the one after it and `Front`
    /// moves it to the end. Every node's [`Node::z`] is then renumbered from its place in the list, so the
    /// file says the same thing the screen does. **This is marked dirty**, unlike [`Self::raise`]: somebody
    /// chose this order on purpose and expects it to be there next time. Answers whether anything moved.
    ///
    /// `task-2200`. Clicking a node used to raise it, which made `Send Backward` last only until the next
    /// click on that node, so a click now chooses a node and leaves the order alone.
    pub fn arrange(&mut self, id: NodeId, how: Arrange) -> bool {
        if !self.editable() {
            return false;
        }
        let Some(at) = self.nodes.iter().position(|node| node.id == id) else { return false };
        let last = self.nodes.len() - 1;
        let to = match how {
            Arrange::Front => last,
            Arrange::Forward => (at + 1).min(last),
            Arrange::Backward => at.saturating_sub(1),
            Arrange::Back => 0,
        };
        if to == at {
            return false;
        }
        let node = self.nodes.remove(at);
        self.nodes.insert(to, node);
        for (z, node) in self.nodes.iter_mut().enumerate() {
            node.z = z as u32;
        }
        self.dirty = true;
        true
    }

    /// Bring a node to the front without writing anything down.
    ///
    /// What `realm screenshot` uses, so the node it photographs is not covered. The list is the drawing
    /// order, so "to the front" is "to the end", and [`Node::z`] becomes one more than the largest so the
    /// file says the same. **Nothing is marked dirty by it on its own**: the new `z` goes out with the next
    /// change, as one line. A person asks for the same thing through [`Self::arrange`].
    pub fn raise(&mut self, id: NodeId) {
        let top = self.top_z();
        let Some(at) = self.nodes.iter().position(|node| node.id == id) else { return };
        if at + 1 == self.nodes.len() {
            return;
        }
        let mut node = self.nodes.remove(at);
        node.z = top.map(|top| top + 1).unwrap_or(0);
        self.nodes.push(node);
    }

    /// Which node the keyboard is in, when it is in one.
    pub fn chosen(&self) -> Option<NodeId> {
        self.chosen
    }

    /// Choose a node, which is where the keys and a command with no `--from` go.
    ///
    /// **It is written down**, since `task-1914`: a canvas that came back with nothing chosen answered
    /// every key press with nothing, on a window whose editing area may not even be showing. So this
    /// marks the realm dirty, and it compares first — clicking the node that is already chosen, which
    /// happens on every frame of a drag, must not ask for a write. It goes to the sidecar, so a read only
    /// realm takes it too.
    pub fn choose(&mut self, id: Option<NodeId>) {
        if self.chosen == id {
            return;
        }
        self.chosen = id;
        self.dirty = true;
    }

    /// Change a node's own state — its address, its font size, the folders it has open.
    ///
    /// One function rather than a setter a field, so every change goes through the one place that
    /// marks the realm dirty.
    ///
    /// **Allowed on a read only realm**, because most of what is changed this way is where somebody is in a
    /// node — a caret, a scroll, the conversation a terminal named — and that goes to the sidecar. Whatever
    /// it changes that belongs in the realm file is never written, because a read only realm's file is not.
    pub fn change<R>(&mut self, id: NodeId, change: impl FnOnce(&mut State) -> R) -> Option<R> {
        let node = self.node_mut(id)?;
        let answer = change(&mut node.state);
        self.dirty = true;
        Some(answer)
    }

    // ------------------------------------------------------------------------------- the edges

    /// Wire one node's output to another's input.
    ///
    /// Refused when either node is missing, when they are the same node, when that edge is already
    /// there, when a pipe is asked for into a kind that cannot take one — see [`Self::takes_a_pipe`] — or
    /// when the realm is read only.
    pub fn connect(&mut self, from: NodeId, to: NodeId, carrying: Pipe) -> Result<EdgeId, String> {
        if let Some(why) = self.read_only_because() {
            return Err(why.to_owned());
        }
        if from == to {
            return Err("A node cannot be wired to itself.".to_owned());
        }
        let Some(source) = self.node(from) else { return Err(format!("There is no node {from}.")) };
        let Some(target) = self.node(to) else { return Err(format!("There is no node {to}.")) };
        if self.edge_between(from, to).is_some() {
            return Err("Those two are already wired that way round.".to_owned());
        }
        if carrying == Pipe::Lines && !Self::takes_a_pipe(source.kind(), target.kind()) {
            return Err(format!(
                "A {} node has nothing to type a line into. Wire it without a pipe and drive it with `realm {}` instead.",
                target.kind().label().to_lowercase(),
                target.kind().name(),
            ));
        }
        let id = self.fresh_id();
        self.edges.push(Edge::new(id, from, to, carrying));
        self.dirty = true;
        Ok(id)
    }

    /// Whether text may be piped from one kind into another.
    ///
    /// Only into a terminal, because only a terminal has an input a line can be typed into. Typing a
    /// line of shell output into somebody's file, or into an address bar, would be a change nobody
    /// asked for.
    pub fn takes_a_pipe(_from: Kind, to: Kind) -> bool {
        matches!(to, Kind::Terminal)
    }

    pub fn disconnect(&mut self, edge: EdgeId) -> bool {
        if !self.editable() {
            return false;
        }
        let Some(at) = self.edges.iter().position(|other| other.id == edge) else { return false };
        self.edges.remove(at);
        self.dirty = true;
        true
    }

    /// Turn an edge's pipe on or off.
    pub fn set_pipe(&mut self, edge: EdgeId, carrying: Pipe) -> Result<(), String> {
        if let Some(why) = self.read_only_because() {
            return Err(why.to_owned());
        }
        let Some(found) = self.edges.iter().find(|other| other.id == edge).cloned() else {
            return Err(format!("There is no connection {edge}."));
        };
        let (Some(source), Some(target)) = (self.node(found.from), self.node(found.to)) else {
            return Err("That connection names a node that is not there.".to_owned());
        };
        if carrying == Pipe::Lines && !Self::takes_a_pipe(source.kind(), target.kind()) {
            return Err(format!(
                "A {} node has nothing to type a line into.",
                target.kind().label().to_lowercase()
            ));
        }
        if let Some(found) = self.edges.iter_mut().find(|other| other.id == edge) {
            found.pipe = carrying;
        }
        self.dirty = true;
        Ok(())
    }

    /// Whether `from` may act on `to`, which is the whole of the permission model.
    ///
    /// An agent in a terminal node may drive the nodes that terminal is wired to and nothing else.
    /// The window's own agent passes no `from` at all and may drive everything, which is the ticket's
    /// *"the main agent for the IDE can control every single node"*.
    pub fn may_reach(&self, from: NodeId, to: NodeId) -> bool {
        self.edge_between(from, to).is_some()
    }

    // ------------------------------------------------------------------------------- repair

    /// Put right anything a change could have left inconsistent.
    ///
    /// Two things, each of which is a state a hand edited realm file can also ask for: an edge always
    /// names two nodes that are there, and a chosen node is one that is there.
    pub fn tidy(&mut self) {
        let ids: Vec<NodeId> = self.nodes.iter().map(|node| node.id).collect();
        self.edges.retain(|edge| ids.contains(&edge.from) && ids.contains(&edge.to));
        if let Some(chosen) = self.chosen {
            if !ids.contains(&chosen) {
                self.chosen = None;
            }
        }
    }

    /// A copy of this realm to be written to `path`, under new ids.
    ///
    /// **New ids, because a copy is a second thing.** Keeping them would make one node live on two realms,
    /// and the live terminal behind it — keyed by id — would then be drawn twice and typed into twice.
    ///
    /// **And a copied terminal is not on the original's conversation.** `task-1906` gives a terminal node
    /// running an agent a session id it is resumed onto with `--resume`, and a copy that kept it would be two
    /// agents writing into one thread.
    pub fn duplicated(&self, path: &Path, name: &str) -> Realm {
        let mut copy = Realm::new(path, name);
        copy.format = self.format.clone();
        copy.extra = self.extra.clone();
        copy.camera = self.camera;
        let mut renamed: Vec<(NodeId, NodeId)> = Vec::new();
        for node in &self.nodes {
            let fresh = copy.fresh_id();
            renamed.push((node.id, fresh));
            let mut made = Node { id: fresh, ..node.clone() };
            if let State::Terminal(terminal) = &mut made.state {
                terminal.session.clear();
            }
            copy.nodes.push(made);
        }
        let renamed_id = |was: NodeId| -> Option<NodeId> {
            renamed.iter().find(|(old, _)| *old == was).map(|(_, new)| *new)
        };
        for edge in &self.edges {
            let (Some(from), Some(to)) = (renamed_id(edge.from), renamed_id(edge.to)) else {
                continue;
            };
            let id = copy.fresh_id();
            copy.edges.push(Edge { id, from, to, pipe: edge.pipe, extra: edge.extra.clone() });
        }
        copy.dirty = true;
        copy
    }

    /// Whether something has changed since the last time the realm was written down.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Say it has been written down.
    pub fn written(&mut self) {
        self.dirty = false;
    }

    /// Whether two realms hold the same thing, ignoring whether either needs writing.
    ///
    /// **What is compared is what is written down**, including the chosen node since `task-1914`.
    /// `bring_the_current_view_to_life` puts the saved choice back after it has opened each node's tabs,
    /// so a window that opened a project and touched nothing still writes nothing.
    pub fn holds_the_same_as(&self, other: &Realm) -> bool {
        self.path == other.path
            && self.name == other.name
            && self.camera == other.camera
            && self.nodes == other.nodes
            && self.edges == other.edges
            && self.chosen == other.chosen
            && self.extra == other.extra
            && self.format == other.format
    }

    /// Mark it as changed, for a caller that changed something through a field rather than a method.
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    /// Move the camera by `by`, and remember that it moved.
    ///
    /// `task-1922` B8. `take_the_canvas_input` reached the camera directly, and
    /// `write_the_realm_if_it_changed` only writes when the realm is dirty -- so a canvas that was panned and
    /// left came back somewhere else. Dragging and zooming go through these two.
    pub fn pan_by(&mut self, by: Vec2) {
        self.camera.pan_by(by);
        self.dirty = true;
    }

    /// Zoom to `wanted`, keeping the world point under `at` where it is.
    ///
    /// The other half of [`Realm::pan_by`], and dirty for the same reason.
    pub fn zoom_at(&mut self, wanted: f32, origin: Pos2, at: Pos2) {
        self.camera.zoom_to(wanted, origin, at);
        self.dirty = true;
    }

    /// The whole realm as data, which is what `realm view` prints and what a test asserts on.
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "path": slashed(&self.path),
            "name": self.title(),
            "access": match &self.access {
                Access::Edit => "edit".to_owned(),
                Access::ReadOnly(_) => "read only".to_owned(),
            },
            "chosen": self.chosen,
            "camera": { "x": self.camera.at.x, "y": self.camera.at.y, "zoom": self.camera.zoom },
            "nodes": self.nodes.iter().map(node_as_json).collect::<Vec<_>>(),
            "edges": self
                .edges
                .iter()
                .map(|edge| serde_json::json!({
                    "id": edge.id,
                    "from": edge.from,
                    "to": edge.to,
                    "pipe": edge.pipe.name(),
                }))
                .collect::<Vec<_>>(),
        })
    }
}

/// What the realm bar calls the realm at `path`: its file name without `.realm`.
pub fn title_of(path: &Path) -> String {
    path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A path as a realm file writes one: `/` between the parts, whatever the platform.
pub fn slashed(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// One node as data, including what its kind has written down.
fn node_as_json(node: &Node) -> serde_json::Value {
    let mut value = serde_json::json!({
        "id": node.id,
        "kind": node.state.kind_name(),
        "title": node.title,
        "x": node.at.x,
        "y": node.at.y,
        "width": node.size.x,
        "height": node.size.y,
        "z": node.z,
    });
    let map = value.as_object_mut().expect("it was built as an object");
    if let Some(why) = &node.refused {
        map.insert("refused".into(), why.clone().into());
    }
    if let Some(file) = node.state.file() {
        map.insert("file".into(), file.display().to_string().into());
    }
    match &node.state {
        State::Terminal(terminal) => {
            map.insert("command".into(), terminal.command.clone().into());
            map.insert("session".into(), terminal.session.clone().into());
            map.insert("fontSize".into(), terminal.font_size.into());
            if let Some(folder) = &terminal.folder {
                map.insert("folder".into(), folder.display().to_string().into());
            }
        }
        State::Browser(browser) => {
            map.insert("url".into(), browser.url.clone().into());
        }
        State::Folder(folder) => {
            if let Some(root) = &folder.root {
                map.insert("root".into(), root.display().to_string().into());
            }
            map.insert(
                "expanded".into(),
                folder
                    .expanded
                    .iter()
                    .map(|path| serde_json::Value::from(path.display().to_string()))
                    .collect::<Vec<_>>()
                    .into(),
            );
            map.insert("scroll".into(), folder.scroll.into());
        }
        State::Editor(editor) => {
            // `path` is the file that is showing and `paths` is every tab. Both, because an agent asking
            // "what is in this node" wants the list and one asking "what am I looking at" wants the one —
            // and `path` is what every caller written before `task-1906` reads.
            if let Some(path) = editor.showing() {
                map.insert("path".into(), path.display().to_string().into());
            }
            map.insert(
                "paths".into(),
                editor
                    .paths
                    .iter()
                    .map(|path| serde_json::Value::from(path.display().to_string()))
                    .collect::<Vec<_>>()
                    .into(),
            );
            map.insert("caret".into(), editor.caret.into());
            map.insert("scroll".into(), editor.scroll.into());
        }
        State::Chat(chat) => {
            map.insert("conversation".into(), chat.conversation.clone().into());
            map.insert("zoom".into(), chat.zoom.into());
        }
        State::Tasks(tasks) => {
            map.insert("zoom".into(), tasks.zoom.into());
        }
        State::Image(image) => {
            map.insert("fit".into(), image.fit.name().into());
        }
        State::Audio(audio) => {
            map.insert("volume".into(), audio.volume.into());
            map.insert("loop".into(), audio.looping.into());
            map.insert("position".into(), audio.position.into());
        }
        State::Video(video) => {
            map.insert("volume".into(), video.volume.into());
            map.insert("loop".into(), video.looping.into());
            map.insert("muted".into(), video.muted.into());
            map.insert("position".into(), video.position.into());
        }
        State::Note(note) => {
            map.insert("view".into(), note.view.name().into());
            map.insert("caret".into(), note.caret.into());
            map.insert("scroll".into(), note.scroll.into());
        }
        State::Unknown(unknown) => {
            map.insert(
                "keys".into(),
                serde_json::Value::Object(
                    unknown
                        .keys
                        .iter()
                        .map(|(key, value)| (key.clone(), serde_json::Value::from(value.clone())))
                        .collect(),
                ),
            );
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_canvas() -> Realm {
        Realm::default()
    }

    #[test]
    fn a_new_realm_is_empty_and_has_nothing_to_write() {
        let realm = a_canvas();
        assert!(realm.nodes.is_empty());
        assert_eq!(realm.title(), "main");
        assert!(!realm.is_dirty(), "nothing has been changed yet");
        assert!(realm.editable());
    }

    /// **A canvas that was panned or zoomed has something to write down.** `task-1922` B8.
    #[test]
    fn panning_or_zooming_the_canvas_is_something_to_write_down() {
        let mut realm = a_canvas();
        realm.pan_by(Vec2::new(-40.0, 12.0));
        assert!(realm.is_dirty(), "a pan moved the camera, so the sidecar is out of date");
        assert_eq!(realm.camera.at, Pos2::new(40.0, -12.0), "and it really moved");

        realm.written();
        realm.zoom_at(1.5, Pos2::ZERO, Pos2::new(100.0, 100.0));
        assert!(realm.is_dirty(), "so did a zoom");
        assert!((realm.camera.zoom - 1.5).abs() < 0.001);
    }

    /// Item 10 of §8: an id is eight lower case hex digits' worth, and a realm never hands one out twice.
    #[test]
    fn ids_are_random_eight_hex_digit_numbers_that_never_collide_on_one_realm() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1_000_000 {
            let id = fresh_id(|_| false);
            assert!(id != 0 && id <= 0xffff_ffff, "{id:x} is not eight hex digits");
            seen.insert(id);
        }
        // A million draws from four billion: the birthday bound puts the expected number of repeats near
        // 116, so this is a statistical check that the draws are spread, not a promise of none.
        assert!(seen.len() > 999_500, "only {} distinct ids in a million", seen.len());
        let written = store::hex(fresh_id(|_| false));
        assert_eq!(written.len(), 8);
        assert!(written.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

        // Within one realm of the largest size a file is believed at, nothing is ever handed out twice,
        // because the realm asks about what it already holds.
        let mut realm = a_canvas();
        for at in 0..store::NODE_LIMIT {
            realm.add_node(Kind::Folder, Pos2::new(at as f32, 0.0), None);
        }
        let mut ids: Vec<NodeId> = realm.nodes.iter().map(|node| node.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), store::NODE_LIMIT);
    }

    #[test]
    fn deleting_a_node_takes_its_wires_with_it() {
        let mut realm = a_canvas();
        let terminal = realm.add_node(Kind::Terminal, Pos2::ZERO, None);
        let browser = realm.add_node(Kind::Browser, Pos2::new(700.0, 0.0), None);
        realm.connect(terminal, browser, Pipe::Off).expect("both are there");
        assert_eq!(realm.edges.len(), 1);
        realm.remove_node(browser);
        assert!(realm.edges.is_empty(), "an edge to a node that has gone is not an edge");
        assert_eq!(realm.nodes.len(), 1);
    }

    #[test]
    fn a_wire_is_refused_when_it_would_be_wrong_rather_than_drawn_and_ignored() {
        let mut realm = a_canvas();
        let terminal = realm.add_node(Kind::Terminal, Pos2::ZERO, None);
        let editor = realm.add_node(Kind::Editor, Pos2::new(700.0, 0.0), None);
        assert!(
            realm.connect(terminal, terminal, Pipe::Off).is_err(),
            "a node cannot wire to itself"
        );
        assert!(realm.connect(terminal, 9999, Pipe::Off).is_err(), "there is no such node");
        realm.connect(terminal, editor, Pipe::Off).expect("control is fine");
        assert!(realm.connect(terminal, editor, Pipe::Off).is_err(), "that edge is already there");

        // And a pipe into something with no input to type into names what does apply.
        let second = realm.add_node(Kind::Terminal, Pos2::new(0.0, 600.0), None);
        let refusal =
            realm.connect(second, editor, Pipe::Lines).expect_err("an editor takes no pipe");
        assert!(refusal.contains("realm editor"), "the refusal names what does apply: {refusal}");
        realm.connect(second, terminal, Pipe::Lines).expect("a terminal does take one");
    }

    #[test]
    fn a_pair_of_nodes_may_be_wired_both_ways_because_that_is_what_back_and_forth_is() {
        let mut realm = a_canvas();
        let left = realm.add_node(Kind::Terminal, Pos2::ZERO, None);
        let right = realm.add_node(Kind::Terminal, Pos2::new(700.0, 0.0), None);
        realm.connect(left, right, Pipe::Lines).expect("one way");
        realm.connect(right, left, Pipe::Lines).expect("and back");
        assert_eq!(realm.reaches(left), vec![right]);
        assert_eq!(realm.reaches(right), vec![left]);
        assert!(realm.may_reach(left, right) && realm.may_reach(right, left));
    }

    #[test]
    fn a_node_may_only_act_on_what_it_is_wired_to() {
        let mut realm = a_canvas();
        let terminal = realm.add_node(Kind::Terminal, Pos2::ZERO, None);
        let browser = realm.add_node(Kind::Browser, Pos2::new(700.0, 0.0), None);
        let other = realm.add_node(Kind::Browser, Pos2::new(0.0, 700.0), None);
        realm.connect(terminal, browser, Pipe::Off).expect("wired");
        assert!(realm.may_reach(terminal, browser));
        assert!(!realm.may_reach(terminal, other), "an unwired node is out of reach");
        assert!(!realm.may_reach(browser, terminal), "an edge is one way round");
    }

    #[test]
    fn duplicating_a_realm_copies_its_nodes_under_new_ids_and_keeps_the_wiring() {
        let mut realm = a_canvas();
        let terminal = realm.add_node(Kind::Terminal, Pos2::new(10.0, 20.0), None);
        let browser = realm.add_node(Kind::Browser, Pos2::new(700.0, 20.0), None);
        realm.connect(terminal, browser, Pipe::Off).expect("wired");
        let made = realm.duplicated(Path::new(".realm-files/main copy.realm"), "main copy");

        assert_eq!(made.title(), "main copy");
        assert_eq!(made.nodes.len(), realm.nodes.len());
        assert_eq!(made.edges.len(), 1);
        for node in &made.nodes {
            assert!(!realm.nodes.iter().any(|was| was.id == node.id), "a copy is a second thing");
        }
        let wire = &made.edges[0];
        assert_eq!(wire.from, made.nodes[0].id);
        assert_eq!(wire.to, made.nodes[1].id);
        assert_eq!(made.nodes[0].at, Pos2::new(10.0, 20.0), "and it is in the same place");
    }

    /// A copied terminal node is not on the original's conversation.
    #[test]
    fn a_copied_agent_node_starts_its_own_conversation() {
        let mut realm = a_canvas();
        let agent = realm.add_node(Kind::Terminal, Pos2::ZERO, None);
        realm.change(agent, |state| {
            if let State::Terminal(terminal) = state {
                terminal.command = "claude".to_owned();
                terminal.session = "the-original-conversation".to_owned();
            }
        });
        let made = realm.duplicated(Path::new("copy.realm"), "copy");
        match &made.nodes[0].state {
            State::Terminal(terminal) => {
                assert_eq!(terminal.command, "claude", "it still runs the same program");
                assert!(terminal.session.is_empty(), "the copy is resuming {:?}", terminal.session);
            }
            other => panic!("{other:?}"),
        }
        match &realm.node(agent).expect("it is there").state {
            State::Terminal(terminal) => {
                assert_eq!(terminal.session, "the-original-conversation");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_node_is_resized_no_smaller_than_its_kind_allows() {
        let mut realm = a_canvas();
        let id = realm.add_node(Kind::Terminal, Pos2::ZERO, None);
        realm.resize_node(id, Vec2::new(10.0, 10.0));
        assert_eq!(realm.node(id).expect("it is there").size, Kind::Terminal.smallest());
    }

    #[test]
    fn clicking_a_node_brings_it_to_the_front_and_the_top_one_is_what_is_under_the_pointer() {
        let mut realm = a_canvas();
        let under = realm.add_node(Kind::Folder, Pos2::ZERO, None);
        let over = realm.add_node(Kind::Folder, Pos2::new(20.0, 20.0), None);
        assert_eq!(realm.node_at(Pos2::new(40.0, 40.0)), Some(over));
        let top = realm.node(over).expect("there").z;
        realm.raise(under);
        assert_eq!(realm.node_at(Pos2::new(40.0, 40.0)), Some(under));
        assert_eq!(realm.nodes.len(), 2, "raising moves rather than copies");
        assert_eq!(realm.node(under).expect("there").z, top + 1, "one line: its own z");
        assert_eq!(realm.node(over).expect("there").z, top, "and nothing else's");
    }

    #[test]
    fn arranging_a_node_moves_it_one_place_or_to_either_end_and_is_written_down() {
        let mut realm = a_canvas();
        let a = realm.add_node(Kind::Folder, Pos2::ZERO, None);
        let b = realm.add_node(Kind::Folder, Pos2::ZERO, None);
        let c = realm.add_node(Kind::Folder, Pos2::ZERO, None);
        let order = |realm: &Realm| realm.nodes.iter().map(|node| node.id).collect::<Vec<_>>();
        realm.written();
        assert!(realm.arrange(c, Arrange::Backward));
        assert_eq!(order(&realm), vec![a, c, b]);
        assert!(realm.is_dirty(), "an order somebody chose is written down");
        assert!(realm.arrange(a, Arrange::Forward));
        assert_eq!(order(&realm), vec![c, a, b]);
        assert!(realm.arrange(c, Arrange::Front));
        assert_eq!(order(&realm), vec![a, b, c]);
        assert!(realm.arrange(c, Arrange::Back));
        assert_eq!(order(&realm), vec![c, a, b]);
        assert!(!realm.arrange(c, Arrange::Back), "already at the back moves nothing");
        assert!(!realm.arrange(b, Arrange::Forward), "already in front moves nothing");
        let zs: Vec<u32> = realm.nodes.iter().map(|node| node.z).collect();
        assert_eq!(zs, vec![0, 1, 2], "the file's z follows the drawing order");
        assert_eq!(realm.node_at(Pos2::new(5.0, 5.0)), Some(b), "the front one is what is hit");
    }

    #[test]
    fn a_change_marks_the_realm_as_needing_writing_and_writing_it_clears_that() {
        let mut realm = a_canvas();
        assert!(!realm.is_dirty());
        let id = realm.add_node(Kind::Terminal, Pos2::ZERO, None);
        assert!(realm.is_dirty());
        realm.written();
        assert!(!realm.is_dirty());
        realm.move_node(id, Pos2::new(5.0, 5.0));
        assert!(realm.is_dirty());
        realm.written();
        // Moving it to where it already is is not a change, so it does not cause a write.
        realm.move_node(id, Pos2::new(5.0, 5.0));
        assert!(!realm.is_dirty());
        // Nor does choosing what is already chosen, or raising: both happen on every click.
        realm.choose(Some(id));
        realm.raise(id);
        assert!(!realm.is_dirty());
    }

    /// Item 4 of §8, the model's half: on a read only realm a change to what it holds is refused and marks
    /// nothing, while where it is looked at from still moves.
    #[test]
    fn a_read_only_realm_refuses_every_change_to_what_it_holds() {
        let mut realm = a_canvas();
        let id = realm.add_node(Kind::Terminal, Pos2::ZERO, None);
        let other = realm.add_node(Kind::Terminal, Pos2::new(700.0, 0.0), None);
        realm.written();
        realm.access = Access::ReadOnly("newer".to_owned());

        assert_eq!(realm.add_node(Kind::Folder, Pos2::ZERO, None), 0);
        assert!(!realm.move_node(id, Pos2::new(9.0, 9.0)));
        assert!(!realm.title_node(id, "renamed"));
        assert!(!realm.remove_node(id));
        assert!(realm.connect(id, other, Pipe::Off).is_err());
        assert!(!realm.is_dirty(), "nothing was changed, so nothing is to be written");
        assert_eq!(realm.nodes.len(), 2);
        assert_eq!(realm.node(id).expect("there").title, "");
    }

    #[test]
    fn the_bounds_are_every_node_and_nothing_on_an_empty_canvas() {
        let mut realm = a_canvas();
        assert_eq!(realm.bounds(), None);
        realm.add_node(Kind::Folder, Pos2::new(100.0, 100.0), None);
        realm.add_node(Kind::Folder, Pos2::new(-50.0, 400.0), None);
        let bounds = realm.bounds().expect("there are two nodes");
        assert_eq!(bounds.min, Pos2::new(-50.0, 100.0));
        assert!(bounds.max.x >= 100.0 + Kind::Folder.opens_at().x);
    }

    #[test]
    fn the_whole_canvas_reads_back_as_data() {
        // Unluminous's rule is that what a person sees an agent can read, and a pane drawn with `egui`
        // is invisible unless something answers with it.
        let mut realm = a_canvas();
        let terminal = realm.add_node(Kind::Terminal, Pos2::new(10.0, 20.0), None);
        let browser = realm.add_node(Kind::Browser, Pos2::new(700.0, 20.0), None);
        realm.change(browser, |state| {
            if let State::Browser(browser) = state {
                browser.url = "https://example.com/".to_owned();
            }
        });
        realm.connect(terminal, browser, Pipe::Off).expect("wired");
        let value = realm.as_json();
        assert_eq!(value["path"], ".realm-files/main.realm");
        assert_eq!(value["nodes"][0]["kind"], "terminal");
        assert_eq!(value["nodes"][0]["x"], 10.0);
        assert_eq!(value["nodes"][1]["url"], "https://example.com/");
        assert_eq!(value["edges"][0]["from"], terminal);
        assert_eq!(value["edges"][0]["pipe"], "off");
    }

    #[test]
    fn a_newer_writer_or_a_feature_this_build_lacks_opens_read_only() {
        assert_eq!(Format::default().access(), Access::Edit);
        let newer = Format { writer: 2, ..Format::default() };
        assert!(matches!(newer.access(), Access::ReadOnly(why) if why.contains('2')));
        let groups = Format { needs: vec!["groups".to_owned()], ..Format::default() };
        assert!(matches!(groups.access(), Access::ReadOnly(why) if why.contains("groups")));
    }
}
