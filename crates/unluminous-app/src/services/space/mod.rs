//! The Base of Infinite Space: the canvas of nodes `task-1904` asks for.
//!
//! `tasks/task-1904-base-of-infinite-space-tdd.md` is the design. This module is the **model** — what
//! views there are, what nodes are on them, where they sit and what is wired to what — and it holds no
//! window and no process. `space::live` owns the terminals, the browser tabs and the file trees, keyed
//! by the ids here; `components::space` draws it.
//!
//! That split is `unluminous-core`'s, applied inside one module: everything in this file is a unit
//! test with no window, no graphics card and no fonts, which is where the arithmetic that has to be
//! right belongs.
//!
//! ## Three rules the model keeps
//!
//! **An id is never an index.** Nodes are deleted and views are duplicated, and every index into a
//! list would shift underneath the edges naming it.
//!
//! **`tidy` runs after every change.** An edge whose node has gone, a current view that was deleted, a
//! chosen node that is no longer there — each is repaired in one place rather than at each of the
//! places that could cause it, which is `OpenFiles::tidy`'s own arrangement and its own reason.
//!
//! **A change marks the canvas dirty and nothing else writes.** The ticket asks for views "saved on
//! edit", and a file written on every frame would be a file written sixty times a second while
//! somebody drags a node. The window writes at the end of a frame on which something changed.

pub mod geometry;
pub mod launch;
pub mod live;
pub mod node;
pub mod pipe;
pub mod store;

use std::path::Path;

use egui::{Pos2, Rect, Vec2};

pub use geometry::Grip;
pub use node::{Camera, Edge, EdgeId, Kind, Node, NodeId, Pipe, State, ViewId};

/// One canvas: its name, what is on it, and where it is being looked at from.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub id: ViewId,
    pub name: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub camera: Camera,
    /// Which node the keyboard and the commands are about, when one has been chosen.
    pub chosen: Option<NodeId>,
}

impl View {
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
    /// top — which is also what [`Space::raise`] relies on.
    pub fn node_at(&self, world: Pos2) -> Option<NodeId> {
        self.nodes.iter().rev().find(|node| node.rect().contains(world)).map(|node| node.id)
    }
}

/// Every canvas a project has, and which one is showing.
#[derive(Debug, Clone, PartialEq)]
pub struct Space {
    views: Vec<View>,
    current: ViewId,
    /// One counter for nodes, edges and views, so no two things in a project ever share an id even
    /// across kinds. Counts up and is never reset — `OpenFiles::clock`'s own arrangement.
    next: u64,
    dirty: bool,
}

impl Default for Space {
    fn default() -> Self {
        Space::new()
    }
}

impl Space {
    /// One empty view called `Main`, which is what a project that has never had a canvas opens with.
    pub fn new() -> Space {
        let mut space = Space { views: Vec::new(), current: 0, next: 1, dirty: false };
        let id = space.add_view("Main");
        space.current = id;
        space.dirty = false;
        space
    }

    /// The next id, from the one counter.
    fn take_an_id(&mut self) -> u64 {
        let id = self.next;
        self.next = self.next.saturating_add(1);
        id
    }

    // ------------------------------------------------------------------------------- the views

    pub fn views(&self) -> &[View] {
        &self.views
    }

    pub fn current_id(&self) -> ViewId {
        self.current
    }

    /// The view that is showing. There is always one: [`Self::tidy`] makes a view when the last one
    /// is deleted, so nothing in the window has to answer for a canvas with no view on it.
    pub fn current(&self) -> &View {
        self.views.iter().find(|view| view.id == self.current).unwrap_or(&self.views[0])
    }

    pub fn current_mut(&mut self) -> &mut View {
        let current = self.current;
        let at = self.views.iter().position(|view| view.id == current).unwrap_or(0);
        &mut self.views[at]
    }

    pub fn view(&self, id: ViewId) -> Option<&View> {
        self.views.iter().find(|view| view.id == id)
    }

