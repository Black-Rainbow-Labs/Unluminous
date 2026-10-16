//! Where something the pointer is carrying would land, and where it did.
//!
//! A tab, a file or a whole panel is picked up in one place and let go in another as often as not,
//! and the list it was picked up in cannot know where that turned out to be — it may be a node it has
//! never heard of. So each list reports **that** something is in the air and where the pointer is,
//! and every question about where it landed is settled here, once everything that can take a drop has
//! been drawn.
//!
//! See [`crate::app::Drag`] for the three fields that report it and why there are three.

use egui::{Pos2, Rect};

use crate::components::file_tabs::{self};

use crate::app::{Focus, UnluminousApp};

/// Where pane `from`, let go at `at`, would go: how many panes along the row, counting the panes as they are
/// now, and the half of the pane it is over that would be lit. `None` over no pane, over its own pane, or
/// where it would land exactly where it already is.
pub fn where_a_pane_would_land(
    pane_rects: &[Rect],
    from: usize,
    at: Pos2,
) -> Option<(usize, Rect)> {
    let over = pane_rects.iter().position(|rect| rect.contains(at))?;
    if over == from {
        return None;
    }
    let rect = pane_rects[over];
    let left = at.x < rect.center().x;
    let before = if left { over } else { over + 1 };
    if before == from || before == from + 1 {
        return None;
    }
    let half = match left {
        true => Rect::from_min_max(rect.min, Pos2::new(rect.center().x, rect.bottom())),
        false => Rect::from_min_max(Pos2::new(rect.center().x, rect.top()), rect.max),
    };
    Some((before, half))
}

impl UnluminousApp {
    /// Work out where the tab being carried would land, draw the mark that says so, and move it when
    /// it is let go.
    ///
    /// Called once the whole row of panes has been drawn, which is the earliest moment anything
    /// knows where every strip is. A tab may be dropped **anywhere in a pane** rather than on its
    /// strip alone, which is what the reference editor does and is what a person dragging a file into the pane
    /// beside them is aiming at; where along the strip it goes is read from the pointer's x.
    ///
    /// Dropped outside every pane — over the explorer, the terminal, the status bar — nothing
    /// happens and no mark is drawn, so a drag can be thought better of.
    pub(crate) fn settle_the_tab_drag(&mut self, ui: &mut egui::Ui, pane_rects: &[Rect]) {
        let Some((file, at, dropped)) = self.tab_drag.in_the_air() else {
            return;
        };
        // **A File Editor node is asked about first**, because a node is drawn *over* the panes: a canvas
        // docked to the bottom is inside the body the panes were laid out in, so a point inside a node is
        // very often inside a pane as well, and the node is what the pointer is actually over.
        // `task-1905`.
        if let Some((node, rect, strip)) =
            self.node_tab_strips.iter().find(|(_, rect, _)| rect.contains(at)).cloned()
        {
            let position = strip.position_at(at.x);
            if dropped {
                self.files.drag_tab_to_node(file, node, position);
                self.remember_a_nodes_tabs(node);
                self.focus = Focus::Realm;
                self.realm.realm.choose(Some(node));
                return;
            }
            // **A node showing one file draws no strip**, so there is nowhere to put an insertion mark:
            // `Strip::default` is `Rect::NOTHING` and its edges are infinities. The node itself is lit
            // instead, which says the same thing — this is where the tab would land.
            match strip.area.is_finite() {
                true => file_tabs::insertion_mark(ui.painter(), &strip, position),
                false => crate::components::realm::landing_mark(ui.painter(), rect),
            }
            return;
        }
        // **The empty canvas is a fourth place a tab can be dropped.** `task-1914`: *"I should be able to
        // drag tabs onto the canvas and it break out into a new node."* Asked after the nodes and before
        // the panes, for the reason the nodes are: the canvas is a panel laid out beside the editing area,
        // so a point on it is never inside a pane, but a node on it is drawn over the canvas and is what
        // the pointer is really over.
        if self.canvas_would_take_a_drop_at(at) {
            if dropped {
                self.break_a_tab_out_onto_the_canvas(file, at);
                return;
            }
            crate::components::realm::landing_mark(
                ui.painter(),
                self.where_a_fresh_node_would_be(at),
            );
            return;
        }
        let Some(pane) = pane_rects.iter().position(|rect| rect.contains(at)) else {
            return;
        };
        let Some(strip) = self.tab_strips.get(pane) else {
            return;
        };
        let position = strip.position_at(at.x);
        if dropped {
            self.files.drag_tab(file, pane, position);
            self.focus = Focus::Editor;
            return;
        }
        // The mark goes over the strip it is about, so it is drawn into the window's own painter
        // rather than the pane's: the pane was drawn already and a mark added to it would be under
        // the strip it is meant to be over.
        file_tabs::insertion_mark(ui.painter(), strip, position);
    }

