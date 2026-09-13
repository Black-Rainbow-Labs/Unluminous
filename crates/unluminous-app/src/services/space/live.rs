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
    /// Which nodes have had something typed into them since the window opened.
    ///
    /// **Separate from `taps`, which every started terminal has an entry in**: `follow_from_here` makes one the
    /// moment a session starts, so `taps` answers "this node exists" rather than "somebody has used it". See
    /// [`Self::has_been_used`] for what depends on the difference. `task-1907`.
    typed: std::collections::HashSet<NodeId>,
    browsers: HashMap<NodeId, BrowserTab>,
    trees: HashMap<NodeId, FileTree>,
    /// Which row a folder node's own cursor is on.
    ///
    /// A folder node has a selection of its own for the reason the explorer panel does: the row the
    /// arrow keys are on is a different question from the file that is showing.
    selected: HashMap<NodeId, std::path::PathBuf>,
    /// How far down a folder node's rows are scrolled, in the node's own points.
    ///
    /// **Held here rather than in `egui`'s own `ScrollArea` state**, and that is not a preference. A
    /// node's contents are drawn into a layer made by hand with `set_sublayer`, which puts the layer in
    /// the order list and registers **no `AreaState`** — only `egui::Area` does that and `set_state` is
    /// `pub(crate)`. `Areas::layer_id_at` reads that map, `Context::rect_contains_pointer` asks it, and a
    /// `ScrollArea` decides whether the wheel is its own by asking exactly that. So no `ScrollArea`
    /// inside a node can ever take the wheel, whatever it is handed. `task-1905` is the report — *"I
    /// can't scroll the node"* — and the answer is that the window reads the wheel and tells the explorer
    /// where to scroll, through the `View::scroll_to` and `ExplorerOutcome::scroll` pair the panel's own
    /// zoom already uses.
    ///
    /// Not written to `space.conf`: `Folder::expanded` is what a project comes back with, and a scroll
    /// into a tree whose folders may have been opened or shut since means nothing.
    scrolls: HashMap<NodeId, f32>,
    /// How big each browser node draws its page. See [`Live::page_zoom_of`].
    page_zooms: HashMap<NodeId, f32>,
    /// One whole Agent-Chat behind each chat node.
    ///
    /// **A chat of its own rather than a second view of the pane's**, which is `Kind::Chat`'s own reason:
    /// the ticket asks for a node that drives the nodes it is wired to, and two views of one conversation
    /// are one agent that cannot say which node it is. Each holds its own conversation, its own client and
    /// its own turn, and they share the store on disk — so the history list is every conversation in this
    /// window and each node is on one of them.
    ///
    /// Here rather than in `services::plugin_ui`, because a node is not a plugin surface: nothing in the
    /// manifests contributed it, `plugins.chrome` is still what decides whether it draws depth, and a
    /// window with the Agent-Chat plugin switched off still has a canvas.
    chats: HashMap<NodeId, crate::services::agent_chat::AgentChat>,
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
            .field("chats", &self.chats.len())
            .finish()
    }
}

impl Live {
    // ------------------------------------------------------------------------------- chats

    pub fn chat(&self, node: NodeId) -> Option<&crate::services::agent_chat::AgentChat> {
        self.chats.get(&node)
    }

    pub fn chat_mut(
        &mut self,
        node: NodeId,
    ) -> Option<&mut crate::services::agent_chat::AgentChat> {
        self.chats.get_mut(&node)
    }

    /// Put an opened chat behind a node. Whatever was there is stopped first, which is what closing one is.
    pub fn put_a_chat(&mut self, node: NodeId, chat: crate::services::agent_chat::AgentChat) {
        self.stop_a_chat(node);
        self.chats.insert(node, chat);
    }

    /// Which nodes have a chat behind them, in id order, for the frame that catches them all up.
    pub fn chat_nodes(&self) -> Vec<NodeId> {
        let mut found: Vec<NodeId> = self.chats.keys().copied().collect();
        found.sort_unstable();
        found
    }

