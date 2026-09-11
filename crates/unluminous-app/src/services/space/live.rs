//! The things on the canvas that are actually running.
//!
//! [`super::Space`] is a value: where a node is, what kind it is and what it is wired to. This is the
//! other half — the pseudoterminal behind a terminal node, the tab behind a browser node, the file
//! tree behind a folder node — keyed by the ids in that value.
//!
//! They are apart for the reason `unluminous-core` holds no `egui`: a process cannot be cloned,
//! written to a file or compared, and the model is all three. It is also what makes duplicating a view
//! cheap, and what makes it obvious that a copied node is a second node with a terminal of its own
//! rather than two drawings of one.

use std::collections::HashMap;

use crate::services::browser::BrowserTab;
use crate::services::file_tree::FileTree;

use super::node::NodeId;
use super::pipe::{self, Tap};

/// How often the pipes are read, in seconds.
///
/// Not every frame: reading what a program wrote builds a string for each line of the tail, and at
/// sixty frames a second that is twenty four thousand of them. Five times a second is far quicker
/// than anybody types and far cheaper than a frame.
pub const PIPE_INTERVAL: f32 = 0.2;

/// Everything behind the nodes that is not a value.
#[derive(Default)]
pub struct Live {
    terminals: HashMap<NodeId, unluminous_terminal::Session>,
    /// Whether a drag is selecting text in that node's grid, which is what `terminal_panel::grid`
    /// keeps for the tile.
    selecting: HashMap<NodeId, bool>,
    /// What each node's outgoing pipes have read, and what has been typed into it.
    taps: HashMap<NodeId, Tap>,
    browsers: HashMap<NodeId, BrowserTab>,
    trees: HashMap<NodeId, FileTree>,
    /// Which row a folder node's own cursor is on.
    ///
    /// A folder node has a selection of its own for the reason the explorer panel does: the row the
    /// arrow keys are on is a different question from the file that is showing.
    selected: HashMap<NodeId, std::path::PathBuf>,
    /// When the pipes were last read, in seconds of the window's own clock.
    read_at: f64,
}

impl std::fmt::Debug for Live {
    /// Written by hand because a `Session` holds a pseudoterminal and a `FileTree` holds a folder walk,
    /// and neither has a `Debug` worth printing. What is printed is what a test wants when an assertion
    /// about the canvas fails.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Live")
            .field("terminals", &self.terminals.len())
            .field("browsers", &self.browsers.len())
            .field("trees", &self.trees.len())
            .finish()
    }
}

impl Live {
    // ------------------------------------------------------------------------------- terminals

    pub fn terminal(&self, node: NodeId) -> Option<&unluminous_terminal::Session> {
        self.terminals.get(&node)
    }

    pub fn terminal_mut(&mut self, node: NodeId) -> Option<&mut unluminous_terminal::Session> {
        self.terminals.get_mut(&node)
    }

    pub fn has_a_terminal(&self, node: NodeId) -> bool {
        self.terminals.contains_key(&node)
    }

    /// A node's session and its selection flag together.
    ///
    /// `terminal_panel::grid` wants both at once, and asking for them one at a time would be two
    /// mutable borrows of this value. One call, two disjoint fields.
    pub fn terminal_and_selection(
        &mut self,
        node: NodeId,
    ) -> (Option<&mut unluminous_terminal::Session>, &mut bool) {
        let session = self.terminals.get_mut(&node);
        let selecting = self.selecting.entry(node).or_default();
        (session, selecting)
    }

    /// Put a session behind a node, replacing whatever was there.
    ///
    /// Replacing rather than refusing, because that is what `Restart` means and there is only ever one
    /// program a node. The session that was there is dropped, which shuts its pseudoterminal down —
    /// `Session`'s own `Drop`, taken deliberately rather than by forgetting about it.
    pub fn start_terminal(&mut self, node: NodeId, session: unluminous_terminal::Session) {
        self.terminals.insert(node, session);
        self.selecting.insert(node, false);
    }