    /// The view called `name`, for a command line that names one the way a person does.
    ///
    /// `split_off_a_name`'s rule in `agent_tasks`: what is on the screen is the name, so that is what
    /// a command takes. A number is accepted too, because an id is what `space view list` prints.
    pub fn view_named(&self, name: &str) -> Option<ViewId> {
        if let Ok(id) = name.parse::<ViewId>() {
            if self.views.iter().any(|view| view.id == id) {
                return Some(id);
            }
        }
        self.views
            .iter()
            .find(|view| view.name.eq_ignore_ascii_case(name.trim()))
            .map(|view| view.id)
    }

    /// Make a view and return its id. It does not become the current one; [`Self::show_view`] does that.
    pub fn add_view(&mut self, name: &str) -> ViewId {
        let id = self.take_an_id();
        self.views.push(View {
            id,
            name: self.unused_name(name),
            nodes: Vec::new(),
            edges: Vec::new(),
            camera: Camera::default(),
            chosen: None,
        });
        self.dirty = true;
        id
    }

    /// `name`, with a number after it when something else is already called that.
    ///
    /// `unluminous_terminal::Tabs::names`' rule: two things called the same thing cannot be told
    /// apart, and a command line that names one by its name would reach whichever came first.
    fn unused_name(&self, name: &str) -> String {
        self.unused_name_apart_from(name, None)
    }

    /// The same, ignoring one view — which is the view being renamed, and is not a conflict with
    /// itself.
    fn unused_name_apart_from(&self, name: &str, apart_from: Option<ViewId>) -> String {
        let wanted = match name.trim() {
            "" => "View",
            other => other,
        };
        let taken = |tried: &str| {
            self.views
                .iter()
                .filter(|view| Some(view.id) != apart_from)
                .any(|view| view.name.eq_ignore_ascii_case(tried))
        };
        if !taken(wanted) {
            return wanted.to_owned();
        }
        (2..)
            .map(|number| format!("{wanted} {number}"))
            .find(|tried| !taken(tried))
            .unwrap_or_else(|| wanted.to_owned())
    }

    pub fn show_view(&mut self, id: ViewId) -> bool {
        if !self.views.iter().any(|view| view.id == id) {
            return false;
        }
        self.current = id;
        self.dirty = true;
        true
    }

    /// Call a view something else.
    ///
    /// **A view is not a conflict with itself.** `unused_name` walks every view, so renaming `Main` to
    /// `Main` found `Main` and answered `Main 2` — which meant opening Rename and pressing the button
    /// without editing anything renamed the view. Found by the `task-1904` review.
    pub fn rename_view(&mut self, id: ViewId, name: &str) -> bool {
        let Some(at) = self.views.iter().position(|view| view.id == id) else { return false };
        if self.views[at].name == name.trim() {
            return true;
        }
        let unused = self.unused_name_apart_from(name, Some(id));
        if self.views[at].name == unused {
            return true;
        }
        self.views[at].name = unused;
        self.dirty = true;
        true
    }

    /// Copy a view, its nodes, its edges and its camera, under new ids.
    ///
    /// **New ids, because a copy is a second thing.** Keeping the ids would make one node live on two
    /// canvases, and the live terminal behind it would then be drawn twice and typed into twice.
    pub fn duplicate_view(&mut self, id: ViewId) -> Option<ViewId> {
        let source = self.view(id)?.clone();
        let copy = self.add_view(&source.name);
        let mut renamed: Vec<(NodeId, NodeId)> = Vec::new();
        let mut nodes = Vec::new();
        for node in &source.nodes {
            let fresh = self.take_an_id();
            renamed.push((node.id, fresh));
            let mut copy = Node { id: fresh, ..node.clone() };
            // **A copy is a second node, so it is not on the original's conversation.** `task-1906` gives a
            // terminal node running an agent a session id it is handed with `--session-id` and resumed onto
            // with `--resume`, and cloning the state cloned that too — so a duplicated view held two nodes
            // both resuming one conversation, which is two agents writing into one thread. The copy starts a
            // fresh one, which is the same reasoning this function already applies to the node's own id: a
            // copy is a second thing, and keeping the id would make one node live on two canvases.
            if let State::Terminal(terminal) = &mut copy.state {
                terminal.session.clear();
            }
            nodes.push(copy);
        }
        let renamed_id = |was: NodeId| -> Option<NodeId> {
            renamed.iter().find(|(old, _)| *old == was).map(|(_, new)| *new)
        };
        let mut edges = Vec::new();
        for edge in &source.edges {
            let (Some(from), Some(to)) = (renamed_id(edge.from), renamed_id(edge.to)) else {
                continue;
            };
            edges.push(Edge { id: self.take_an_id(), from, to, pipe: edge.pipe });
        }
        let view = self.views.iter_mut().find(|view| view.id == copy)?;
        view.nodes = nodes;
        view.edges = edges;
        view.camera = source.camera;
        self.dirty = true;
        Some(copy)
    }