    /// Where a whole pane carried by the empty part of its tab strip would land, and where it did.
    ///
    /// `task-2280`: *"I'm unable to drag the file pane around. I should be able to click in the empty space to
    /// the right of a tab and drag and place it."* A pane lands beside another pane: let go over the left half
    /// of one and it goes before it, over the right half and it goes after it. The half it would land beside is
    /// lit while it is carried. Let go over its own pane, or anywhere outside the editing area, and nothing
    /// moves, which is the promise every other drag in the window makes.
    pub(crate) fn settle_the_pane_drag(&mut self, ui: &mut egui::Ui, pane_rects: &[Rect]) {
        let Some((from, at, dropped)) = self.pane_drag.in_the_air() else {
            return;
        };
        let Some((before, half)) = where_a_pane_would_land(pane_rects, from, at) else {
            return;
        };
        if dropped {
            if self.files.move_pane(from, before) {
                self.focus = Focus::Editor;
                let now = if before > from { before - 1 } else { before };
                self.message = Some(format!("Pane {from} is now pane {now}"));
            }
            return;
        }
        crate::components::realm::landing_mark(ui.painter(), half);
    }

    /// Whether a thing let go at `at` would land on the empty canvas.
    ///
    /// Asked after the File Editor nodes and before the panes, for the reason the nodes are asked first: the
    /// canvas is a panel laid out beside the editing area, so a point on it is never inside a pane, and a
    /// node on it is drawn over the canvas and is what the pointer is really over.
    fn canvas_would_take_a_drop_at(&self, at: Pos2) -> bool {
        self.the_canvas_is_drawn() && self.realm.body.contains(at)
    }

    /// The rectangle a node made by a drop at `at` would cover on the screen, cut to the canvas.
    fn where_a_fresh_node_would_be(&self, at: Pos2) -> Rect {
        let size = crate::services::realm::Kind::Editor.opens_at();
        let world = self.where_a_node_dropped_at_would_go(at);
        self.realm
            .realm
            .camera
            .rect_to_screen(self.realm.body.min, Rect::from_min_size(world, size))
            .intersect(self.realm.body)
    }

    /// A tab let go on the empty canvas: a File Editor node of its own, with that tab in it.
    ///
    /// `task-1914`: *"I should be able to drag tabs onto the canvas and it break out into a new node."*
    ///
    /// **The one place that happens**, so the pointer and a test reach it the same way — which is
    /// `run_cli`'s rule about a command and a menu entry. Answers the node it made.
    pub fn break_a_tab_out_onto_the_canvas(
        &mut self,
        file: usize,
        at: Pos2,
    ) -> crate::services::realm::NodeId {
        let world = self.where_a_node_dropped_at_would_go(at);
        let node = self.add_a_realm_node(crate::services::realm::Kind::Editor, world);
        self.files.drag_tab_to_node(file, node, 0);
        self.remember_a_nodes_tabs(node);
        self.focus = Focus::Realm;
        self.realm.realm.choose(Some(node));
        node
    }

    /// A file let go on the canvas: onto a File Editor node as a new tab, or onto the empty canvas as a
    /// node of its own.
    ///
    /// `task-1914`: *"I should be able to drag a file onto the canvas to have it open into a new file editor
    /// node. Or if I drag to existing file node, it should open the file in a new tab."*
    ///
    /// Answers which node took it, or the reason it could not be opened. The same function the pointer
    /// reaches through [`Self::settle_the_file_drag`].
    pub fn drop_a_file_onto_the_canvas(
        &mut self,
        path: &std::path::Path,
        at: Pos2,
    ) -> Result<crate::services::realm::NodeId, String> {
        let onto = self
            .node_tab_strips
            .iter()
            .find(|(_, rect, _)| rect.contains(at))
            .map(|(node, _, _)| *node);
        let node = match onto {
            Some(node) => node,
            None => {
                let world = self.where_a_node_dropped_at_would_go(at);
                // **A picture, a sound, a video or a Markdown file makes the node that shows it**, which is
                // `task-2202`'s drop: anything else opens in a File Editor node, as it always did.
                if let Some(kind) = crate::app::realm_nodes::kind_for_a_file(path) {
                    return self.add_a_file_node(kind, path, world);
                }
                self.add_a_realm_node(crate::services::realm::Kind::Editor, world)
            }
        };
        self.open_in_a_realm_node(node, path)?;
        Ok(node)
    }