    /// The flag `terminal_panel::grid` keeps while a drag is selecting.
    pub fn selecting(&mut self, node: NodeId) -> &mut bool {
        self.selecting.entry(node).or_default()
    }

    /// Read whatever every terminal's program has said. Answers whether anything is still running.
    ///
    /// `UiProvider::catch_up`'s bargain, applied to nodes: a program that printed while its node was
    /// scrolled off the canvas must not lose what it printed, so every session is pumped whether it was
    /// drawn or not. `Session::pump` reads what is on its queue and costs nothing when there is nothing.
    pub fn catch_up(&mut self) -> bool {
        let mut running = false;
        for session in self.terminals.values_mut() {
            session.pump();
            running |= session.is_running();
        }
        running
    }

    // ------------------------------------------------------------------------------- the pipes

    /// Carry one reading of every pipe, at most once every [`PIPE_INTERVAL`].
    ///
    /// `edges` is every connection carrying lines, as pairs of node ids. Answers what was sent, which
    /// is what a test asserts on and what the status bar says nothing about — a pipe that reported
    /// itself would report several times a second.
    pub fn carry_the_pipes(&mut self, now: f64, edges: &[(NodeId, NodeId)]) -> Vec<(NodeId, String)> {
        if edges.is_empty() || now - self.read_at < f64::from(PIPE_INTERVAL) {
            return Vec::new();
        }
        self.read_at = now;
        let mut sent = Vec::new();
        for (from, to) in edges {
            let Some(source) = self.terminals.get(from) else { continue };
            if !self.terminals.contains_key(to) {
                continue;
            }
            let tail = pipe::lines_of(&source.written_text(Some(pipe::TAIL)));
            let tap = self.taps.entry(*from).or_insert_with(|| Tap::following(&tail));
            let lines = tap.take(&tail);
            for line in lines {
                if let Some(target) = self.taps.get_mut(to) {
                    target.sent(&line);
                }
                if let Some(session) = self.terminals.get(to) {
                    session.send(format!("{line}\r").into_bytes());
                }
                sent.push((*to, line));
            }
        }
        sent
    }

    /// Start following a node's output from where it is now, which is what turning a pipe on does.
    ///
    /// Without this a pipe turned on would send the whole of a terminal's history into the other one.
    pub fn follow_from_here(&mut self, node: NodeId) {
        let tail = match self.terminals.get(&node) {
            Some(session) => pipe::lines_of(&session.written_text(Some(pipe::TAIL))),
            None => Vec::new(),
        };
        self.taps.insert(node, Tap::following(&tail));
    }

    /// Remember that a line was typed into a node by hand, so its echo is not piped on.
    ///
    /// `space send` goes through here as well as a pipe does: a line somebody sent is a line the
    /// target's shell will echo, and an echo forwarded is the same loop by another route.
    pub fn typed_into(&mut self, node: NodeId, line: &str) {
        self.taps.entry(node).or_default().sent(line);
    }

    // ------------------------------------------------------------------------------- browsers

    pub fn browser(&self, node: NodeId) -> Option<&BrowserTab> {
        self.browsers.get(&node)
    }

    pub fn browser_mut(&mut self, node: NodeId) -> Option<&mut BrowserTab> {
        self.browsers.get_mut(&node)
    }

    pub fn put_a_browser(&mut self, node: NodeId, tab: BrowserTab) {
        self.browsers.insert(node, tab);
    }

    /// Every browser tab on the canvas, which is what the window adds to the list it reconciles the
    /// one native child view against.
    pub fn browsers(&self) -> impl Iterator<Item = &BrowserTab> {
        self.browsers.values()
    }

    /// Which node holds the tab with this id, for an event that arrives naming a tab.
    pub fn node_of_browser(&self, tab: u64) -> Option<NodeId> {
        self.browsers.iter().find(|(_, held)| held.id == tab).map(|(node, _)| *node)
    }