    /// Throw a view away. The nodes on it are the caller's to stop first — see
    /// [`live::Live::forget_view`].
    pub fn delete_view(&mut self, id: ViewId) -> bool {
        let Some(at) = self.views.iter().position(|view| view.id == id) else { return false };
        self.views.remove(at);
        self.tidy();
        self.dirty = true;
        true
    }

    // ------------------------------------------------------------------------------- the nodes

    /// Put a node of `kind` on the current view with its top left corner at `at`.
    pub fn add_node(&mut self, kind: Kind, at: Pos2, project: Option<&Path>) -> NodeId {
        let id = self.take_an_id();
        let state = State::new(kind, project);
        self.current_mut().nodes.push(Node {
            id,
            at,
            size: kind.opens_at(),
            title: String::new(),
            state,
        });
        self.current_mut().chosen = Some(id);
        self.dirty = true;
        id
    }

    /// Take a node off the current view, with every edge that named it.
    pub fn remove_node(&mut self, id: NodeId) -> bool {
        let view = self.current_mut();
        let Some(at) = view.nodes.iter().position(|node| node.id == id) else { return false };
        view.nodes.remove(at);
        self.tidy();
        self.dirty = true;
        true
    }

    pub fn move_node(&mut self, id: NodeId, to: Pos2) -> bool {
        let Some(node) = self.current_mut().node_mut(id) else { return false };
        if node.at == to {
            return true;
        }
        node.at = to;
        self.dirty = true;
        true
    }

    /// Resize a node, never below its kind's smallest.
    pub fn resize_node(&mut self, id: NodeId, size: Vec2) -> bool {
        let Some(node) = self.current_mut().node_mut(id) else { return false };
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
        let Some(node) = self.current_mut().node_mut(id) else { return false };
        if node.title == title {
            return true;
        }
        node.title = title.to_owned();
        self.dirty = true;
        true
    }

    /// Bring a node to the front, which is what clicking one does.
    ///
    /// The list is the drawing order, so "to the front" is "to the end". Nothing is marked dirty by
    /// it on its own: a canvas that wrote itself to disk every time somebody clicked a node would be
    /// writing on every click.
    pub fn raise(&mut self, id: NodeId) {
        let view = self.current_mut();
        let Some(at) = view.nodes.iter().position(|node| node.id == id) else { return };
        let node = view.nodes.remove(at);
        view.nodes.push(node);
    }

    /// Which node the keyboard is in, when it is in one.
    pub fn chosen(&self) -> Option<NodeId> {
        self.current().chosen
    }

    /// Choose a node, which is where the keys and a command with no `--from` go.
    ///
    /// **It is written down**, since `task-1914`: a canvas that came back with nothing chosen answered
    /// every key press with nothing, on a window whose editing area may not even be showing. So this
    /// marks the canvas dirty, and it compares first — clicking the node that is already chosen, which
    /// happens on every frame of a drag, must not ask for a write.
    pub fn choose(&mut self, id: Option<NodeId>) {
        let view = self.current_mut();
        if view.chosen == id {
            return;
        }
        view.chosen = id;
        self.dirty = true;
    }

    /// Change a node's own state — its address, its font size, the folders it has open.
    ///
    /// One function rather than a setter a field, so every change goes through the one place that
    /// marks the canvas dirty.
    pub fn change<R>(&mut self, id: NodeId, change: impl FnOnce(&mut State) -> R) -> Option<R> {
        let node = self.current_mut().node_mut(id)?;
        let answer = change(&mut node.state);
        self.dirty = true;
        Some(answer)
    }