    /// Stop and forget one node's chat, which writes whatever it was holding and ends any turn.
    fn stop_a_chat(&mut self, node: NodeId) {
        if let Some(mut chat) = self.chats.remove(&node) {
            crate::services::plugin_ui::UiProvider::close(&mut chat);
        }
    }

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
    ///
    /// **A source is read once and what it said goes down every wire out of it.** Reading it once an
    /// edge would move the anchor on the first one and leave every later edge with nothing, so a
    /// terminal wired to two others fed only whichever happened to be first — which is the whole
    /// point of the anchor working, applied to the wrong loop. Found by the `task-1904` review.
    pub fn carry_the_pipes(
        &mut self,
        now: f64,
        edges: &[(NodeId, NodeId)],
    ) -> Vec<(NodeId, String)> {
        if edges.is_empty() || now - self.read_at < f64::from(PIPE_INTERVAL) {
            return Vec::new();
        }
        self.read_at = now;
        let mut sent = Vec::new();
        let mut sources: Vec<NodeId> = edges.iter().map(|(from, _)| *from).collect();
        sources.sort_unstable();
        sources.dedup();
        for from in sources {
            let targets: Vec<NodeId> = edges
                .iter()
                .filter(|(source, _)| *source == from)
                .map(|(_, to)| *to)
                .filter(|to| self.terminals.contains_key(to))
                .collect();
            if targets.is_empty() {
                continue;
            }
            let Some(source) = self.terminals.get(&from) else { continue };
            let tail = pipe::lines_of(&source.written_text(Some(pipe::TAIL)));
            let tap = self.taps.entry(from).or_insert_with(|| Tap::following(&tail));
            let lines = tap.take(&tail);
            for line in lines {
                for to in &targets {
                    if let Some(target) = self.taps.get_mut(to) {
                        target.sent(&line);
                    }
                    if let Some(session) = self.terminals.get(to) {
                        session.send(format!("{line}\r").into_bytes());
                    }
                    sent.push((*to, line.clone()));
                }
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
        self.typed.insert(node);
    }

    /// Whether anything has been typed into this node since the window opened.
    ///
    /// **What it is for is deciding whether a shell is a shell somebody is really at.** A restored terminal
    /// node starts a shell, so the first reading of what it is running is a prompt — and that would clear the
    /// program `space.conf` held before anything could offer it. See the guard in
    /// `UnluminousApp::note_what_a_node_is_running`. The protection has to end, or a program somebody
    /// deliberately quit would be offered on every restart from then on; once something has been typed, a
    /// prompt is a prompt somebody really is at. `task-1907`.
    ///
    /// **Not a `taps` entry, which every started node has.** `follow_from_here` inserts one the moment a
    /// terminal starts, so asking `taps` answered yes for every restored node and the protection never applied
    /// at all — measured on the released build, `running = sleep` survived the window closing and was cleared
    /// a second after it reopened. What is counted is lines a person or an agent really sent, which is
    /// [`Self::typed_into`]'s own event.
    pub fn has_been_used(&self, node: NodeId) -> bool {
        self.typed.contains(&node)
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

    /// How big a browser node draws its page.
    ///
    /// **Kept here rather than on the node's state**, because it is the engine's number rather than
    /// Unluminous's drawing: `wry::WebView::zoom` is what applies it, and a window has one native view, so
    /// it has to be applied again whenever the view is pointed at this node's tab.
    pub fn page_zoom_of(&self, node: NodeId) -> f32 {
        self.page_zooms.get(&node).copied().unwrap_or(1.0)
    }

    /// Remember how big this node draws its page.
    pub fn set_page_zoom(&mut self, node: NodeId, zoom: f32) {
        self.page_zooms.insert(node, zoom.max(0.1));
    }

    /// How far a folder node's rows are scrolled.
    pub fn scroll_of(&self, node: NodeId) -> f32 {
        self.scrolls.get(&node).copied().unwrap_or(0.0)
    }

    /// Scroll a folder node's rows to `offset`, never above the top.
    pub fn scroll_to(&mut self, node: NodeId, offset: f32) {
        self.scrolls.insert(node, offset.max(0.0));
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
        self.typed.remove(&node);
        self.browsers.remove(&node);
        self.trees.remove(&node);
        self.selected.remove(&node);
        self.scrolls.remove(&node);
        self.page_zooms.remove(&node);
        self.stop_a_chat(node);
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
            .chain(self.chats.keys())
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

    /// A detached session, which is what the terminal's own tests use: the same emulator over fixed
    /// bytes, with no shell behind it, so what it holds is the same on every run.
    fn a_terminal(live: &mut Live, node: NodeId, wrote: &str) {
        let size = unluminous_terminal::session::Size::new(24, 80);
        let mut session = unluminous_terminal::Session::detached(size);
        session.feed(wrote.as_bytes());
        live.start_terminal(node, session);
    }

    #[test]
    fn what_one_terminal_wrote_goes_down_every_wire_out_of_it() {
        // The `task-1904` review's first finding: reading the source once **an edge** moved its
        // anchor on the first one, so a terminal wired to two others fed whichever happened to be
        // first and the other got nothing at all.
        let mut live = Live::default();
        a_terminal(&mut live, 1, "");
        a_terminal(&mut live, 2, "");
        a_terminal(&mut live, 3, "");
        live.follow_from_here(1);

        if let Some(session) = live.terminal_mut(1) {
            session.feed(
                b"cargo test
",
            );
        }
        let sent = live.carry_the_pipes(10.0, &[(1, 2), (1, 3)]);
        let lines: Vec<(NodeId, String)> =
            sent.into_iter().filter(|(_, line)| line == "cargo test").collect();
        let mut reached: Vec<NodeId> = lines.iter().map(|(to, _)| *to).collect();
        reached.sort_unstable();
        assert_eq!(reached, vec![2, 3], "both wires carried the line");

        // And it is sent exactly once down each, which is what the anchor is for.
        let again = live.carry_the_pipes(20.0, &[(1, 2), (1, 3)]);
        assert!(again.is_empty(), "nothing new was written, so nothing was sent");
    }

    #[test]
    fn forgetting_a_node_takes_everything_behind_it() {
        let mut live = Live::default();
        live.put_a_tree(7, FileTree::new(std::env::temp_dir()));
        live.typed_into(7, "something");
        live.scroll_to(7, 240.0);
        assert!(live.has_a_tree(7));
        assert_eq!(live.nodes(), vec![7]);
        live.forget(7);
        assert!(!live.has_a_tree(7));
        assert!(live.nodes().is_empty());
        assert_eq!(live.scroll_of(7), 0.0, "and where it was scrolled to went with it");
    }

    /// A folder node's scroll starts at the top and is never taken above it.
    #[test]
    fn a_folder_nodes_scroll_is_kept_and_never_goes_above_the_top() {
        let mut live = Live::default();
        assert_eq!(live.scroll_of(7), 0.0, "a node nobody has scrolled is at the top");
        live.scroll_to(7, 180.0);
        assert_eq!(live.scroll_of(7), 180.0);
        live.scroll_to(7, -40.0);
        assert_eq!(live.scroll_of(7), 0.0, "a wheel turned up at the top stops at the top");
    }
}