    /// Where a node dropped at `at` on the screen goes, in world points.
    ///
    /// **Under the pointer rather than starting at it**, which is what dropping something means: a node
    /// whose top left corner was the pointer would appear down and to the right of where it was let go.
    /// The pointer lands a header's height into the node and a little in from its left edge, so the thing
    /// under it after the drop is the node's own title bar — the part it is dragged by.
    fn where_a_node_dropped_at_would_go(&self, at: Pos2) -> Pos2 {
        let camera = self.realm.realm.camera;
        let world = camera.to_world(self.realm.body.min, at);
        Pos2::new(world.x - 24.0, world.y - 12.0)
    }

    /// Where a file being carried would land, once every list and every node has said where it is.
    ///
    /// `task-1914`: *"I should be able to drag a file onto the canvas to have it open into a new file
    /// editor node. Or if I drag to existing file node, it should open the file in a new tab."*
    ///
    /// The same shape as [`Self::settle_the_tab_drag`], and settled beside it for the same reason: the
    /// explorer is drawn long before the canvas, and a list cannot know about a node it never heard of.
    /// A drop anywhere but the canvas is left alone — the explorer's own drop on a folder is a **move on
    /// disk** and it has already claimed those, and a drag let go over nothing is a drag that was thought
    /// better of, which is the promise the row drag and the tab drag both already make.
    pub(crate) fn settle_the_file_drag(&mut self, ui: &mut egui::Ui) {
        let Some((path, at, dropped)) = self.file_drag.take() else {
            return;
        };
        if !self.canvas_would_take_a_drop_at(at) {
            return;
        }
        // A folder is not a file, and a File Editor node has nothing to do with one. Dropping one on the
        // canvas could reasonably make a Folder View node, and that is a different feature with a
        // different answer — it is left out rather than guessed at.
        if path.is_dir() {
            return;
        }
        if dropped {
            if let Err(problem) = self.drop_a_file_onto_the_canvas(&path, at) {
                self.message = Some(problem);
            }
            return;
        }
        // Where it would land: a File Editor node it is over, or the node it would make on the canvas.
        let over = self
            .node_tab_strips
            .iter()
            .find(|(_, rect, _)| rect.contains(at))
            .map(|(_, rect, _)| *rect);
        let mark = over.unwrap_or_else(|| self.where_a_fresh_node_would_be(at));
        crate::components::realm::landing_mark(ui.painter(), mark);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `task-2280`: a pane let go over the left half of another goes before it, and over the right half after
    /// it; over itself, or beside itself, it does not move.
    #[test]
    fn a_pane_lands_before_or_after_the_pane_it_is_let_go_over() {
        let rects: Vec<Rect> = (0..3)
            .map(|pane| {
                let left = pane as f32 * 300.0;
                Rect::from_min_max(Pos2::new(left, 0.0), Pos2::new(left + 300.0, 500.0))
            })
            .collect();
        let at = |x: f32| Pos2::new(x, 200.0);
        assert_eq!(where_a_pane_would_land(&rects, 0, at(850.0)).map(|(at, _)| at), Some(3));
        assert_eq!(where_a_pane_would_land(&rects, 0, at(650.0)).map(|(at, _)| at), Some(2));
        assert_eq!(where_a_pane_would_land(&rects, 2, at(50.0)).map(|(at, _)| at), Some(0));
        assert_eq!(where_a_pane_would_land(&rects, 0, at(100.0)), None, "over itself");
        assert_eq!(where_a_pane_would_land(&rects, 0, at(350.0)), None, "where it already is");
        assert_eq!(where_a_pane_would_land(&rects, 0, at(5000.0)), None, "outside every pane");
        let (_, half) = where_a_pane_would_land(&rects, 0, at(850.0)).expect("a landing");
        assert_eq!(half, Rect::from_min_max(Pos2::new(750.0, 0.0), Pos2::new(900.0, 500.0)));
    }
}