    // ------------------------------------------------------------------------------- the edges

    /// Wire one node's output to another's input.
    ///
    /// Refused when either node is missing, when they are the same node, when that edge is already
    /// there, or when a pipe is asked for into a kind that cannot take one — see
    /// [`Self::takes_a_pipe`].
    pub fn connect(&mut self, from: NodeId, to: NodeId, carrying: Pipe) -> Result<EdgeId, String> {
        if from == to {
            return Err("A node cannot be wired to itself.".to_owned());
        }
        let view = self.current();
        let Some(source) = view.node(from) else { return Err(format!("There is no node {from}.")) };
        let Some(target) = view.node(to) else { return Err(format!("There is no node {to}.")) };
        if view.edge_between(from, to).is_some() {
            return Err("Those two are already wired that way round.".to_owned());
        }
        if carrying == Pipe::Lines && !Self::takes_a_pipe(source.kind(), target.kind()) {
            return Err(format!(
                "A {} node has nothing to type a line into. Wire it without a pipe and drive it with `space {}` instead.",
                target.kind().label().to_lowercase(),
                target.kind().name(),
            ));
        }
        let id = self.take_an_id();
        self.current_mut().edges.push(Edge { id, from, to, pipe: carrying });
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
        let view = self.current_mut();
        let Some(at) = view.edges.iter().position(|other| other.id == edge) else { return false };
        view.edges.remove(at);
        self.dirty = true;
        true
    }