    // ------------------------------------------------------------------------------- folders

    pub fn tree(&self, node: NodeId) -> Option<&FileTree> {
        self.trees.get(&node)
    }

    pub fn tree_mut(&mut self, node: NodeId) -> Option<&mut FileTree> {
        self.trees.get_mut(&node)
    }

    pub fn put_a_tree(&mut self, node: NodeId, tree: FileTree) {
        self.trees.insert(node, tree);
    }

    pub fn has_a_tree(&self, node: NodeId) -> bool {
        self.trees.contains_key(&node)
    }

    /// Which row a folder node's cursor is on.
    pub fn tree_selection(&self, node: NodeId) -> Option<&std::path::Path> {
        self.selected.get(&node).map(std::path::PathBuf::as_path)
    }

    /// Put its cursor on a row, or take it off.
    pub fn select_in_tree(&mut self, node: NodeId, path: Option<std::path::PathBuf>) {
        match path {
            Some(path) => {
                self.selected.insert(node, path);
            }
            None => {
                self.selected.remove(&node);
            }
        }
    }

    // ------------------------------------------------------------------------------- ending

    /// Stop and forget everything behind one node.
    ///
    /// The session is **killed** rather than dropped on its own, which is the path closing a terminal
    /// tab already takes: dropping shuts the reader loop down, and killing first is what stops the
    /// program as well. `task-1769` found 119 shells left behind by the difference.
    pub fn forget(&mut self, node: NodeId) {
        if let Some(mut session) = self.terminals.remove(&node) {
            session.kill();
        }
        self.selecting.remove(&node);
        self.taps.remove(&node);
        self.browsers.remove(&node);
        self.trees.remove(&node);
        self.selected.remove(&node);
    }

    /// Stop and forget everything behind a list of nodes, which is what deleting a view is.
    pub fn forget_all(&mut self, nodes: &[NodeId]) {
        for node in nodes {
            self.forget(*node);
        }
    }

    /// Everything the canvas is holding, which is what closing the window stops.
    pub fn nodes(&self) -> Vec<NodeId> {
        let mut found: Vec<NodeId> = self
            .terminals
            .keys()
            .chain(self.browsers.keys())
            .chain(self.trees.keys())
            .copied()
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    /// Stop everything. Called when the window closes and when a project is left.
    pub fn stop_everything(&mut self) {
        for node in self.nodes() {
            self.forget(node);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipe_is_read_no_more_often_than_its_interval() {
        // Reading builds a string a line, and a canvas that read every frame would build twenty four
        // thousand of them a second.
        let mut live = Live::default();
        let edges = vec![(1_u64, 2_u64)];
        // No terminals behind the ids, so nothing is carried; what is being asserted is the clock.
        assert!(live.carry_the_pipes(0.0, &edges).is_empty());
        let first = live.read_at;
        assert!(live.carry_the_pipes(0.05, &edges).is_empty());
        assert_eq!(live.read_at, first, "too soon to read again");
        assert!(live.carry_the_pipes(1.0, &edges).is_empty());
        assert_eq!(live.read_at, 1.0);
    }

    #[test]
    fn nothing_is_read_at_all_when_no_edge_carries_lines() {
        let mut live = Live::default();
        assert!(live.carry_the_pipes(10.0, &[]).is_empty());
        assert_eq!(live.read_at, 0.0, "a canvas with no pipes on it does no reading");
    }

    #[test]
    fn forgetting_a_node_takes_everything_behind_it() {
        let mut live = Live::default();
        live.put_a_tree(7, FileTree::new(std::env::temp_dir()));
        live.typed_into(7, "something");
        assert!(live.has_a_tree(7));
        assert_eq!(live.nodes(), vec![7]);
        live.forget(7);
        assert!(!live.has_a_tree(7));
        assert!(live.nodes().is_empty());
    }
}