    /// Turn an edge's pipe on or off.
    pub fn set_pipe(&mut self, edge: EdgeId, carrying: Pipe) -> Result<(), String> {
        let view = self.current();
        let Some(found) = view.edges.iter().find(|other| other.id == edge).copied() else {
            return Err(format!("There is no connection {edge}."));
        };
        let (Some(source), Some(target)) = (view.node(found.from), view.node(found.to)) else {
            return Err("That connection names a node that is not there.".to_owned());
        };
        if carrying == Pipe::Lines && !Self::takes_a_pipe(source.kind(), target.kind()) {
            return Err(format!(
                "A {} node has nothing to type a line into.",
                target.kind().label().to_lowercase()
            ));
        }
        let view = self.current_mut();
        if let Some(found) = view.edges.iter_mut().find(|other| other.id == edge) {
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
        self.current().edge_between(from, to).is_some()
    }

    // ------------------------------------------------------------------------------- repair

    /// Put right anything a change could have left inconsistent.
    ///
    /// Four things, each of which is a state a hand edited `space.conf` can also ask for: there is
    /// always at least one view, the current view is one that exists, an edge always names two nodes
    /// that are there, and a chosen node is one that is there.
    pub fn tidy(&mut self) {
        if self.views.is_empty() {
            let id = self.add_view("Main");
            self.current = id;
        }
        if !self.views.iter().any(|view| view.id == self.current) {
            self.current = self.views[0].id;
        }
        for view in &mut self.views {
            let ids: Vec<NodeId> = view.nodes.iter().map(|node| node.id).collect();
            view.edges.retain(|edge| ids.contains(&edge.from) && ids.contains(&edge.to));
            if let Some(chosen) = view.chosen {
                if !ids.contains(&chosen) {
                    view.chosen = None;
                }
            }
        }
    }

    /// Take the views a file was read into, and make the canvas consistent with them.
    ///
    /// **The counter carries on from the largest id in the file**, or the first node added after a
    /// project opened would be handed a number an edge already names. An edge arrives with no id,
    /// because a connection is named by the two nodes it joins and a hand written file giving two of
    /// them one number would be a file this could not repair; each is handed a fresh one here.
    ///
    /// `tidy` runs afterwards, which is what takes away an edge naming a node that is not there.
    pub fn adopt(&mut self, views: Vec<View>, current: ViewId) {
        let largest = views
            .iter()
            .flat_map(|view| std::iter::once(view.id).chain(view.nodes.iter().map(|node| node.id)))
            .max()
            .unwrap_or(0);
        self.views = views;
        self.next = largest.saturating_add(1);
        for at in 0..self.views.len() {
            for index in 0..self.views[at].edges.len() {
                let id = self.take_an_id();
                self.views[at].edges[index].id = id;
            }
        }
        self.current = current;
        self.tidy();
        self.dirty = false;
    }

    /// Whether something has changed since the last time the canvas was written down.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Say it has been written down.
    pub fn written(&mut self) {
        self.dirty = false;
    }

    /// Whether two canvases hold the same thing, ignoring whether either needs writing.
    ///
    /// **What is compared is what is written down.** `View::chosen` is part of that since `task-1914`, so it
    /// is compared: a canvas whose only difference is which node the keys go to is a canvas the file on disk
    /// no longer describes. `bring_the_current_view_to_life` puts the saved choice back after it has opened
    /// each node's tabs, so a window that opened a project and touched nothing still writes nothing.
    ///
    /// `dirty` is a field of `Space`, so a copy taken while it was clean can never be `==` to the same canvas
    /// once anything has marked it — which makes the derived comparison useless for the one question worth
    /// asking with it: *did that really change anything?* This is what `bring_the_current_view_to_life` asks,
    /// so a window that opened a project and touched nothing does not rewrite `space.conf`.
    pub fn holds_the_same_as(&self, other: &Space) -> bool {
        if self.current != other.current || self.next != other.next {
            return false;
        }
        if self.views.len() != other.views.len() {
            return false;
        }
        self.views.iter().zip(other.views.iter()).all(|(mine, theirs)| {
            mine.id == theirs.id
                && mine.name == theirs.name
                && mine.camera == theirs.camera
                && mine.nodes == theirs.nodes
                && mine.edges == theirs.edges
                && mine.chosen == theirs.chosen
        })
    }

    /// Mark it as changed, for a caller that changed something through a field rather than a method.
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    /// Every node on every view, which is what the window walks when it is stopping things.
    pub fn every_node(&self) -> impl Iterator<Item = (ViewId, &Node)> {
        self.views.iter().flat_map(|view| view.nodes.iter().map(move |node| (view.id, node)))
    }

    /// Which view a node is on, wherever it is.
    pub fn view_of(&self, node: NodeId) -> Option<ViewId> {
        self.views
            .iter()
            .find(|view| view.nodes.iter().any(|other| other.id == node))
            .map(|view| view.id)
    }

    /// The whole canvas as data, which is what `space view` prints and what a test asserts on.
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::json!({
            "current": self.current,
            "views": self.views.iter().map(view_as_json).collect::<Vec<_>>(),
        })
    }
}

/// One view as data.
fn view_as_json(view: &View) -> serde_json::Value {
    serde_json::json!({
        "id": view.id,
        "name": view.name,
        "chosen": view.chosen,
        "camera": { "x": view.camera.at.x, "y": view.camera.at.y, "zoom": view.camera.zoom },
        "nodes": view.nodes.iter().map(node_as_json).collect::<Vec<_>>(),
        "edges": view
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

/// One node as data, including what its kind has written down.
fn node_as_json(node: &Node) -> serde_json::Value {
    let mut value = serde_json::json!({
        "id": node.id,
        "kind": node.kind().name(),
        "title": node.title,
        "x": node.at.x,
        "y": node.at.y,
        "width": node.size.x,
        "height": node.size.y,
    });
    let map = value.as_object_mut().expect("it was built as an object");
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
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_canvas() -> Space {
        Space::new()
    }

    #[test]
    fn a_new_canvas_has_one_empty_view_and_nothing_to_write() {
        let space = a_canvas();
        assert_eq!(space.views().len(), 1);
        assert_eq!(space.current().name, "Main");
        assert!(space.current().nodes.is_empty());
        assert!(!space.is_dirty(), "nothing has been changed yet");
    }

    #[test]
    fn an_id_is_never_reused_even_across_kinds() {
        // An edge names nodes by id and a view names itself by one. Two things sharing an id is how a
        // node deleted on one view takes an edge off another.
        let mut space = a_canvas();
        let first = space.add_node(Kind::Terminal, Pos2::ZERO, None);
        let second = space.add_node(Kind::Browser, Pos2::new(700.0, 0.0), None);
        let edge = space.connect(first, second, Pipe::Off).expect("they are both there");
        let view = space.add_view("Second");
        let ids = [first, second, edge, view, space.current_id()];
        for (at, id) in ids.iter().enumerate() {
            assert!(!ids[..at].contains(id), "{id} was handed out twice");
        }
    }

    #[test]
    fn deleting_a_node_takes_its_wires_with_it() {
        let mut space = a_canvas();
        let terminal = space.add_node(Kind::Terminal, Pos2::ZERO, None);
        let browser = space.add_node(Kind::Browser, Pos2::new(700.0, 0.0), None);
        space.connect(terminal, browser, Pipe::Off).expect("both are there");
        assert_eq!(space.current().edges.len(), 1);
        space.remove_node(browser);
        assert!(space.current().edges.is_empty(), "an edge to a node that has gone is not an edge");
        assert_eq!(space.current().nodes.len(), 1);
    }

    #[test]
    fn a_wire_is_refused_when_it_would_be_wrong_rather_than_drawn_and_ignored() {
        let mut space = a_canvas();
        let terminal = space.add_node(Kind::Terminal, Pos2::ZERO, None);
        let editor = space.add_node(Kind::Editor, Pos2::new(700.0, 0.0), None);
        assert!(
            space.connect(terminal, terminal, Pipe::Off).is_err(),
            "a node cannot wire to itself"
        );
        assert!(space.connect(terminal, 9999, Pipe::Off).is_err(), "there is no such node");
        space.connect(terminal, editor, Pipe::Off).expect("control is fine");
        assert!(space.connect(terminal, editor, Pipe::Off).is_err(), "that edge is already there");

        // And a pipe into something with no input to type into names what does apply.
        let second = space.add_node(Kind::Terminal, Pos2::new(0.0, 600.0), None);
        let refusal =
            space.connect(second, editor, Pipe::Lines).expect_err("an editor takes no pipe");
        assert!(refusal.contains("space editor"), "the refusal names what does apply: {refusal}");
        space.connect(second, terminal, Pipe::Lines).expect("a terminal does take one");
    }

    #[test]
    fn a_pair_of_nodes_may_be_wired_both_ways_because_that_is_what_back_and_forth_is() {
        let mut space = a_canvas();
        let left = space.add_node(Kind::Terminal, Pos2::ZERO, None);
        let right = space.add_node(Kind::Terminal, Pos2::new(700.0, 0.0), None);
        space.connect(left, right, Pipe::Lines).expect("one way");
        space.connect(right, left, Pipe::Lines).expect("and back");
        assert_eq!(space.current().reaches(left), vec![right]);
        assert_eq!(space.current().reaches(right), vec![left]);
        assert!(space.may_reach(left, right) && space.may_reach(right, left));
    }

    #[test]
    fn a_node_may_only_act_on_what_it_is_wired_to() {
        let mut space = a_canvas();
        let terminal = space.add_node(Kind::Terminal, Pos2::ZERO, None);
        let browser = space.add_node(Kind::Browser, Pos2::new(700.0, 0.0), None);
        let other = space.add_node(Kind::Browser, Pos2::new(0.0, 700.0), None);
        space.connect(terminal, browser, Pipe::Off).expect("wired");
        assert!(space.may_reach(terminal, browser));
        assert!(!space.may_reach(terminal, other), "an unwired node is out of reach");
        assert!(!space.may_reach(browser, terminal), "an edge is one way round");
    }

    #[test]
    fn duplicating_a_view_copies_its_nodes_under_new_ids_and_keeps_the_wiring() {
        let mut space = a_canvas();
        let terminal = space.add_node(Kind::Terminal, Pos2::new(10.0, 20.0), None);
        let browser = space.add_node(Kind::Browser, Pos2::new(700.0, 20.0), None);
        space.connect(terminal, browser, Pipe::Off).expect("wired");
        let copy = space.duplicate_view(space.current_id()).expect("there is a view to copy");

        let source = space.current().clone();
        let made = space.view(copy).expect("it was just made").clone();
        assert_eq!(made.name, "Main 2", "a second thing called Main is numbered");
        assert_eq!(made.nodes.len(), source.nodes.len());
        assert_eq!(made.edges.len(), 1);
        for node in &made.nodes {
            assert!(!source.nodes.iter().any(|was| was.id == node.id), "a copy is a second thing");
        }
        let wire = made.edges[0];
        assert_eq!(wire.from, made.nodes[0].id);
        assert_eq!(wire.to, made.nodes[1].id);
        assert_eq!(made.nodes[0].at, Pos2::new(10.0, 20.0), "and it is in the same place");
    }

    /// A copied terminal node is not on the original's conversation.
    ///
    /// `task-1906` gives an agent node a session id it is handed with `--session-id` and resumed onto with
    /// `--resume`. `duplicate_view` clones a node's state, so the copy carried that id too — and a duplicated
    /// view then held two nodes both resuming one conversation, which is two agents writing into one thread.
    /// It is the same reasoning this function already applies to a node's own id: a copy is a second thing.
    #[test]
    fn a_copied_agent_node_starts_its_own_conversation() {
        let mut space = a_canvas();
        let agent = space.add_node(Kind::Terminal, Pos2::ZERO, None);
        space.change(agent, |state| {
            if let State::Terminal(terminal) = state {
                terminal.command = "claude".to_owned();
                terminal.session = "the-original-conversation".to_owned();
            }
        });
        let copy = space.duplicate_view(space.current_id()).expect("there is a view to copy");

        let made = space.view(copy).expect("it was just made");
        let copied = made.nodes.first().expect("the node was copied");
        match &copied.state {
            State::Terminal(terminal) => {
                assert_eq!(terminal.command, "claude", "it still runs the same program");
                assert!(
                    terminal.session.is_empty(),
                    "the copy is resuming {:?}, which the original is already on",
                    terminal.session,
                );
            }
            other => panic!("{other:?}"),
        }
        // And the original keeps its own, which is the half that would be worse to lose.
        match &space.current().node(agent).expect("it is there").state {
            State::Terminal(terminal) => {
                assert_eq!(terminal.session, "the-original-conversation");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn deleting_the_last_view_leaves_a_canvas_with_one_rather_than_none() {
        // Nothing in the window should have to answer for a canvas with no view on it.
        let mut space = a_canvas();
        space.delete_view(space.current_id());
        assert_eq!(space.views().len(), 1);
        assert!(space.view(space.current_id()).is_some());
    }

    #[test]
    fn deleting_the_current_view_moves_to_one_that_is_there() {
        let mut space = a_canvas();
        let second = space.add_view("Second");
        space.show_view(second);
        space.delete_view(second);
        assert_eq!(space.current().name, "Main");
    }

    #[test]
    fn two_views_are_never_called_the_same_thing() {
        let mut space = a_canvas();
        space.add_view("Main");
        space.add_view("Main");
        let names: Vec<&str> = space.views().iter().map(|view| view.name.as_str()).collect();
        assert_eq!(names, vec!["Main", "Main 2", "Main 3"]);
        space.rename_view(space.views()[2].id, "Main");
        assert_eq!(space.views()[2].name, "Main 3", "the first spelling nothing else has taken");
    }

    #[test]
    fn renaming_a_view_to_the_name_it_already_has_changes_nothing() {
        // A view is not a conflict with itself. Without that, opening Rename and pressing the button
        // without editing anything renamed `Main` to `Main 2` — which the `task-1904` review found,
        // and which is the one rename nobody would ever expect to change anything.
        let mut space = a_canvas();
        space.written();
        assert!(space.rename_view(space.current_id(), "Main"));
        assert_eq!(space.current().name, "Main");
        assert!(!space.is_dirty(), "and nothing was changed, so nothing has to be written");

        // Case is still what it was typed as, and a second view called the same thing is still
        // numbered.
        space.rename_view(space.current_id(), "Rendering");
        assert_eq!(space.current().name, "Rendering");
        let second = space.add_view("Rendering");
        assert_eq!(space.view(second).expect("it is there").name, "Rendering 2");
    }

    #[test]
    fn a_view_is_named_on_the_command_line_by_its_name_or_by_its_id() {
        let mut space = a_canvas();
        let second = space.add_view("Rendering");
        assert_eq!(space.view_named("Rendering"), Some(second));
        assert_eq!(space.view_named("rendering"), Some(second));
        assert_eq!(space.view_named(&second.to_string()), Some(second));
        assert_eq!(space.view_named("nothing like this"), None);
    }

    #[test]
    fn a_node_is_resized_no_smaller_than_its_kind_allows() {
        let mut space = a_canvas();
        let id = space.add_node(Kind::Terminal, Pos2::ZERO, None);
        space.resize_node(id, Vec2::new(10.0, 10.0));
        assert_eq!(space.current().node(id).expect("it is there").size, Kind::Terminal.smallest());
    }

    #[test]
    fn clicking_a_node_brings_it_to_the_front_and_the_top_one_is_what_is_under_the_pointer() {
        let mut space = a_canvas();
        let under = space.add_node(Kind::Folder, Pos2::ZERO, None);
        let over = space.add_node(Kind::Folder, Pos2::new(20.0, 20.0), None);
        assert_eq!(space.current().node_at(Pos2::new(40.0, 40.0)), Some(over));
        space.raise(under);
        assert_eq!(space.current().node_at(Pos2::new(40.0, 40.0)), Some(under));
        assert_eq!(space.current().nodes.len(), 2, "raising moves rather than copies");
    }

    #[test]
    fn a_change_marks_the_canvas_as_needing_writing_and_writing_it_clears_that() {
        let mut space = a_canvas();
        assert!(!space.is_dirty());
        let id = space.add_node(Kind::Terminal, Pos2::ZERO, None);
        assert!(space.is_dirty());
        space.written();
        assert!(!space.is_dirty());
        space.move_node(id, Pos2::new(5.0, 5.0));
        assert!(space.is_dirty());
        space.written();
        // Moving it to where it already is is not a change, so it does not cause a write.
        space.move_node(id, Pos2::new(5.0, 5.0));
        assert!(!space.is_dirty());
        // Nor does choosing or raising: both happen on every click.
        space.choose(Some(id));
        space.raise(id);
        assert!(!space.is_dirty());
    }

    #[test]
    fn the_bounds_are_every_node_and_nothing_on_an_empty_canvas() {
        let mut space = a_canvas();
        assert_eq!(space.current().bounds(), None);
        space.add_node(Kind::Folder, Pos2::new(100.0, 100.0), None);
        space.add_node(Kind::Folder, Pos2::new(-50.0, 400.0), None);
        let bounds = space.current().bounds().expect("there are two nodes");
        assert_eq!(bounds.min, Pos2::new(-50.0, 100.0));
        assert!(bounds.max.x >= 100.0 + Kind::Folder.opens_at().x);
    }

    #[test]
    fn the_whole_canvas_reads_back_as_data() {
        // Unluminous's rule is that what a person sees an agent can read, and a pane drawn with `egui`
        // is invisible unless something answers with it.
        let mut space = a_canvas();
        let terminal = space.add_node(Kind::Terminal, Pos2::new(10.0, 20.0), None);
        let browser = space.add_node(Kind::Browser, Pos2::new(700.0, 20.0), None);
        space.change(browser, |state| {
            if let State::Browser(browser) = state {
                browser.url = "https://example.com/".to_owned();
            }
        });
        space.connect(terminal, browser, Pipe::Off).expect("wired");
        let value = space.as_json();
        assert_eq!(value["current"], space.current_id());
        assert_eq!(value["views"][0]["nodes"][0]["kind"], "terminal");
        assert_eq!(value["views"][0]["nodes"][0]["x"], 10.0);
        assert_eq!(value["views"][0]["nodes"][1]["url"], "https://example.com/");
        assert_eq!(value["views"][0]["edges"][0]["from"], terminal);
        assert_eq!(value["views"][0]["edges"][0]["pipe"], "off");
    }
}
