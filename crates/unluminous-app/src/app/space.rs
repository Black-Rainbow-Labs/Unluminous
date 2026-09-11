//! The window's side of the Base of Infinite Space — `task-1904`.
//!
//! `services::space` is the model and `components::space` is the drawing. This is the third piece:
//! the pane, the gestures, the four node bodies, and the one place a canvas command turns into a
//! change.
//!
//! ## A node's body is drawn by the window, not by the component
//!
//! Three of the four node kinds need something only the window has — the terminal's renderer, the one
//! native browser child, a `Document` in `OpenFiles`. So the loop is here and the component is called
//! twice for each node: `parts_of` says where the body goes, the window draws it, and `frame` draws
//! the header, the ports and the grips **after** it, so those take the points they cover. That is
//! `components::resize_edges`'s ordering rule, applied inside a node.
//!
//! ## The keyboard, and what `active()` means while a node has it
//!
//! A File Editor node is an ordinary tab whose [`crate::app::files::Home`] is that node, so
//! `files.active()` answers with it while it has the keyboard and four hundred lines of `show_editor`
//! work on it unchanged. The pane loop already borrows the focus this way; this borrows it for a node.

use egui::{Pos2, Rect, Vec2};

use crate::app::actions::SpaceAction;
use crate::app::dock;
use crate::app::files::{Home, OpenFile};
use crate::app::{Focus, UnluminousApp};
use serde_json::{json, Value};

use unluminous_cli::protocol::{code, Request};

use crate::app::cli::{done, lines, no, ok, unknown, Outcome};
use crate::components::space::{self as space_view, add_modal};
use crate::services::space::{live::Live, store, Kind, Node, NodeId, Pipe, Space, State};

/// How long to wait before writing `space.conf` again after a write failed, in seconds.
const RETRY_A_FAILED_WRITE: f64 = 2.0;

/// What is being dragged on the canvas right now.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Gesture {
    #[default]
    None,
    /// The canvas itself is being dragged.
    Panning,
    /// A wire is being pulled out of a node's output port. Where its free end is, in world points.
    Wiring { from: NodeId, at: Pos2 },
}

/// Which node, wire or view a right click was on, so the menu's entries can be parameterless.
///
/// `actions::tab_menu`'s rule: a right click **shows** what it was over before the menu is drawn, so
/// every entry is about the thing in hand and the `View` menu, the keyboard and `unluminous-cli action
/// run` can all ask for the same action.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct InHand {
    pub wire: Option<u64>,
    pub view: Option<u64>,
}

/// Everything the window holds for the canvas.
#[derive(Debug)]
pub struct SpaceState {
    pub space: Space,
    pub live: Live,
    pub visible: bool,
    pub gesture: Gesture,
    /// The add modal, while it is open.
    pub adding: Option<add_modal::State>,
    /// The space manager, while it is open — `task-1906`.
    pub managing: Option<crate::components::space::manager::State>,
    /// Where a right click menu is open, and which menu it is.
    pub menu: Option<(Pos2, Menu)>,
    pub in_hand: InHand,
    /// Which view's nodes have been brought to life.
    ///
    /// **Derived rather than fired from each of the places a view changes**, which is
    /// `follow_the_open_file`'s own rule: a list of the places that have to remember to start a
    /// view's terminals is a list whose next entry is the one that forgets. The `task-1904` review
    /// found exactly that — the strip of views changed the model and nothing else, so a view chosen
    /// from it had no sessions, no pages and no files behind its nodes.
    pub brought_to_life: Option<crate::services::space::ViewId>,
    /// When a write of `space.conf` last failed, so a broken disk is not written to sixty times a
    /// second while the canvas stays marked as needing writing.
    pub write_failed_at: Option<f64>,
    /// The rectangle the canvas body had last frame.
    ///
    /// What a command with no place of its own puts a node at — `space add` with no `--x` — and what
    /// the keyboard's zoom is centred on. Read back from the drawing rather than worked out twice.
    pub body: Rect,
}

impl Default for SpaceState {
    /// Written by hand because `egui::Rect` has none: a rectangle nothing has been drawn into yet is
    /// `Rect::ZERO`, which every reader of one in this file already treats as "not there".
    fn default() -> Self {
        Self {
            space: Space::new(),
            live: Live::default(),
            visible: false,
            gesture: Gesture::None,
            adding: None,
            managing: None,
            menu: None,
            in_hand: InHand::default(),
            brought_to_life: None,
            write_failed_at: None,
            body: Rect::ZERO,
        }
    }
}

/// Which of the canvas's three right click menus is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    Node,
    Wire,
    View,
}

impl SpaceState {
    /// The node the commands and the keyboard are about.
    pub fn chosen(&self) -> Option<NodeId> {
        self.space.chosen()
    }
}

impl UnluminousApp {
    // ------------------------------------------------------------------------------- drawing

    /// Draw the whole canvas: its header, its strip of views, its ground, its wires and its nodes.
    pub(crate) fn show_the_space(&mut self, ui: &mut egui::Ui) {
        let rect = self.panel_rects.of(dock::Panel::Space);
        if rect.width() < 2.0 || rect.height() < 2.0 {
            return;
        }
        let header = Rect::from_min_size(
            rect.min,
            Vec2::new(rect.width(), crate::components::agent_tasks::PANE_HEADER),
        );
        let outcome = {
            let mut header_ui = ui.new_child(egui::UiBuilder::new().max_rect(header));
            let count = self.space.space.current().nodes.len();
            crate::components::agent_tasks::pane_header(
                &mut header_ui,
                header,
                "Base of Infinite Space",
                Some(&count.to_string()),
                dock::Panel::Space,
                self.settings.opacity,
            )
        };
        self.note_a_panel_grab(dock::Panel::Space, outcome.grab);
        if outcome.closed {
            self.show_a_panel(dock::Panel::Space, false);
            return;
        }

        let bar = Rect::from_min_size(
            Pos2::new(rect.left(), header.bottom()),
            Vec2::new(rect.width(), space_view::VIEW_BAR),
        );
        let body = Rect::from_min_max(Pos2::new(rect.left(), bar.bottom()), rect.max);
        self.space.body = body;

        // **The same switch a plugin's decoration has.** `plugins.chrome` is what somebody turns off
        // when the rasteriser costs more than the depth is worth, and a canvas that ignored it would
        // be the one surface in the window that did. Off, it records nothing and costs nothing, and
        // every part of the canvas draws its flat form - each asks `chrome.is_recording()`.
        let chrome = match self.settings.plugin_chrome {
            true => crate::services::vello_canvas::Chrome::recording(),
            false => crate::services::vello_canvas::Chrome::off(),
        };
        let look = space_view::Look {
            opacity: self.settings.opacity,
            page: crate::theme::color::editor(),
            card: crate::theme::color::code_panel(),
            header: crate::theme::color::explorer(),
            chrome: &chrome,
        };
        let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
        body_ui.set_clip_rect(body);
        space_view::ground(&body_ui, body, &self.space.space.current().camera, look);
        // The slot the decoration is rasterised into, reserved after the ground and before anything
        // else — the rule `show_the_plugin_panes` records: a ground painted after it covers the very
        // thing it is for.
        let slot = body_ui.painter().add(egui::Shape::Noop);

        let bar_outcome = {
            let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar));
            let views = self.space.space.views().to_vec();
            let current = self.space.space.current_id();
            let zoom = self.space.space.current().camera.zoom;
            space_view::view_bar(&mut bar_ui, bar, &views, current, zoom, look)
        };
        self.act_on_the_view_bar(bar_outcome);

        self.take_the_canvas_input(&mut body_ui, body);
        let wire_outcome = {
            let view = self.space.space.current().clone();
            let camera = view.camera;
            space_view::wires(&mut body_ui, body, &view, &camera, look)
        };
        if let Some((at, edge)) = wire_outcome.menu {
            self.space.in_hand.wire = Some(edge);
            self.space.menu = Some((at, Menu::Wire));
        }
        self.show_the_space_nodes(ui, &mut body_ui, body, look);
        self.settle_the_wire_in_the_air(&body_ui, body);
        if self.space.space.current().nodes.is_empty() {
            space_view::nothing_here_yet(&body_ui, body);
        }
        self.paint_the_chrome(ui, slot, egui::Id::new("space-canvas"), body, &chrome);
        self.zoom_over_a_panel(ui, dock::Panel::Space, body);
    }

    /// Pan, zoom and the right click that opens the add modal.
    ///
    /// Read before the nodes are drawn, over the whole body, so a node drawn afterwards takes the
    /// points it covers and only the empty canvas is left to this.
    fn take_the_canvas_input(&mut self, ui: &mut egui::Ui, body: Rect) {
        let response = ui.interact(
            body,
            ui.id().with("space-canvas"),
            egui::Sense::click_and_drag(),
        );
        if response.dragged() {
            let by = response.drag_delta();
            self.space.space.current_mut().camera.pan_by(by);
            self.space.gesture = Gesture::Panning;
        } else if self.space.gesture == Gesture::Panning {
            self.space.gesture = Gesture::None;
        }
        if response.clicked() {
            self.space.space.choose(None);
            self.take_the_keyboard_for_the_space();
        }
        if response.secondary_clicked() {
            if let Some(at) = response.interact_pointer_pos() {
                let world = self.space.space.current().camera.to_world(body.min, at);
                self.space.adding = Some(add_modal::State { at: world, ..Default::default() });
            }
        }
        // The wheel over the empty canvas zooms, which is what Chordical does and what a canvas
        // means by a wheel. Over a node it goes to the node, which is what the node's own widgets
        // take before this is reached.
        if response.hovered() {
            let steps = ui.input(|input| input.smooth_scroll_delta.y);
            if steps.abs() > 0.5 {
                if let Some(at) = ui.ctx().pointer_latest_pos() {
                    let notches = (steps / 50.0).clamp(-3.0, 3.0);
                    let camera = &mut self.space.space.current_mut().camera;
                    let wanted = camera.zoom * 1.1_f32.powf(notches);
                    camera.zoom_to(wanted, body.min, at);
                }
            }
        }
    }

    /// Draw every node that can be seen, each into a layer of its own carrying the camera.
    fn show_the_space_nodes(
        &mut self,
        ui: &mut egui::Ui,
        body_ui: &mut egui::Ui,
        body: Rect,
        look: space_view::Look<'_>,
    ) {
        let camera = self.space.space.current().camera;
        let clip = space_view::clip_for_nodes(body, ui.ctx().content_rect(), &camera);
        let to_global = egui::emath::TSTransform::new(
            body.min.to_vec2() - camera.at.to_vec2() * camera.zoom,
            camera.zoom,
        );
        let nodes: Vec<Node> = self
            .space
            .space
            .current()
            .nodes
            .iter()
            .filter(|node| space_view::is_showing(node, body, &camera))
            .cloned()
            .collect();
        let chosen = self.space.chosen();
        let wiring = matches!(self.space.gesture, Gesture::Wiring { .. });
        // Which node the wire in the air would land on, worked out once from the model rather than
        // asked of each port - see the note in `components::space::show_the_ports`.
        let landing = match self.space.gesture {
            Gesture::Wiring { from, at } => {
                self.space.space.current().node_at(at).filter(|over| *over != from)
            }
            _ => None,
        };
        // **Which node the pointer is over is decided once, here, before any of them is drawn.**
        //
        // The loop below draws back to front, so a node asking "am I under the pointer" while it is drawn
        // answers yes for the *backmost* of a stack — and both the wheel and the modifier wheel then went to
        // the node underneath the one somebody was looking at, which the Codex Sol review of `task-1905`
        // found. One answer, worked out from the drawing order the way `View::node_at` does, and handed to
        // whichever node it names.
        //
        // It is the **chosen** node first when the pointer is inside it, because a node that has been clicked
        // is moved to the top egui layer by `move_to_top` whether or not `Space::raise` moved it in the
        // model — so the model's own order is not the whole truth about what is on top.
        let pointer = body_ui
            .input(|input| input.pointer.hover_pos().or_else(|| input.pointer.latest_pos()))
            .filter(|at| body.contains(*at));
        let under_the_pointer = pointer.and_then(|at| {
            let over = |node: &Node| camera.rect_to_screen(body.min, node.rect()).contains(at);
            nodes
                .iter()
                .find(|node| Some(node.id) == chosen && over(node))
                .or_else(|| nodes.iter().rev().find(|node| over(node)))
                .map(|node| node.id)
        });
        // **The modifier wheel goes to that node**, before `zoom_over_a_panel` at the end of
        // `show_the_space` can give it to the camera. `task-1905`.
        self.zoom_over_a_node(body_ui, under_the_pointer);
        let parent = body_ui.layer_id();
        let mut menu: Option<(Pos2, NodeId)> = None;
        for node in nodes {
            let layer = egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new(("space-node-layer", node.id)),
            );
            ui.ctx().set_sublayer(parent, layer);
            ui.ctx().set_transform_layer(layer, to_global);
            if Some(node.id) == chosen {
                // The chosen node is the one in front. A layer keeps the place it was first given, so
                // raising a node in the model is not enough on its own — this is what actually moves it.
                ui.ctx().move_to_top(layer);
            }
            let parts = space_view::parts_of(&node);
            let mut node_ui = ui.new_child(
                egui::UiBuilder::new().layer_id(layer).max_rect(node.rect()).id_salt(("space-node", node.id)),
            );
            node_ui.set_clip_rect(clip);
            let focused = Some(node.id) == chosen && matches!(self.focus, Focus::Space);
            let has_the_pointer = under_the_pointer == Some(node.id);
            self.show_a_node_body(&mut node_ui, &node, parts.body, focused, has_the_pointer);
            let framing = space_view::Framing {
                chosen: Some(node.id) == chosen,
                keyboard: focused,
                on_screen: camera.rect_to_screen(body.min, node.rect()),
                visible: body,
                wire_is_looking: wiring,
                landing,
                fallback_title: &self.name_of_a_node(&node),
            };
            let outcome = space_view::frame(&mut node_ui, &node, framing, look);
            if let Some(at) = outcome.menu {
                menu = Some((at, node.id));
            }
            self.act_on_a_node(&node, outcome);
        }
        if let Some((at, node)) = menu {
            self.space.space.choose(Some(node));
            self.space.space.raise(node);
            self.space.menu = Some((at, Menu::Node));
        }
    }

    /// The modifier wheel over a node zooms **that node**, and the canvas gets it only when no node did.
    ///
    /// `task-1905`: *"If I CMD/CTRL mouse wheel while hovering over a node, that node should zoom in/out,
    /// rather than the entire canvas."* Both gestures reached the camera before — the plain wheel through
    /// `take_the_canvas_input` and the modifier one through `zoom_over_a_panel` — and nothing reached a
    /// node.
    ///
    /// **Claimed here, before `zoom_over_a_panel(Panel::Space)` is reached**, which is the last line of
    /// `show_the_space`: `zoom_taken` is the lock, one level further in than `task-1771` put it. And
    /// `zoom_steps` is called **at most once a frame**, because calling it twice spends the same notch
    /// twice — so which node the pointer is in is decided first and it is called once for that node.
    fn zoom_over_a_node(&mut self, ui: &egui::Ui, under_the_pointer: Option<NodeId>) {
        if self.zoom_taken {
            return;
        }
        let Some(node) = under_the_pointer else {
            return;
        };
        self.zoom_taken = true;
        let steps = self.zoom_steps(ui);
        if steps != 0 {
            self.zoom_a_node(node, steps);
        }
    }

    /// Take `steps` off or on to how big one node draws what it holds.
    ///
    /// **Each kind walks the number that really decides its size**, which is `task-1771`'s rule — where a
    /// pane already has a size a person chooses, the zoom walks that setting, because one number saying how
    /// big the text is beats a setting and a multiplier that can disagree. A terminal and an editor have a
    /// point size; a folder has none of its own, so its is a multiplier; a page's size is the page's own
    /// business and `wry` has `WebView::zoom` for it.
    pub(crate) fn zoom_a_node(&mut self, node: NodeId, steps: i32) {
        let Some(found) = self.space.space.current().node(node).cloned() else { return };
        let up = steps > 0;
        match found.kind() {
            Kind::Terminal => self.step_a_node_font(node, steps),
            Kind::Editor => {
                let was = space_view::editor_font_size_of(&found, self.settings.font_size);
                let mut size = was;
                for _ in 0..steps.abs() {
                    size = crate::settings::step_font_size(size, up);
                }
                if (size - was).abs() < 0.01 {
                    return;
                }
                self.space.space.change(node, |state| {
                    if let State::Editor(editor) = state {
                        editor.font_size = size;
                    }
                });
                // The tab in this node is laid out again at the new size, which is what makes it show.
                if let Some(index) = self.files.tab_in_node(node) {
                    self.files.at_mut(index).cached.stale = true;
                }
            }
            Kind::Folder => {
                let was = match &found.state {
                    State::Folder(folder) => folder.zoom,
                    _ => 1.0,
                };
                let mut zoom = was;
                for _ in 0..steps.abs() {
                    zoom = crate::settings::step_zoom(zoom, up);
                }
                if (zoom - was).abs() < 0.001 {
                    return;
                }
                self.space.space.change(node, |state| {
                    if let State::Folder(folder) = state {
                        folder.zoom = zoom;
                    }
                });
            }
            // **A page's own zoom, because a page is not Unluminous's drawing at all.** How big it is
            // drawn is the engine's, and `wry::WebView::zoom` is what changes it.
            Kind::Browser => {
                let was = self.space.live.page_zoom_of(node);
                let mut zoom = was;
                for _ in 0..steps.abs() {
                    zoom = crate::settings::step_zoom(zoom, up);
                }
                if (zoom - was).abs() < 0.001 {
                    return;
                }
                self.space.live.set_page_zoom(node, zoom);
                if let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) {
                    if let Err(problem) = self.browser.zoom(tab, f64::from(zoom)) {
                        self.message = Some(problem);
                    }
                }
            }
        }
    }

    /// Act on what one node's frame reported.
    fn act_on_a_node(&mut self, node: &Node, outcome: space_view::NodeOutcome) {
        if outcome.chose {
            self.space.space.choose(Some(node.id));
            self.space.space.raise(node.id);
            self.take_the_keyboard_for_the_space();
        }
        if let Some(by) = outcome.moved {
            self.space.space.move_node(node.id, node.at + by);
        }
        if let Some((grip, by)) = outcome.resized {
            let rect = crate::services::space::geometry::resized(
                node.rect(),
                grip,
                by,
                node.kind().smallest(),
            );
            self.space.space.place_node(node.id, rect);
        }
        if let Some(at) = outcome.wiring {
            self.space.gesture = Gesture::Wiring { from: node.id, at };
        }
        if outcome.font_step != 0 {
            self.step_a_node_font(node.id, outcome.font_step);
        }
        if outcome.closed {
            self.close_a_space_node(node.id);
        }
    }

    /// Draw the wire that is being pulled out of a port, and let it go when the button comes up.
    fn settle_the_wire_in_the_air(&mut self, ui: &egui::Ui, body: Rect) {
        let Gesture::Wiring { from, at } = self.space.gesture else { return };
        let camera = self.space.space.current().camera;
        let Some(node) = self.space.space.current().node(from) else {
            self.space.gesture = Gesture::None;
            return;
        };
        let start = camera.to_screen(body.min, node.output_port());
        let end = camera.to_screen(body.min, at);
        let over = self.space.space.current().node_at(at).filter(|over| *over != from);
        space_view::wire_in_the_air(ui, body, start, end, over.is_some());
        // A drag that ended over nothing is a drag that was thought better of, which is the promise
        // the explorer's row drag and the tab drag already make.
        if ui.input(|input| input.pointer.any_released()) {
            self.space.gesture = Gesture::None;
            if let Some(over) = over {
                self.land_the_wire(from, over);
            }
        }
    }

    /// A wire was let go over another node.
    fn land_the_wire(&mut self, from: NodeId, to: NodeId) {
        match self.space.space.connect(from, to, Pipe::Off) {
            Ok(_) => self.message = Some("Connected.".to_owned()),
            Err(problem) => self.message = Some(problem),
        }
    }

    // ------------------------------------------------------------------------------- the bodies

    /// Draw whatever one node holds, into the rectangle under its header.
    fn show_a_node_body(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node,
        body: Rect,
        focused: bool,
        has_the_pointer: bool,
    ) {
        if body.width() < 2.0 || body.height() < 2.0 {
            return;
        }
        match node.kind() {
            Kind::Terminal => self.show_a_terminal_node(ui, node, body, focused),
            Kind::Browser => self.show_a_browser_node(ui, node, body, focused),
            Kind::Folder => self.show_a_folder_node(ui, node, body, focused, has_the_pointer),
            Kind::Editor => self.show_an_editor_node(ui, node, body, focused),
        }
    }

    /// A terminal node: `components::terminal_panel::grid`, which is what the terminal tile and the
    /// run tile already share.
    fn show_a_terminal_node(&mut self, ui: &mut egui::Ui, node: &Node, body: Rect, focused: bool) {
        let font = space_view::font_size_of(node, self.settings.terminal_font_size);
        let opacity = self.settings.opacity;
        let id = format!("space-terminal-{}", node.id);
        let outcome = {
            let (session, selecting) = self.space.live.terminal_and_selection(node.id);
            crate::components::terminal_panel::grid(
                ui,
                body,
                session,
                selecting,
                focused,
                &id,
                "This terminal has not been started.",
                &self.renderer,
                font,
                opacity,
            )
        };
        if outcome.take_focus {
            self.space.space.choose(Some(node.id));
            self.take_the_keyboard_for_the_space();
        }
        if let Some(text) = outcome.copy {
            ui.ctx().copy_text(text);
        }
    }

    /// A browser node: Unluminous's own toolbar, and the rectangle the one native child view is placed in.
    ///
    /// **The toolbar is drawn whether or not the node has a page**, which is the whole of `task-1905`'s
    /// first report. This used to return before reaching the component whenever there was no tab yet, so
    /// the node had no address bar — and the only ways to give it an address were `space browser go` and
    /// `space add --url`. An address bar on a browser node is not a control that can never apply; it is
    /// the control that makes the node usable.
    fn show_a_browser_node(&mut self, ui: &mut egui::Ui, node: &Node, body: Rect, focused: bool) {
        let tab = self.space.live.browser(node.id).cloned();
        // **One native view a window**, so only the tab it is pointed at renders. The others say so,
        // which is the sentence `browser_view::show` already says for a second rendered tab in
        // another pane.
        let showing = tab
            .as_ref()
            .is_none_or(|tab| self.browser.showing().is_none_or(|id| id == tab.id));
        // What is being typed lives on the node, so it survives the node scrolling off the canvas and
        // stopping being drawn. Taken out, handed over, and put back if it changed.
        let mut typed = match &node.state {
            State::Browser(browser) => browser.typed.clone(),
            _ => String::new(),
        };
        let mut editing = matches!(&node.state, State::Browser(browser) if browser.editing);
        let (outcome, placement) = crate::components::browser_view::show(
            ui,
            body,
            crate::components::browser_view::Toolbar {
                tab: tab.as_ref(),
                typed: &mut typed,
                editing: &mut editing,
                id: egui::Id::new(("space-browser-address", node.id)),
            },
            focused,
            showing,
        );
        if let Some(mut placement) = placement {
            // **A native child view is a real window, so its placement is in the window's own points.**
            // Everything else about a node is drawn in world points into a layer carrying the camera, and
            // the rectangle that comes back from the component is in those — so handed over as it stands,
            // the page was placed at the node's *world* position and drawn up and to the left of the node,
            // at the wrong size. `task-1905`, reported against the installed build.
            //
            // The whole rectangle is converted rather than only its corner, so the page is also the size it
            // is on the screen: a canvas at 0.5 draws a node half as wide, and a page that kept its world
            // size would hang out of it. That is what "resize/zoom/etc" asks for, and it costs one call.
            let camera = self.space.space.current().camera;
            placement.area =
                camera.rect_to_screen(self.space.body.min, placement.area).intersect(self.space.body);
            // **The camera's zoom is spent on the page's own zoom, because a native child cannot be
            // transformed.** Everything else in a node is drawn into a layer carrying the camera, so it is
            // genuinely scaled; a `WebView` has no such transform, and `set_bounds` alone would make a
            // half-size node **reflow** its page at half the width rather than draw it half as large. The
            // Codex Sol review of `task-1905` named that, and the page's own zoom is the only lever `wry`
            // offers. It is combined with whatever zoom the node was given, so the two compose.
            //
            // What it does **not** fix is the clipping: a node hanging off the canvas has its view's viewport
            // narrowed rather than cropped, so a page laid out against the viewport reflows. There is no
            // cropping a native child either, and the honest answer is the one `browser_view` already gives
            // for a second rendered tab — see the note in `show_a_browser_node`.
            let wanted = self.space.live.page_zoom_of(node.id) * camera.zoom;
            if let Some(tab) = self.space.live.browser(node.id).map(|tab| tab.id) {
                let _ = self.browser.zoom(tab, f64::from(wanted));
            }
            // A node scrolled off the canvas has nothing on the screen to place, and a rectangle with no
            // room in it would ask the view to be a pixel wide somewhere on the pane's edge.
            if placement.area.width() > 1.0 && placement.area.height() > 1.0 {
                self.browser_placements.push(placement);
            }
        }
        let changed = match &node.state {
            State::Browser(browser) => browser.typed != typed || browser.editing != editing,
            _ => false,
        };
        if changed {
            self.space.space.change(node.id, |state| {
                if let State::Browser(browser) = state {
                    browser.typed = typed;
                    browser.editing = editing;
                }
            });
        }
        if let Some(command) = outcome.command {
            match (&tab, command) {
                // **An address typed at a node that has no page yet.** There is no tab to send a command
                // to, so the node is pointed at it directly — the same function `space browser go` calls.
                (None, crate::services::browser::BrowserCommand::Go(address)) => {
                    if let Err(problem) = self.send_a_space_browser_to(node.id, address.trim()) {
                        self.message = Some(problem);
                    }
                }
                (Some(tab), command) => self.run_browser_command(tab.id, command),
                (None, _) => {}
            }
        }
        if outcome.took_focus {
            self.space.space.choose(Some(node.id));
            self.take_the_keyboard_for_the_space();
        }
    }

    /// A folder node: `components::explorer`, with its panel furniture left off.
    ///
    /// **The window reads the wheel and the explorer is told where to scroll.** A `ScrollArea` inside a
    /// node can never take the wheel itself — see `Live::scrolls` for the reason, which is that a node's
    /// layer registers no `AreaState` and so `Context::rect_contains_pointer` is false everywhere inside
    /// one. `task-1905` reports that as *"I can't scroll the node"*. What is used instead is the
    /// `View::scroll_to` and `ExplorerOutcome::scroll` pair the panel's own zoom already drives.
    fn show_a_folder_node(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node,
        body: Rect,
        focused: bool,
        has_the_pointer: bool,
    ) {
        self.make_sure_a_node_has_a_tree(node);
        let selected = self.space.live.tree_selection(node.id).map(std::path::Path::to_path_buf);
        let showing = self.files.active().path().map(std::path::Path::to_path_buf);
        let opacity = self.settings.opacity;
        let scroll = self.wheel_over_a_folder_node(ui, node, has_the_pointer);
        let node_zoom = space_view::folder_zoom_of(node);
        // **The icons and the git colours, before the borrow.** `task-1904` asked for a node to have the
        // same style and functionality as the panel, and this was the one place it did not: the closure was
        // a placeholder answering `default()`, so a `.rs` file had no Rust icon and a modified file no
        // colour. `task-1906` is the report. See `decorations_for_a_folder_node` for why it is a map built
        // first rather than a question asked per row.
        let decorations = self.decorations_for_a_folder_node(ui.ctx(), node.id);
        let outcome = {
            let decorate = |path: &std::path::Path| -> crate::components::explorer::Decoration {
                decorations.get(path).cloned().unwrap_or_default()
            };
            let Some(tree) = self.space.live.tree_mut(node.id) else { return };
            let mut filter = match &node.state {
                State::Folder(folder) => folder.filter.clone(),
                _ => String::new(),
            };
            let view = crate::components::explorer::View {
                current: showing.as_deref(),
                selected: selected.as_deref(),
                keyboard: focused,
                unsaved: false,
                reveal: false,
                reveal_selected: false,
                opacity,
                zoom: node_zoom,
                scroll_to: scroll,
                host: crate::components::explorer::Host::Node,
            };
            let outcome = crate::components::explorer::show(ui, body, tree, &mut filter, view, &decorate);
            (outcome, filter)
        };
        let (outcome, filter) = outcome;
        // Read back where the list really ended up, which is what the next wheel is measured from — the
        // same round trip `keep_the_place_through_a_panels_zoom` makes for the panel.
        self.space.live.scroll_to(node.id, outcome.scroll);
        self.act_on_a_folder_node(node, outcome, filter);
    }

    /// How far this folder node's rows should be scrolled, given the wheel this frame.
    ///
    /// Answers `None` on a frame nobody turned the wheel, which is what leaves the list where it was:
    /// `View::scroll_to` is applied when it is `Some` and ignored otherwise.
    ///
    /// Two rules, and each is a thing that would otherwise be wrong. **The delta is in screen points and
    /// the offset is in the node's own**, so it is divided by the camera's zoom — a canvas at 0.5 would
    /// otherwise scroll twice as far as the pointer moved. And **a wheel this takes is taken out of the
    /// frame**, which is what `egui::ScrollArea` itself does when it takes one: without it the canvas
    /// would pan or zoom at the same time as the node scrolled, which is one gesture doing two things.
    /// `has_the_pointer` is whether this is the node the pointer is over, worked out **once** before any
    /// node was drawn — see `show_the_space_nodes`. Each node asking for itself gave the wheel to the
    /// backmost of a stack, and cleared the delta so the one on top got nothing.
    fn wheel_over_a_folder_node(
        &mut self,
        ui: &egui::Ui,
        node: &Node,
        has_the_pointer: bool,
    ) -> Option<f32> {
        if !has_the_pointer {
            return None;
        }
        let camera = self.space.space.current().camera;
        let delta = ui.input(|input| input.smooth_scroll_delta.y);
        if delta.abs() < 0.5 {
            return None;
        }
        ui.ctx().input_mut(|input| input.smooth_scroll_delta.y = 0.0);
        let was = self.space.live.scroll_of(node.id);
        Some((was - delta / camera.zoom.max(0.01)).max(0.0))
    }

    /// What a folder node's rows asked for.
    fn act_on_a_folder_node(
        &mut self,
        node: &Node,
        outcome: crate::components::explorer::ExplorerOutcome,
        filter: String,
    ) {
        let changed = match &node.state {
            State::Folder(folder) => folder.filter != filter,
            _ => false,
        };
        if changed {
            self.space.space.change(node.id, |state| {
                if let State::Folder(folder) = state {
                    folder.filter = filter;
                }
            });
        }
        if let Some(path) = outcome.toggle {
            if let Some(tree) = self.space.live.tree_mut(node.id) {
                tree.toggle(&path);
            }
            self.remember_a_folder_nodes_open_folders(node.id);
        }
        if let Some(path) = outcome.select.clone() {
            self.space.live.select_in_tree(node.id, Some(path));
        }
        if outcome.focus {
            self.space.space.choose(Some(node.id));
            self.take_the_keyboard_for_the_space();
        }
        // **A double click opens a wired File Editor node; a single click opens into the editing area.**
        // `task-1905`: *"If I double click a file, it should open a file view node and connect it, if one
        // isn't already open, or open a new tab in the connected file view node."*
        //
        // The split is the panel's own: a single click there is a way of looking through a folder, so a
        // preview belongs in the editing area, and a double click is what says "I mean this one". That is
        // why the report asks for a double click rather than a click.
        if let Some(path) = outcome.open_permanently {
            self.open_from_a_folder_node(node.id, &path);
        } else if let Some(path) = outcome.open {
            let _ = self.open_path_permanently(&path);
        }
    }

    /// Open a file a folder node was double clicked in, in a File Editor node wired to it.
    ///
    /// **One wired already and the file joins its tabs; none and one is made beside this node and wired.**
    /// Made *beside* means to the right of the folder node with a gap, so the wire is visible rather than
    /// crossing the node that drew it. `task-1905`.
    fn open_from_a_folder_node(&mut self, from: NodeId, path: &std::path::Path) {
        let wired = self
            .space
            .space
            .current()
            .reaches(from)
            .into_iter()
            .find(|other| {
                self.space.space.current().node(*other).is_some_and(|node| node.kind() == Kind::Editor)
            });
        let editor = match wired {
            Some(editor) => editor,
            None => {
                let Some(found) = self.space.space.current().node(from).cloned() else { return };
                let at = Pos2::new(found.rect().right() + 60.0, found.at.y);
                let made = self.add_a_space_node(Kind::Editor, at);
                if let Err(problem) = self.space.space.connect(from, made, Pipe::Off) {
                    self.message = Some(problem);
                }
                made
            }
        };
        if let Err(problem) = self.open_in_a_space_node(editor, path) {
            self.message = Some(problem);
        }
    }

    /// [`Self::open_from_a_folder_node`] by name, for a test.
    ///
    /// A double click on a row inside a node is what calls it, and a test cannot synthesise one: a node's
    /// contents are drawn into a transformed sublayer, so a press at the rectangle the accessibility tree
    /// reports lands where the widget in the layer's own coordinates is not.
    pub fn open_from_a_folder_node_for_a_test(&mut self, from: NodeId, path: &std::path::Path) {
        self.open_from_a_folder_node(from, path);
    }

    /// An editor node: the editing area's own four hundred lines, on the tab that lives in this node.
    fn show_an_editor_node(&mut self, ui: &mut egui::Ui, node: &Node, body: Rect, focused: bool) {
        // **A strip of tabs across the top, which is what `show_pane` draws for a pane.** `task-1905` asks
        // for a node *"just like our editing area, where I can see and edit files in multiple tabs"*, and
        // `components::file_tabs::show` is the same component — so dragging a tab along it, the close cross,
        // the middle click and the unsaved dot all arrive with no code here.
        let indices = self.files.tabs_in_node(node.id);
        let body = match indices.len() > 1 {
            true => {
                let strip = Rect::from_min_size(
                    body.min,
                    Vec2::new(body.width(), crate::components::file_tabs::HEIGHT),
                );
                self.show_an_editor_nodes_tabs(ui, node, strip, &indices, focused);
                Rect::from_min_max(Pos2::new(body.left(), strip.bottom()), body.max)
            }
            // **One tab draws no strip**, because a strip naming the one file a node's own header already
            // names is a row of furniture saying nothing. It appears with the second tab.
            false => body,
        };
        if body.height() < 2.0 {
            return;
        }
        let Some(index) = self.files.tab_in_node(node.id) else {
            let painter = ui.painter_at(body);
            painter.text(
                body.center(),
                egui::Align2::CENTER_CENTER,
                "Give this node a file to edit.",
                egui::FontId::proportional(12.0),
                crate::theme::color::text_faint(),
            );
            return;
        };
        // **The focus is borrowed**, exactly as the pane loop borrows it: `files.active()` answers
        // with this node's file for as long as it is being drawn, so nothing in `show_editor` had to
        // learn what a node is.
        let was = self.files.focus();
        self.files.show(index);
        self.files.focus_node(node.id);
        // **And its own font size, the same way.** The editor's font is one setting for the whole window —
        // `set_the_font_everywhere` exists because of it — so a node that walked that number would resize
        // every other tab. What it walks instead is `Editor::font_size`, applied to this tab's document
        // for the frame it is drawn in and put back by the layout being marked stale when it changes.
        // `task-1905`.
        let wanted = space_view::editor_font_size_of(node, self.settings.font_size);
        // **Applied once per change rather than every frame**, because `set_base_style` walks every byte of
        // the document and bumps its text revision — which would re-colour and re-lay out the file sixty
        // times a second.
        //
        // **And remembered on the tab, not on the node**, which is the fault the Codex Sol review of
        // `task-1905` found: keyed on the node, the second tab shown in one was never restyled because the
        // node's cache already held the wanted size, and a tab dragged back into a pane kept the node's size.
        // A document carries the base style, so the document remembers what it was given. `OpenFile::sized_at`
        // is `None` for a tab nothing has resized, which is every tab in a pane — and putting a node back to
        // the window's own size therefore restyles rather than skipping.
        let already = self.files.at(index).sized_at;
        let asked = match (wanted - self.settings.font_size).abs() > 0.01 {
            true => Some(wanted),
            // Back to the window's own size, which is a change like any other when the tab was resized.
            false => None,
        };
        if already != asked {
            let change = unluminous_core::StyleChange {
                size: Some(wanted),
                ..self.settings.as_style_change()
            };
            self.files.at_mut(index).document.set_base_style(change);
            self.files.at_mut(index).cached.stale = true;
            self.files.at_mut(index).sized_at = asked;
        }
        let took = self.show_editor(ui, body, focused);
        if took {
            self.space.space.choose(Some(node.id));
            self.take_the_keyboard_for_the_space();
        }
        if !focused {
            self.files.restore_focus(was);
        }
    }

    /// The strip of tabs across the top of a File Editor node.
    ///
    /// What it reports is turned back into an index into the open files here, which is `show_pane`'s own
    /// arrangement: the strip counts within itself and knows nothing about `OpenFiles`.
    fn show_an_editor_nodes_tabs(
        &mut self,
        ui: &mut egui::Ui,
        node: &Node,
        strip: Rect,
        indices: &[usize],
        focused: bool,
    ) {
        let icons: Vec<Option<egui::TextureHandle>> = indices
            .iter()
            .map(|index| self.files.at(*index).path().map(std::path::Path::to_path_buf))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|path| self.plugin_icon(ui.ctx(), path.as_deref()))
            .collect();
        let tabs: Vec<crate::components::file_tabs::TabView> = indices
            .iter()
            .zip(icons)
            .map(|(index, icon)| {
                let file = self.files.at(*index);
                crate::components::file_tabs::TabView {
                    name: file.name(),
                    modified: file.document.is_modified(),
                    transient: file.transient,
                    marker: file
                        .path()
                        .map(crate::theme::file_marker)
                        .unwrap_or(crate::theme::color::file_text()),
                    icon,
                }
            })
            .collect();
        let active = self
            .files
            .tab_in_node(node.id)
            .and_then(|showing| indices.iter().position(|index| *index == showing))
            .unwrap_or(0);
        let opacity = self.settings.opacity;
        let outcome = {
            let mut strip_ui = ui.new_child(egui::UiBuilder::new().max_rect(strip));
            // The pane number a strip is given is only an id salt and a drag's origin, and a node is not a
            // pane — so it is given the node's id, which is unique across both by construction.
            crate::components::file_tabs::show(
                &mut strip_ui,
                strip,
                &tabs,
                active,
                node.id as usize,
                focused,
                opacity,
            )
        };
        // **Where this strip drew itself, and where the node is**, for the drag to be settled against once
        // everything has been drawn — which is `settle_the_tab_drag`'s own reason and is what lets a tab be
        // dragged out of a node into a pane, or the other way. `task-1905`.
        let camera = self.space.space.current().camera;
        let on_screen = camera.rect_to_screen(self.space.body.min, node.rect());
        self.node_tab_strips.push((node.id, on_screen.intersect(self.space.body), outcome.strip.clone()));
        let at = |within: usize| indices.get(within).copied();
        if let Some((within, pointer)) = outcome.dragging {
            if let Some(file) = at(within) {
                // The pointer comes back in the node's own world points, and the drag is settled in screen
                // points against every strip in the window — so it is converted here, where the camera is
                // to hand.
                self.tab_drag = Some(crate::app::TabDrag {
                    file,
                    at: camera.to_screen(self.space.body.min, pointer),
                    dropped: outcome.dropped,
                });
            }
        }
        if let Some(index) = outcome.show.and_then(at).or_else(|| outcome.keep.and_then(at)) {
            self.files.show(index);
            self.files.focus_node(node.id);
            self.space.space.choose(Some(node.id));
            self.take_the_keyboard_for_the_space();
        }
        if let Some(index) = outcome.keep.and_then(at) {
            self.files.make_permanent(index);
        }
        if let Some(index) = outcome.close.and_then(at) {
            // Closing a tab writes it if it was edited, which is `close_tab`'s own promise. A node whose
            // last tab is closed draws "Give this node a file to edit", which is what it drew before one
            // was opened.
            self.close_tab(index);
        }
        if let Some((within, where_)) = outcome.menu {
            if let Some(index) = at(within) {
                self.files.show(index);
                self.files.focus_node(node.id);
                self.space.space.choose(Some(node.id));
                self.take_the_keyboard_for_the_space();
            }
            self.tab_menu = Some((where_, node.id as usize));
        }
    }

    // ------------------------------------------------------------------------------- state

    /// What a node's header says when nobody has renamed it.
    pub fn name_of_a_node(&self, node: &Node) -> String {
        match &node.state {
            // **A node with a command is called after its command.** The session's own name is the
            // program the operating system started, and since a batch file is started through
            // `cmd.exe` a node running `codex` was called `cmd.exe` on a live window. The command is
            // also what a person typed, and it does not change under them while the program sets and
            // resets a title of its own, which `claude` does on every prompt.
            State::Terminal(terminal) => match terminal.command.trim() {
                "" => match self.space.live.terminal(node.id) {
                    Some(session) => session.name().to_owned(),
                    None => "Terminal".to_owned(),
                },
                command => crate::services::space::launch::words(command)
                    .first()
                    .map(|program| {
                        std::path::Path::new(program)
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().to_string())
                            .unwrap_or_else(|| program.clone())
                    })
                    .unwrap_or_else(|| "Terminal".to_owned()),
            },
            State::Browser(browser) => match browser.url.is_empty() {
                true => "Web Browser".to_owned(),
                false => browser.url.clone(),
            },
            State::Folder(folder) => folder
                .root
                .as_ref()
                .and_then(|root| root.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "Folder View".to_owned()),
            State::Editor(_) => self
                .files
                .tab_in_node(node.id)
                .map(|index| self.files.at(index).name())
                .unwrap_or_else(|| "File Editor".to_owned()),
        }
    }

    /// The keyboard goes to the canvas, and leaves wherever it was.
    pub(crate) fn take_the_keyboard_for_the_space(&mut self) {
        self.focus = Focus::Space;
        // An editor node's tab is the one `active()` answers with while the canvas has the keys, so
        // the focus follows the chosen node here rather than at each of the places that choose one.
        if let Some(node) = self.space.chosen() {
            if let Some(index) = self.files.tab_in_node(node) {
                self.files.show(index);
                self.files.focus_node(node);
            }
        }
    }

    /// The icons and the git colours for one folder node's rows.
    ///
    /// **Built before the node is drawn**, because [`UnluminousApp::plugin_icon`] takes `&mut self` — it
    /// caches the texture it decodes in `services::icons` — and the node loop is holding `self` for the
    /// whole of the frame.
    ///
    /// **A map rather than a question per row**, which is the panel's own reason: it used to search a list
    /// as each row was drawn, comparing paths, so a project with four hundred rows open did a hundred and
    /// sixty thousand path comparisons every frame.
    ///
    /// **Only the rows that are showing.** `FileTree::rows` is what the explorer draws, so a folder nobody
    /// has opened out costs nothing.
    ///
    /// **And git is asked about the window's own repository**, which answers nothing for a path outside it.
    /// A folder node can be rooted anywhere — `task-1905` gave it `Choose Folder...` — and a second
    /// repository per node would be a second working tree for `unluminous-git` to run the machine's real
    /// git on. A node pointed at somebody else's checkout is a node showing files git here knows nothing
    /// about, which is the honest answer rather than a wrong one.
    fn decorations_for_a_folder_node(
        &mut self,
        ctx: &egui::Context,
        node: NodeId,
    ) -> std::collections::HashMap<std::path::PathBuf, crate::components::explorer::Decoration> {
        let Some(tree) = self.space.live.tree(node) else {
            return std::collections::HashMap::new();
        };
        let rows: Vec<std::path::PathBuf> =
            tree.rows().iter().map(|row| row.entry.path.clone()).collect();
        rows.into_iter()
            .map(|path| {
                let icon = self.plugin_icon(ctx, Some(&path));
                let tint = self
                    .git
                    .as_ref()
                    .and_then(|git| git.state_of(&path))
                    .map(crate::app::git_colour);
                (path, crate::components::explorer::Decoration { tint, icon })
            })
            .collect()
    }

    /// [`Self::decorations_for_a_folder_node`] by name, for a test.
    ///
    /// A `TextureHandle` cannot be read back out of a picture, so what a test asserts on is the map the
    /// component is handed.
    pub fn decorations_for_a_folder_node_for_a_test(
        &mut self,
        node: NodeId,
    ) -> std::collections::HashMap<std::path::PathBuf, crate::components::explorer::Decoration> {
        let ctx = self.context.clone().expect("a window has a context");
        self.decorations_for_a_folder_node(&ctx, node)
    }

    /// Give a folder node a tree of its own, the first time it is drawn.
    fn make_sure_a_node_has_a_tree(&mut self, node: &Node) {
        if self.space.live.has_a_tree(node.id) {
            return;
        }
        let State::Folder(folder) = &node.state else { return };
        let root = folder.root.clone().unwrap_or_else(|| self.tree.root().to_path_buf());
        let mut tree = crate::services::file_tree::FileTree::new(root);
        tree.set_exclude(&self.settings.exclude);
        for open in &folder.expanded {
            tree.expand(open);
        }
        self.space.live.put_a_tree(node.id, tree);
    }

    /// Write down which files a File Editor node holds and which was showing, so they come back.
    ///
    /// **Derived rather than reported**, which is `follow_the_open_file`'s rule: a list of the places that
    /// have to remember to write this down — opening a file, closing a tab, dragging one in, dragging one out
    /// — is a list whose next entry is the one that forgets. `task-1906`.
    pub(crate) fn remember_a_nodes_tabs(&mut self, node: NodeId) {
        let open_here: Vec<std::path::PathBuf> = self
            .files
            .tabs_in_node(node)
            .into_iter()
            .filter_map(|index| self.files.at(index).path().map(std::path::Path::to_path_buf))
            .collect();
        // **A file this node was left holding that another node has open is kept in the record.** One file is
        // open once — `OpenFiles::open`'s rule — so two nodes naming the same path cannot both hold it, and
        // whichever view is showing gets it. Forgetting it on the node that lost it would mean coming back to
        // a node with fewer tabs than it was left with, every time a view was switched. `task-1906`.
        let was = match self.space.space.current().node(node).map(|found| &found.state) {
            Some(State::Editor(editor)) => editor.paths.clone(),
            _ => Vec::new(),
        };
        let mut paths = open_here.clone();
        for path in was {
            if paths.contains(&path) {
                continue;
            }
            // Only a file another **node** has; one that has simply been closed is gone on purpose.
            let held_elsewhere = self
                .files
                .index_of(&path)
                .and_then(|index| self.files.at(index).home.node())
                .is_some_and(|held| held != node);
            if held_elsewhere {
                paths.push(path);
            }
        }
        let showing = self
            .files
            .tab_in_node(node)
            .and_then(|index| self.files.at(index).path().map(std::path::Path::to_path_buf))
            .and_then(|path| paths.iter().position(|other| *other == path))
            .unwrap_or(0);
        // Compared before `change`, for the reason `note_where_the_nodes_are_reading` records: `change`
        // marks the canvas dirty whatever the closure did.
        let changed = match self.space.space.current().node(node).map(|node| &node.state) {
            Some(State::Editor(editor)) => editor.paths != paths || editor.showing != showing,
            _ => false,
        };
        if !changed {
            return;
        }
        self.space.space.change(node, |state| {
            if let State::Editor(editor) = state {
                editor.paths = paths;
                editor.showing = showing;
            }
        });
    }

    /// Write down which folders a folder node has open, so they come back.
    fn remember_a_folder_nodes_open_folders(&mut self, node: NodeId) {
        let Some(open) = self.space.live.tree(node).map(|tree| tree.expanded_folders()) else {
            return;
        };
        self.space.space.change(node, |state| {
            if let State::Folder(folder) = state {
                folder.expanded = open;
            }
        });
    }

    /// Start everything behind the view that is showing that is not running.
    ///
    /// One call rather than three at each of the places a view changes. `catch_the_space_up` asks it
    /// whenever the current view is not the one it last brought to life, so a view chosen from the
    /// strip, from the command line, by duplicating one or by deleting the one that was showing all
    /// arrive here without any of them having to remember.
    pub fn bring_the_current_view_to_life(&mut self) {
        // **What the canvas said before it was brought to life**, so restoring it can be told from changing
        // it. Bringing a view to life opens each of a node's tabs in turn, and every one of those calls
        // `remember_a_nodes_tabs`, which compares the tabs open *so far* against the whole saved list: the
        // first path makes that comparison say the list changed, `Space::change` marks the canvas dirty
        // whatever the closure did, and the later calls put the list back without clearing the mark. So a
        // window that opened a project and touched nothing rewrote `space.conf` with byte-identical content,
        // which is the rule `Space::is_dirty` exists to keep. A comparison here is the cheapest place to
        // answer it, because it is the one place that knows the whole of the restore is over.
        let before = match self.space.space.is_dirty() {
            true => None,
            false => Some(self.space.space.clone()),
        };
        self.start_the_canvass_terminals();
        self.open_the_canvass_browsers();
        self.open_the_canvass_editors();
        self.scroll_the_canvass_folders();
        // A restore that really changed the canvas — a node whose file has gone, a terminal given a fresh
        // conversation id — is still dirty and is still written, which is what those cases need.
        if let Some(before) = before {
            if before.holds_the_same_as(&self.space.space) {
                self.space.space.written();
            }
        }
        self.space.brought_to_life = Some(self.space.space.current_id());
    }

    /// Put every folder node's saved scroll back where it was.
    ///
    /// **The fourth of the four, and it was missing.** The other three each turn something written down into
    /// something running; a folder node's rows are already drawn from its own state, so its scroll looked as
    /// though it needed nothing — and `Folder::scroll` was written to `space.conf` and read back out of it
    /// while nothing ever put the number anywhere the drawing reads. Where the rows are scrolled to lives in
    /// `Live::scrolls` rather than on the node, because a node registers no `AreaState` and so can never take
    /// the wheel itself; the window reads the wheel and answers with `View::scroll_to`, which is the same
    /// pair the panel's own zoom drives.
    ///
    /// It is worse than a scroll that came back at the top, which is what made it worth finding: the first
    /// idle frame after a restart compared the saved 120 against the live 0, decided the rows had moved, and
    /// wrote the zero over the file. So one restart lost the number and every later one had nothing to lose.
    pub(crate) fn scroll_the_canvass_folders(&mut self) {
        let scrolls: Vec<(NodeId, f32)> = self
            .space
            .space
            .current()
            .nodes
            .iter()
            .filter_map(|node| match &node.state {
                State::Folder(folder) if folder.scroll > 0.5 => Some((node.id, folder.scroll)),
                _ => None,
            })
            .collect();
        for (node, scroll) in scrolls {
            self.space.live.scroll_to(node, scroll);
        }
    }

    /// Start a session behind every terminal node on the view that is showing that has none.
    ///
    /// **What a project comes back with.** `services::project_state` says of the terminal tile that
    /// what a program was doing cannot be brought back and what is restored is the same shells in the
    /// same folder; this is that promise for the canvas, and it is what makes a restored terminal node
    /// a terminal rather than a picture of one. It is also what a **copied** view needs: a node on a
    /// copy is a second node, so it gets a second shell.
    ///
    /// A node whose program has **ended** is left alone. Starting it again would make `exit` mean
    /// "start another shell", and `Restart` on the node's own menu is what asks for that.
    pub(crate) fn start_the_canvass_terminals(&mut self) {
        let waiting: Vec<NodeId> = self
            .space
            .space
            .current()
            .nodes
            .iter()
            .filter(|node| node.kind() == Kind::Terminal)
            .map(|node| node.id)
            .filter(|node| !self.space.live.has_a_terminal(*node))
            .collect();
        for node in waiting {
            // **A node that already has a conversation is resumed onto it.** `task-1906`: *"terminal session
            // should still have claude-code open with same session."* `Resume session` on the node's own menu
            // is what asks for this by hand; a restored node asks for it by itself, because coming back is
            // exactly the case it was written for. A node with no session recorded starts fresh, which is
            // every shell and every agent that cannot take an id.
            let resume = self
                .space
                .space
                .current()
                .node(node)
                .and_then(|found| match &found.state {
                    State::Terminal(terminal) => Some(!terminal.session.trim().is_empty()),
                    _ => None,
                })
                .unwrap_or(false);
            if let Err(problem) = self.start_a_space_terminal(node, resume) {
                self.message = Some(problem);
            }
        }
    }

    /// Open the file every editor node on the view that is showing was left on.
    ///
    /// The third of the three, and the same promise: what comes back is the file and where in it the
    /// caret was, which is what `open-files.txt` already restores for a pane. A node whose file has
    /// since been deleted is left empty rather than refusing, which is the rule the whole of
    /// `project_state` keeps.
    pub(crate) fn open_the_canvass_editors(&mut self) {
        // **Every tab, and then the one that was showing.** `task-1906`: a node holds a strip of tabs since
        // `task-1905`, and opening one path brought one of them back. The caret and the scroll follow the file
        // that was showing, which is the one thing a node deliberately keeps less of than a pane — see §7 of
        // the design.
        let waiting: Vec<(NodeId, Vec<std::path::PathBuf>, usize, usize, f32)> = self
            .space
            .space
            .current()
            .nodes
            .iter()
            .filter_map(|node| match &node.state {
                State::Editor(editor) if !editor.paths.is_empty() => Some((
                    node.id,
                    editor.paths.clone(),
                    editor.showing,
                    editor.caret,
                    editor.scroll,
                )),
                _ => None,
            })
            .filter(|(node, ..)| self.files.tabs_in_node(*node).is_empty())
            .collect();
        for (node, paths, showing, caret, scroll) in waiting {
            for path in &paths {
                // **A file already living on another node is left where it is.** `OpenFiles::open`'s rule is
                // that a file already open is *shown* rather than opened twice — two `Document`s over one path
                // would be two windows on one file — so `open_in_a_space_node` **moves** the tab. Bringing a
                // view to life therefore stole a file from a node on the view being left, and the tab
                // vanished from the canvas somebody came back to. Measured on the installed build: three
                // paths on one node became two after switching views and back. `task-1906`.
                //
                // The path stays written down on both nodes, which is right: it is what each was left
                // holding, and whichever view is showing opens the ones it can. Nothing is lost, and no node
                // takes a file out of another.
                if let Some(open) = self.files.index_of(path) {
                    if self.files.at(open).home.node().is_some_and(|held| held != node) {
                        continue;
                    }
                }
                let _ = self.open_in_a_space_node(node, path);
            }
            // The one that was showing, and where in it the reader was.
            let wanted = paths.get(showing.min(paths.len().saturating_sub(1))).cloned();
            if let Some(wanted) = wanted {
                if let Some(index) = self.files.index_of(&wanted) {
                    self.files.show(index);
                    let end = self.files.at(index).document.text().len_bytes();
                    self.files
                        .at_mut(index)
                        .document
                        .apply(unluminous_core::Command::PlaceCaret {
                            offset: caret.min(end),
                            extend: false,
                        });
                    self.files.at_mut(index).scroll = scroll.max(0.0);
                }
            }
            // **Written down again once the right tab is showing.** Opening the tabs one at a time shows each
            // as it arrives, and every `open_in_a_space_node` calls `remember_a_nodes_tabs` — so by the end of
            // the loop above the node has recorded the **last** path as the one showing rather than the one it
            // was left on. The line above then shows the right one, and nothing told the record. It corrected
            // itself on the next idle frame, so the tab a person saw was right; what was wrong was that a
            // window which opened a project and touched nothing had a canvas needing to be written, which is
            // the rule `Space::is_dirty` exists to keep.
            self.remember_a_nodes_tabs(node);
        }
    }

    /// Point every browser node on the view that is showing at the address it was left on.
    ///
    /// The same promise as the terminals, for a page: what comes back is the address, not the session
    /// behind it. A node with no address is left empty, which is what it looks like before anybody has
    /// given it one.
    pub(crate) fn open_the_canvass_browsers(&mut self) {
        let waiting: Vec<(NodeId, String)> = self
            .space
            .space
            .current()
            .nodes
            .iter()
            .filter_map(|node| match &node.state {
                State::Browser(browser) if !browser.url.trim().is_empty() => {
                    Some((node.id, browser.url.clone()))
                }
                _ => None,
            })
            .filter(|(node, _)| self.space.live.browser(*node).is_none())
            .collect();
        for (node, url) in waiting {
            let _ = self.open_a_space_browser(node, &url);
        }
    }

    /// Make a terminal node's session, or start it again.
    pub(crate) fn start_a_space_terminal(&mut self, node: NodeId, resume: bool) -> Result<(), String> {
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return Err(format!("There is no node {node}."));
        };
        // The id this run will use, chosen before the command line is built and written down after it
        // starts — so a node comes back on the conversation it was on rather than on a new one.
        let session_wanted = crate::services::agent_tasks::new_session_id();
        // What the command line will really say about the conversation, asked once and both used and written
        // down from the one answer.
        let decided = crate::services::space::launch::session_for(
            &match &found.state {
                State::Terminal(terminal) => terminal.command.clone(),
                _ => String::new(),
            },
            &match &found.state {
                State::Terminal(terminal) => terminal.session.clone(),
                _ => String::new(),
            },
            resume,
            &session_wanted,
        )
        .1;
        let settings = self.space_terminal_settings_for(&found, resume, &session_wanted)?;
        let parts = space_view::parts_of(&found);
        let font = space_view::font_size_of(&found, self.settings.terminal_font_size);
        let cell = self.renderer.cell_metrics(font);
        let size = crate::components::terminal_panel::grid_size(parts.body.size(), cell);
        let waker = self.waker();
        match unluminous_terminal::Session::spawn(&settings, size, waker) {
            Ok(session) => {
                self.space.live.start_terminal(node, session);
                self.space.live.follow_from_here(node);
                // **Written down once it really started**, so a node whose program would not start is not
                // left claiming a conversation nothing is on.
                //
                // **And it records what was really sent**, which is `session_for`'s whole reason for
                // answering with both halves. This used to ask `!resume`, which is not the same question: a
                // node with no session started with `resume` true — which is what `Resume session` on the
                // node's own menu does to a node that has never run — was given a fresh `--session-id` and
                // then recorded nothing, so Claude answered to an id the node had already forgotten and the
                // next restart resumed nothing. Asking the value that built the command line cannot disagree
                // with it.
                if let Some(id) = decided {
                    if Some(&id)
                        != self.space.space.current().node(node).and_then(|node| match &node.state {
                            State::Terminal(terminal) => Some(&terminal.session),
                            _ => None,
                        })
                    {
                        self.space.space.change(node, |state| {
                            if let State::Terminal(terminal) = state {
                                terminal.session = id;
                            }
                        });
                    }
                }
                Ok(())
            }
            Err(problem) => Err(format!("The program would not start: {problem}")),
        }
    }

    /// What a terminal node's session is started with: the program, its arguments, its folder and its
    /// environment.
    ///
    /// Split out from starting it so a test can read it with no process behind it — the same bargain
    /// `new_detached_space_node` makes about a session, applied to the arguments it would have been given.
    fn space_terminal_settings_for(
        &self,
        found: &Node,
        resume: bool,
        session_wanted: &str,
    ) -> Result<unluminous_terminal::session::SessionSettings, String> {
        let State::Terminal(terminal) = &found.state else {
            return Err("That node is not a terminal.".to_owned());
        };
        let node = found.id;
        let folder = terminal.folder.clone().unwrap_or_else(|| self.tree.root().to_path_buf());
        // **Resolved before it is spawned**, which is what makes `codex` work at all on Windows — see
        // `services::space::launch` for the three files npm installs and which of them can be started.
        // **An agent that takes a conversation id is given one, and gets the same one back.** `task-1906`:
        // *"terminal session should still have claude-code open with same session."* A node with a session id
        // already recorded is resumed onto it; one without is *given* a fresh one, which goes on the command
        // line as `--session-id` so Claude answers to it afterwards. See `launch::takes_a_session`, and
        // `agent::why_it_cannot_resume` for the agent this cannot be done for.
        //
        // The id is chosen here and written down by the caller, because this function builds a command line
        // and does not change the canvas — `run_cli`'s own split.
        let mut command = terminal.command.clone();
        let (words, _) = crate::services::space::launch::session_for(
            &command,
            &terminal.session,
            resume,
            session_wanted,
        );
        command.push_str(&words);
        let launch = crate::services::space::launch::resolve(&command, self.settings.shell())?;
        Ok(unluminous_terminal::session::SessionSettings {
            shell: launch.program,
            args: launch.args,
            working_directory: Some(folder),
            // **Which node this is**, so an agent started in it knows without being told. It is a number
            // rather than anything a secret could be kept in — `SessionSettings::env`'s own note.
            //
            // **And a sentence saying there is something to read.** A variable holding English is unusual
            // and the reason is measured rather than stylistic: `task-1905` watched an agent run `env`,
            // read a number, and learn nothing it could act on — it then spent nine tool calls and two
            // shell commands establishing what one call answers. It is the cheapest possible place to put
            // a pointer, it costs the child nothing, and any agent reads it whatever its own tooling.
            // `UNLUMINOUS_SPACE_NODE` is untouched, because a program that wants the id wants the id.
            //
            // **And where `unluminous-cli` is, and which window it drives.** `unluminous-cli` is on
            // nobody's `PATH` — on macOS it is inside the application bundle beside `unluminous` and on
            // Windows in the installation folder — so an agent told to run `unluminous-cli space here`
            // answers `command not found`, which is what driving the real window found. The Agent-Tasks
            // board already solved this: `agent::ENV_CLI` and `ENV_INSTANCE`, filled in by
            // `agent_tasks::beside_this_program`. The hint uses those variables rather than a bare name,
            // so what it tells an agent to run is a command that works.
            env: {
                let cli = crate::services::agent_tasks::beside_this_program("unluminous-cli");
                let instance = std::process::id().to_string();
                let hint = format!(
                    "This terminal is node {node} on an Unluminous canvas. Run `\"${cli_var}\" --instance ${instance_var} space here` to see which nodes it is wired to and how to drive them.",
                    cli_var = crate::services::agent_tasks::agent::ENV_CLI,
                    instance_var = crate::services::agent_tasks::agent::ENV_INSTANCE,
                );
                vec![
                    ("UNLUMINOUS_SPACE_NODE".to_owned(), node.to_string()),
                    ("UNLUMINOUS_SPACE_HINT".to_owned(), hint),
                    (crate::services::agent_tasks::agent::ENV_CLI.to_owned(), cli),
                    (crate::services::agent_tasks::agent::ENV_INSTANCE.to_owned(), instance),
                ]
            },
        })
    }

    /// The same, for a node named by its id, which is what a test asks.
    ///
    /// **`fresh` is the id a first run would be given**, and it is the caller's rather than made up here. It
    /// used to reuse whatever the node had already recorded, which is a command line nothing builds: the real
    /// path asks `agent_tasks::new_session_id()` every time and only a *resume* reuses the recorded one — so a
    /// helper that promised "the command line the node would really build" described one no run produces. The
    /// Codex Sol review found it, and the fix is to make the caller say which run it is asking about, because
    /// that is the thing the two differ on.
    pub fn space_terminal_settings(
        &self,
        node: NodeId,
        fresh: &str,
    ) -> Option<unluminous_terminal::session::SessionSettings> {
        let found = self.space.space.current().node(node)?.clone();
        self.space_terminal_settings_for(&found, false, fresh).ok()
    }

    /// The same, as a **restored** node builds it: resuming the conversation it was left on.
    ///
    /// What `start_the_canvass_terminals` asks for on a node that has a session recorded, which is the half
    /// `task-1906` adds and the half a test cannot reach by starting a process.
    pub fn space_terminal_settings_resuming(
        &self,
        node: NodeId,
    ) -> Option<unluminous_terminal::session::SessionSettings> {
        let found = self.space.space.current().node(node)?.clone();
        self.space_terminal_settings_for(&found, true, "a-session-for-a-test").ok()
    }

    /// Send a browser node to an address.
    ///
    /// **An address is resolved the way the editing area resolves one**, through
    /// `BrowserLocation::parse`, which is what turns `page.html` into the `unluminous://` project
    /// origin. Handing the typed text straight to the view is what a live window refused with
    /// *"Class not registered"*: wry passes an unknown scheme to `Navigate`, which refuses it.
    ///
    /// A **remote** address on a tab that already exists is a navigation, so the node's history is
    /// kept. Anything else opens a tab, because a local page's root is registered when its tab is
    /// opened and cannot be changed underneath one.
    pub(crate) fn send_a_space_browser_to(&mut self, node: NodeId, address: &str) -> Result<(), String> {
        let location = crate::services::browser::BrowserLocation::parse(address, self.tree.root())?;
        let remote = location.source_path().is_none();
        if let (true, Some(tab)) = (remote, self.space.live.browser(node).map(|tab| tab.id)) {
            self.browser.navigate(tab, address.trim())?;
            self.space.space.change(node, |state| {
                if let State::Browser(browser) = state {
                    browser.url = address.to_owned();
                }
            });
            return Ok(());
        }
        self.open_a_space_browser(node, address)
    }

    /// Point a browser node at an address, in a tab of its own.
    pub(crate) fn open_a_space_browser(&mut self, node: NodeId, address: &str) -> Result<(), String> {
        if !crate::services::browser::SUPPORTED {
            return Err("Rendered web pages are available on Windows and macOS.".to_owned());
        }
        let location =
            crate::services::browser::BrowserLocation::parse(address, self.tree.root())?;
        // The tab this node had, closed before the new one is made: a node shows one page, and a tab
        // nothing points at is a tab the one native view can still be asked to show.
        if let Some(was) = self.space.live.browser(node).map(|tab| tab.id) {
            self.browser.close_tab(was);
        }
        let tab = self.browser.open_tab(location);
        self.space.live.put_a_browser(node, tab);
        self.space.space.change(node, |state| {
            if let State::Browser(browser) = state {
                browser.url = address.to_owned();
            }
        });
        Ok(())
    }

    /// Put a file in an editor node, moving the tab if it is already open somewhere else.
    pub fn open_in_a_space_node(&mut self, node: NodeId, path: &std::path::Path) -> Result<(), String> {
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return Err(format!("There is no node {node}."));
        };
        if found.kind() != Kind::Editor {
            return Err("That node is not a file editor.".to_owned());
        }
        // **A file already in this node is shown rather than opened twice**, and one that is not joins the
        // tabs already there. `task-1905` asks for the node to hold several — *"just like our editing
        // area, where I can see and edit files in multiple tabs"* — where it used to close whatever was
        // there before opening the next one.
        if let Some(already) =
            self.files.tabs_in_node(node).into_iter().find(|index| self.files.at(*index).path() == Some(path))
        {
            self.files.show(already);
            self.files.focus_node(node);
            self.focus = Focus::Space;
            self.space.space.choose(Some(node));
            return Ok(());
        }
        // **The one place a file is opened**, so a node's editor and the editing area's read a file
        // the same way, refuse it the same way, and answer the same thing when they cannot.
        self.open_path_permanently(path)?;
        let Some(index) = self.files.index_of(path) else {
            return Err(format!("{} did not open.", path.display()));
        };
        self.files.move_to_node(index, node);
        self.remember_a_nodes_tabs(node);
        self.focus = Focus::Space;
        self.space.space.choose(Some(node));
        Ok(())
    }

    /// Take a node off the canvas, stopping whatever was behind it.
    pub(crate) fn close_a_space_node(&mut self, node: NodeId) {
        // **Every tab on it, not only the one showing.** A node holds several since `task-1905`, and closing
        // one tab left the rest with a `Home::Node` naming a node that had gone — reachable from nothing,
        // drawn by nothing, and still holding whatever was typed into them. The Codex Sol review found it.
        //
        // Highest index first, because `close_tab` removes from the vector and every index after it shifts.
        let mut on_it = self.files.tabs_in_node(node);
        on_it.sort_unstable_by(|left, right| right.cmp(left));
        for index in on_it {
            // Closing a tab writes it if it was edited, which is `close_tab`'s own promise.
            self.close_tab(index);
        }
        if let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) {
            self.browser.close_tab(tab);
        }
        self.space.live.forget(node);
        self.space.space.remove_node(node);
    }

    /// Walk a terminal node's font size along the list the Settings window offers.
    pub(crate) fn step_a_node_font(&mut self, node: NodeId, steps: i32) {
        let Some(found) = self.space.space.current().node(node).cloned() else { return };
        let was = space_view::font_size_of(&found, self.settings.terminal_font_size);
        let mut size = was;
        for _ in 0..steps.abs() {
            size = crate::settings::step_terminal_font_size(size, steps > 0);
        }
        if (size - was).abs() < 0.01 {
            return;
        }
        self.space.space.change(node, |state| {
            if let State::Terminal(terminal) = state {
                terminal.font_size = size;
            }
        });
    }

    /// A terminal node with **no shell behind it**, fed bytes directly.
    ///
    /// What the screenshot tests use, exactly as [`UnluminousApp::new_detached_terminal_tab`] is what
    /// the terminal tile's use and for the same reason: when a real program answers is not something
    /// a test can know, so a picture of a node is taken of an emulator that was handed fixed bytes.
    pub fn new_detached_space_node(&mut self, kind: Kind, at: Pos2) -> NodeId {
        let project = self.tree.root().to_path_buf();
        let node = self.space.space.add_node(kind, at, Some(&project));
        if kind == Kind::Terminal {
            let found = self.space.space.current().node(node).cloned().expect("it was just made");
            let parts = space_view::parts_of(&found);
            let font = space_view::font_size_of(&found, self.settings.terminal_font_size);
            let cell = self.renderer.cell_metrics(font);
            let size = crate::components::terminal_panel::grid_size(parts.body.size(), cell);
            self.space.live.start_terminal(node, unluminous_terminal::Session::detached(size));
        }
        node
    }

    /// Feed a detached terminal node the bytes a program would have written.
    pub fn feed_a_space_terminal(&mut self, node: NodeId, bytes: &[u8]) {
        if let Some(session) = self.space.live.terminal_mut(node) {
            session.feed(bytes);
        }
    }

    /// Give a browser node a tab with **no native view behind it**, which is what a test needs.
    ///
    /// `BrowserHost::open_tab` allocates an id and registers a local root and does not create a view —
    /// the view is made in `reconcile`, before the egui pass. So a tab on its own is a value, and this is
    /// `new_detached_space_node`'s bargain applied to a page: a picture of a node, and a state a test can
    /// read, without waiting on WebView2 or WKWebView to answer.
    pub fn new_detached_space_page(&mut self, node: NodeId, url: &str) -> Option<u64> {
        let location =
            crate::services::browser::BrowserLocation::parse(url, self.tree.root()).ok()?;
        let tab = self.browser.open_tab(location);
        let id = tab.id;
        self.space.live.put_a_browser(node, tab);
        self.space.space.change(node, |state| {
            if let State::Browser(browser) = state {
                browser.url = url.to_owned();
                browser.typed = url.to_owned();
            }
        });
        Some(id)
    }

    // ------------------------------------------------------------------------------- each frame

    /// Read what every node's program has said, and carry whatever the pipes carry.
    ///
    /// Called once a frame whether the canvas is showing or not, which is `UiProvider::catch_up`'s own
    /// bargain: a program that printed while its node was put away must not lose what it printed.
    ///
    /// **What it answers is whether a frame has to be asked for, and that is not the same question as
    /// whether a program is running.** A terminal that prints wakes the window itself, through the
    /// waker its session was started with — asking for a frame because a session merely exists would
    /// keep an idle window drawing for as long as a shell sat at its prompt. What genuinely needs one
    /// is a **pipe**, because reading one is a poll on a clock and nothing else will wake the window
    /// to do it.
    pub(crate) fn catch_the_space_up(&mut self, now: f64) -> bool {
        // The view that is showing has everything behind it running, whichever way it came to be
        // showing. Asked rather than told - see `bring_the_current_view_to_life`. Not on the first
        // frame, because starting a pseudoconsole before the window is shown is a fifth of the time
        // before anything appears, which is `start_the_restored_terminals`' own measurement.
        if self.frames > 0
            && self.remembers_this_project()
            && self.space.brought_to_life != Some(self.space.space.current_id())
        {
            // **The view being left is written down before the new one is brought to life.** What
            // `note_where_the_nodes_are_reading` records is derived from the live state and it only ever walks
            // the view that is *showing*, so a frame that both moved something and switched view — a wheel and
            // a chip in one input frame, or a drag that ended on `Duplicate View` — left the last movement
            // unrecorded: by the next frame the old view was no longer the one being walked. There is nowhere
            // else to ask it. `show_view` is in `services`, which cannot reach the live state, and there are
            // seven callers of it — which is `follow_the_open_file`'s rule about a list whose next entry is the
            // one that forgets. This is the one place that knows the view has changed, so it is the one place
            // that asks. The Codex Sol review found it.
            //
            // It is the *old* view that is walked, because `brought_to_life` still names it: the guard above
            // is what says a switch has happened and nothing has moved yet.
            if let Some(leaving) = self.space.brought_to_life {
                if self.space.space.view(leaving).is_some() {
                    let showing = self.space.space.current_id();
                    // Showing a view marks the canvas dirty, and going back to the one that is really showing
                    // is not a change to write — so whether it needed writing is put back as it was found,
                    // leaving only whatever the recording itself had to say.
                    let was_dirty = self.space.space.is_dirty();
                    self.space.space.show_view(leaving);
                    self.space.space.written();
                    self.note_where_the_nodes_are_reading();
                    let recorded = self.space.space.is_dirty();
                    self.space.space.show_view(showing);
                    match was_dirty || recorded {
                        true => self.space.space.touch(),
                        false => self.space.space.written(),
                    }
                }
            }
            self.bring_the_current_view_to_life();
        }
        self.space.live.catch_up();
        // **Only once the view that is showing has been brought to life.** What this writes down is derived
        // from the live state, and before the nodes have been started that state is *empty* — so on the frames
        // between a project opening and its canvas coming alive it wrote an empty tab list over the saved one
        // and a canvas came back with its editor nodes blank. Measured on the installed build: `space.conf`
        // held three paths before the restart and none after it. `task-1906`.
        //
        // The same guard the line above uses, which is the honest one: `brought_to_life` is the view whose
        // nodes really are running, and reading a node's state before that is reading nothing.
        if self.space.brought_to_life == Some(self.space.space.current_id()) {
            self.note_where_the_nodes_are_reading();
        }
        let carrying: Vec<(NodeId, NodeId)> = self
            .space
            .space
            .current()
            .edges
            .iter()
            .filter(|edge| edge.pipe == Pipe::Lines)
            .map(|edge| (edge.from, edge.to))
            .collect();
        self.space.live.carry_the_pipes(now, &carrying);
        !carrying.is_empty()
    }

    /// Take down where each node's own file and rows are being read, so they come back there.
    ///
    /// **Derived rather than reported**, which is `follow_the_open_file`'s rule and the reason this is one
    /// function rather than a line at each of the places a caret or a scroll can move: a list of the places
    /// that have to remember to write it down is a list whose next entry is the one that forgets. `task-1906`
    /// is the report — the three fields it fills in were written to `space.conf` and read back from it since
    /// `task-1904`, and nothing ever put a value in one.
    ///
    /// **Only when the number changed**, because `Space::change` marks the canvas dirty and a canvas that
    /// wrote `space.conf` on every frame of a scroll would write a file sixty times a second — which is
    /// `Space::is_dirty`'s whole reason for existing.
    pub fn note_where_the_nodes_are_reading(&mut self) {
        let nodes: Vec<(NodeId, Kind)> = self
            .space
            .space
            .current()
            .nodes
            .iter()
            .map(|node| (node.id, node.kind()))
            .collect();
        for (node, kind) in nodes {
            match kind {
                Kind::Editor => {
                    self.remember_a_nodes_tabs(node);
                    let Some(index) = self.files.tab_in_node(node) else { continue };
                    let caret = self.files.at(index).document.selection().head;
                    let scroll = self.files.at(index).scroll;
                    // **Compared before `change` is called, not inside it.** `Space::change` marks the canvas
                    // dirty whatever the closure did, so asking inside would write `space.conf` on every
                    // frame — which is the one thing `is_dirty` exists to prevent.
                    let moved = match &self.space.space.current().node(node).map(|node| &node.state) {
                        Some(State::Editor(editor)) => {
                            editor.caret != caret || (editor.scroll - scroll).abs() > 0.5
                        }
                        _ => false,
                    };
                    if moved {
                        self.space.space.change(node, |state| {
                            if let State::Editor(editor) = state {
                                editor.caret = caret;
                                editor.scroll = scroll;
                            }
                        });
                    }
                }
                Kind::Folder => {
                    let scroll = self.space.live.scroll_of(node);
                    let moved = match &self.space.space.current().node(node).map(|node| &node.state) {
                        Some(State::Folder(folder)) => (folder.scroll - scroll).abs() > 0.5,
                        _ => false,
                    };
                    if moved {
                        self.space.space.change(node, |state| {
                            if let State::Folder(folder) = state {
                                folder.scroll = scroll;
                            }
                        });
                    }
                }
                Kind::Terminal | Kind::Browser => {}
            }
        }
    }

    /// Write the canvas down when something about it has changed.
    ///
    /// The ticket asks for views "saved on edit". Written at the end of a frame on which something
    /// changed rather than on every frame, because a canvas being dragged would otherwise write a file
    /// sixty times a second.
    pub(crate) fn write_the_space_if_it_changed(&mut self, now: f64) {
        if !self.space.space.is_dirty() || !self.remembers_this_project() {
            return;
        }
        // A write that failed leaves the canvas marked as needing writing, so the next change tries
        // again — but not on every frame, because a disk that is full or read only would then be
        // written to sixty times a second and the status bar would say so as often.
        if let Some(at) = self.space.write_failed_at {
            if now - at < RETRY_A_FAILED_WRITE {
                return;
            }
        }
        match store::save(self.tree.root(), &self.space.space) {
            Ok(()) => {
                self.space.space.written();
                self.space.write_failed_at = None;
            }
            Err(problem) => {
                // The first failure is said once. After that the canvas is still dirty and will be
                // tried again, quietly, rather than filling the status bar with the same sentence.
                if self.space.write_failed_at.is_none() {
                    self.message = Some(problem);
                }
                self.space.write_failed_at = Some(now);
            }
        }
    }

    /// Read a project's canvases when a window opens on it.
    ///
    /// The model only. What is behind the nodes is started on the second frame, by
    /// [`Self::bring_the_current_view_to_life`], for the reason `start_the_restored_terminals`
    /// records: a pseudoconsole opened before the window is shown is a fifth of the time before
    /// anything appears.
    pub(crate) fn restore_the_space(&mut self) {
        self.space.space = store::load(self.tree.root());
        self.space.brought_to_life = None;
    }

    // ------------------------------------------------------------------------------- commands

    /// The one place a canvas action turns into a change.
    pub(crate) fn run_a_space_action(&mut self, action: SpaceAction) {
        match action {
            SpaceAction::Toggle => {
                let showing = self.was_showing(dock::Panel::Space, self.space.visible);
                self.leave_the_maximised_pane();
                self.show_a_panel(dock::Panel::Space, !showing);
            }
            SpaceAction::OpenAddModal => {
                let at = self.middle_of_the_canvas();
                self.space.adding = Some(add_modal::State { at, ..Default::default() });
                self.show_a_panel(dock::Panel::Space, true);
            }
            SpaceAction::Add(kind) => {
                let at = self.middle_of_the_canvas();
                self.show_a_panel(dock::Panel::Space, true);
                self.add_a_space_node(kind, at);
            }
            SpaceAction::Fit => {
                let bounds = self.space.space.current().bounds();
                let size = self.space.body.size();
                self.space.space.current_mut().camera.fit(bounds, size, 32.0);
                self.space.space.touch();
            }
            SpaceAction::Manage => {
                self.show_a_panel(dock::Panel::Space, true);
                self.space.managing = Some(crate::components::space::manager::State::default());
            }
            SpaceAction::NewView => {
                let id = self.space.space.add_view("View");
                self.space.space.show_view(id);
            }
            SpaceAction::RenameView => {
                let id = self.space.in_hand.view.unwrap_or_else(|| self.space.space.current_id());
                let Some(view) = self.space.space.view(id) else { return };
                self.prompt = Some(crate::components::prompt_dialog::Prompt::new(
                    "Rename View",
                    "What this canvas is called in the strip along the top.",
                    &view.name,
                    "Rename",
                    crate::components::prompt_dialog::Purpose::RenameSpaceView(id),
                ));
            }
            SpaceAction::DuplicateView => {
                let id = self.space.in_hand.view.unwrap_or_else(|| self.space.space.current_id());
                match self.space.space.duplicate_view(id) {
                    Some(copy) => {
                        self.space.space.show_view(copy);
                        self.bring_the_current_view_to_life();
                        self.message = Some("The view was duplicated.".to_owned());
                    }
                    None => self.message = Some("There is no such view.".to_owned()),
                }
            }
            SpaceAction::DeleteView => {
                let id = self.space.in_hand.view.unwrap_or_else(|| self.space.space.current_id());
                self.delete_a_space_view(id);
            }
            SpaceAction::RenameNode => {
                let Some(node) = self.space.chosen() else {
                    self.message = Some("No node is chosen.".to_owned());
                    return;
                };
                let Some(found) = self.space.space.current().node(node).cloned() else { return };
                let name = match found.title.trim().is_empty() {
                    true => self.name_of_a_node(&found),
                    false => found.title.clone(),
                };
                self.prompt = Some(crate::components::prompt_dialog::Prompt::new(
                    "Rename Node",
                    "What this node is called on its header. An empty name puts it back to being called after what it holds.",
                    &name,
                    "Rename",
                    crate::components::prompt_dialog::Purpose::RenameSpaceNode(node),
                ));
            }
            SpaceAction::CloseNode => match self.space.chosen() {
                Some(node) => self.close_a_space_node(node),
                None => self.message = Some("No node is chosen.".to_owned()),
            },
            SpaceAction::ChooseFolder => self.choose_a_folder_for_a_node(),
            SpaceAction::RestartNode => self.restart_a_space_node(false),
            SpaceAction::ResumeSession => self.restart_a_space_node(true),
            SpaceAction::Disconnect => {
                let Some(edge) = self.space.in_hand.wire else { return };
                self.space.space.disconnect(edge);
                self.space.in_hand.wire = None;
            }
            SpaceAction::CarryLines(on) => {
                let Some(edge) = self.space.in_hand.wire else { return };
                let carrying = if on { Pipe::Lines } else { Pipe::Off };
                let ends = self
                    .space
                    .space
                    .current()
                    .edges
                    .iter()
                    .find(|other| other.id == edge)
                    .map(|other| other.from);
                match self.space.space.set_pipe(edge, carrying) {
                    Ok(()) => {
                        if let (Pipe::Lines, Some(from)) = (carrying, ends) {
                            // From here rather than from the beginning of the program's output, or
                            // turning a pipe on would empty one terminal's history into the other.
                            self.space.live.follow_from_here(from);
                        }
                    }
                    Err(problem) => self.message = Some(problem),
                }
            }
        }
    }

    /// Put a node on the canvas and start whatever is behind it.
    pub(crate) fn add_a_space_node(&mut self, kind: Kind, at: Pos2) -> NodeId {
        let project = self.tree.root().to_path_buf();
        let id = self.space.space.add_node(kind, at, Some(&project));
        if kind == Kind::Terminal {
            if let Err(problem) = self.start_a_space_terminal(id, false) {
                self.message = Some(problem);
            }
        }
        self.take_the_keyboard_for_the_space();
        id
    }

    /// Ask which folder a Folder View node shows, and point it there.
    ///
    /// **The same `rfd` dialog `Open Folder` and the Database plugin's file picker already use.** It blocks
    /// inside a frame, which is what a native file dialog is on every platform and what the window already
    /// does for `Open File`.
    ///
    /// What it then does is `space folder <node> root`, which already exists and already does the right
    /// thing: it writes the root, clears `expanded`, and **forgets** the live tree so it is built again
    /// from the node's own state on the next frame. So the pointer and the command line reach one
    /// function, which is `run_cli`'s rule, and no new state is invented. `task-1905`.
    fn choose_a_folder_for_a_node(&mut self) {
        let Some(node) = self.space.chosen() else {
            self.message = Some("No node is chosen.".to_owned());
            return;
        };
        let Some(found) = self.space.space.current().node(node).cloned() else { return };
        let State::Folder(folder) = &found.state else {
            self.message = Some("That node is not a folder view.".to_owned());
            return;
        };
        // Start where the node already is. On a node whose root has been deleted since, that is the
        // folder above it rather than nowhere.
        let start = folder
            .root
            .clone()
            .filter(|root| root.is_dir())
            .or_else(|| folder.root.as_ref().and_then(|root| root.parent().map(std::path::Path::to_path_buf)))
            .unwrap_or_else(|| self.tree.root().to_path_buf());
        let Some(chosen) = rfd::FileDialog::new()
            .set_title("Choose the folder this node shows")
            .set_directory(&start)
            .pick_folder()
        else {
            return;
        };
        self.point_a_folder_node_at(node, &chosen);
    }

    /// Point a folder node at a folder. The one place that happens, so the dialog and `space folder root`
    /// cannot come apart.
    pub(crate) fn point_a_folder_node_at(&mut self, node: NodeId, root: &std::path::Path) {
        self.space.space.change(node, |state| {
            if let State::Folder(folder) = state {
                folder.root = Some(root.to_path_buf());
                folder.expanded.clear();
            }
        });
        // Forgotten rather than re-rooted, so the tree is built again from the node's own state on the
        // next frame — one place a node's tree comes from.
        self.space.live.forget(node);
    }

    /// Throw a view away, stopping everything that was on it.
    pub(crate) fn delete_a_space_view(&mut self, id: u64) {
        if self.space.space.views().len() < 2 {
            self.message = Some("A canvas always has one view.".to_owned());
            return;
        }
        let Some(view) = self.space.space.view(id) else { return };
        let nodes: Vec<NodeId> = view.nodes.iter().map(|node| node.id).collect();
        // Every tab on every node, highest index first — see `close_a_space_node` for why all of them and
        // why in that order.
        let mut on_them: Vec<usize> =
            nodes.iter().flat_map(|node| self.files.tabs_in_node(*node)).collect();
        on_them.sort_unstable_by(|left, right| right.cmp(left));
        for index in on_them {
            self.close_tab(index);
        }
        for node in &nodes {
            if let Some(tab) = self.space.live.browser(*node).map(|tab| tab.id) {
                self.browser.close_tab(tab);
            }
        }
        self.space.live.forget_all(&nodes);
        self.space.space.delete_view(id);
        // Deleting the view that was showing moves to another one, which has to be brought to life
        // like any other view somebody chose.
        self.bring_the_current_view_to_life();
    }

    /// Start the chosen terminal node's program again.
    fn restart_a_space_node(&mut self, resume: bool) {
        let Some(node) = self.space.chosen() else {
            self.message = Some("No node is chosen.".to_owned());
            return;
        };
        match self.start_a_space_terminal(node, resume) {
            Ok(()) => self.message = Some("Started again.".to_owned()),
            Err(problem) => self.message = Some(problem),
        }
    }

    /// The world point in the middle of what is showing, which is where a node with no place of its
    /// own goes.
    pub(crate) fn middle_of_the_canvas(&self) -> Pos2 {
        let body = self.space.body;
        if body.width() < 2.0 {
            return Pos2::ZERO;
        }
        let camera = self.space.space.current().camera;
        let middle = camera.to_world(body.min, body.center());
        // The top left corner rather than the middle, because that is what a node's place is.
        middle - Vec2::new(160.0, 120.0)
    }

    /// What the strip of views asked for.
    fn act_on_the_view_bar(&mut self, outcome: crate::components::space::BarOutcome) {
        if let Some(id) = outcome.show {
            self.space.space.show_view(id);
            self.bring_the_current_view_to_life();
        }
        if outcome.add {
            self.run_a_space_action(SpaceAction::NewView);
        }
        if let Some((at, id)) = outcome.menu {
            self.space.in_hand.view = Some(id);
            self.space.menu = Some((at, Menu::View));
        }
        // **About the middle of the pane**, because a button has no pointer — which is the same choice
        // `step_the_zoom_of` already makes for the keys. `task-1905`.
        if outcome.zoom != 0 {
            let body = self.space.body;
            self.space.space.current_mut().camera.zoom_by(outcome.zoom, body.min, body.center());
            self.space.space.touch();
        }
        if outcome.manage {
            self.run_a_space_action(SpaceAction::Manage);
        }
        if outcome.reset_zoom {
            let body = self.space.body;
            self.space.space.current_mut().camera.zoom_to(1.0, body.min, body.center());
            self.space.space.touch();
        }
    }


    // ------------------------------------------------------------------------------- keyboard

    /// The keys the canvas takes, read before any pane is drawn.
    ///
    /// The same place and the same reason as `route_the_explorer_keys`: a key that meant two things at
    /// once would be worse than one that meant neither, so whichever surface has the keyboard reads
    /// its own keys before anything else reads the frame. A terminal node reads none of them - its
    /// grid takes every key it is given, which is what a terminal is - so this is about a **folder**
    /// node's own cursor, and about `Escape`.
    pub(crate) fn route_the_space_keys(&mut self, ui: &egui::Ui) -> Option<crate::app::actions::Action> {
        if !matches!(self.focus, Focus::Space) || !self.space.visible {
            return None;
        }
        if crate::app::a_modal_has_the_keyboard(ui.ctx())
            || crate::app::text_box_has_the_keyboard(ui.ctx())
        {
            return None;
        }
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.focus = Focus::Editor;
            return None;
        }
        let node = self.space.chosen()?;
        let found = self.space.space.current().node(node)?.clone();
        if found.kind() != Kind::Folder {
            return None;
        }
        let key = ui.input(|input| {
            [
                egui::Key::ArrowDown,
                egui::Key::ArrowUp,
                egui::Key::ArrowRight,
                egui::Key::ArrowLeft,
                egui::Key::Enter,
                egui::Key::Delete,
            ]
            .into_iter()
            .find(|key| input.key_pressed(*key))
        })?;
        match key {
            egui::Key::ArrowDown => self.step_a_folder_nodes_cursor(node, 1),
            egui::Key::ArrowUp => self.step_a_folder_nodes_cursor(node, -1),
            egui::Key::ArrowRight => self.open_a_folder_nodes_row(node, true),
            egui::Key::ArrowLeft => self.open_a_folder_nodes_row(node, false),
            egui::Key::Enter => {
                let path = self.space.live.tree_selection(node)?.to_path_buf();
                match path.is_dir() {
                    true => {
                        if let Some(tree) = self.space.live.tree_mut(node) {
                            tree.toggle(&path);
                        }
                        self.remember_a_folder_nodes_open_folders(node);
                    }
                    false => {
                        let _ = self.open_path_permanently(&path);
                    }
                }
            }
            egui::Key::Delete => {
                return self
                    .space
                    .live
                    .tree_selection(node)
                    .map(|path| crate::app::actions::Action::DeletePath(path.to_path_buf()));
            }
            _ => {}
        }
        None
    }

    /// Move a folder node's own cursor down or up its rows.
    fn step_a_folder_nodes_cursor(&mut self, node: NodeId, step: isize) {
        let Some(tree) = self.space.live.tree(node) else { return };
        let rows: Vec<std::path::PathBuf> =
            tree.rows().iter().map(|row| row.entry.path.clone()).collect();
        if rows.is_empty() {
            return;
        }
        let at = self
            .space
            .live
            .tree_selection(node)
            .and_then(|path| rows.iter().position(|row| row == path))
            .map(|at| (at as isize + step).clamp(0, rows.len() as isize - 1) as usize)
            .unwrap_or(if step > 0 { 0 } else { rows.len() - 1 });
        let chosen = rows[at].clone();
        self.space.live.select_in_tree(node, Some(chosen));
    }

    /// `Right` opens the folder the cursor is on; `Left` shuts it, or steps to the folder above.
    fn open_a_folder_nodes_row(&mut self, node: NodeId, open: bool) {
        let Some(path) = self.space.live.tree_selection(node).map(std::path::Path::to_path_buf)
        else {
            return;
        };
        let Some(tree) = self.space.live.tree(node) else { return };
        let root = tree.root().to_path_buf();
        let showing = tree.find(&path).map(|entry| entry.expanded).unwrap_or(false);
        if path.is_dir() && showing != open {
            if let Some(tree) = self.space.live.tree_mut(node) {
                tree.toggle(&path);
            }
            self.remember_a_folder_nodes_open_folders(node);
            return;
        }
        if !open {
            if let Some(folder) = path.parent() {
                if folder.starts_with(&root) && folder != root {
                    self.space.live.select_in_tree(node, Some(folder.to_path_buf()));
                }
            }
        }
    }

    /// The rows the space manager draws: every view of this project's canvas.
    ///
    /// Built here rather than in the component, because a component draws and does not reach into the
    /// window's state — the rule `explorer::Decoration` states and `manager::Row` follows.
    fn rows_for_the_space_manager(&self) -> Vec<crate::components::space::manager::Row> {
        let current = self.space.space.current_id();
        self.space
            .space
            .views()
            .iter()
            .map(|view| crate::components::space::manager::Row {
                id: view.id,
                name: view.name.clone(),
                nodes: view.nodes.len(),
                connections: view.edges.len(),
                showing: view.id == current,
            })
            .collect()
    }

    /// Draw the space manager, when it is open.
    pub(crate) fn show_the_space_manager(&mut self, ui: &mut egui::Ui) {
        let Some(mut state) = self.space.managing.take() else { return };
        let rows = self.rows_for_the_space_manager();
        let outcome = crate::components::space::manager::show(ui.ctx(), &mut state, &rows);
        if let Some(view) = outcome.show {
            self.space.space.show_view(view);
            self.bring_the_current_view_to_life();
        }
        if outcome.add {
            self.run_a_space_action(SpaceAction::NewView);
        }
        // **Another project is another window**, which is §3.2's answer: a canvas names its own project's
        // files, so opening one here would be a canvas of nodes pointing somewhere else. `Action::OpenFolder`
        // is the same native dialog and the same `launcher::open_window` every other route uses.
        if outcome.open_a_project {
            let ctx = ui.ctx().clone();
            self.run_action(crate::app::actions::Action::OpenFolder, &ctx);
        }
        if let Some((at, view)) = outcome.menu {
            self.space.in_hand.view = Some(view);
            self.space.menu = Some((at, Menu::View));
        }
        if !outcome.closed {
            self.space.managing = Some(state);
        }
    }

    /// Draw the add modal, when it is open.
    pub(crate) fn show_the_space_modal(&mut self, ui: &mut egui::Ui) {
        let Some(mut state) = self.space.adding.take() else { return };
        let outcome = add_modal::show(ui.ctx(), &mut state);
        if let Some(kind) = outcome.chose {
            self.add_a_space_node(kind, state.at);
        }
        if !outcome.closed {
            self.space.adding = Some(state);
        }
    }

    /// Draw whichever of the canvas's three right click menus is open, and answer what was chosen.
    pub(crate) fn show_the_space_menu(&mut self, ui: &mut egui::Ui) -> Option<crate::app::actions::Action> {
        let (at, which) = self.space.menu?;
        let state = self.menu_state();
        let entries = match which {
            Menu::Node => crate::app::actions::space_node_menu(&state),
            Menu::Wire => crate::app::actions::space_wire_menu(&state),
            Menu::View => crate::app::actions::space_view_menu(&state),
        };
        let name = match which {
            Menu::Node => "space-node",
            Menu::Wire => "space-wire",
            Menu::View => "space-view",
        };
        let outcome = crate::components::context_menu::show(ui, name, at, &entries);
        if outcome.close {
            self.space.menu = None;
        }
        outcome.chosen
    }
}


/// The `space` area of `unluminous-cli`, which is the agent's half of the canvas.
///
/// **Every one of these goes through the same functions the pointer does.** `space add` is what the
/// right click modal calls, `space connect` is what letting a wire go calls, and `space remove` is
/// what the close cross calls - which is `UnluminousApp::run_cli`'s own rule, that a thing done by
/// hand and the same thing done by an agent are the same thing rather than two paths that agree
/// today.
///
/// **`--from` is what a connection is for.** A command carrying it is acting *as* that node and is
/// refused when there is no wire from it to the node it names; a command with no `--from` is the
/// window's own agent, which may drive everything. That is the ticket's *"the main agent for the IDE
/// can control every single node"* beside its *"terminal node agents control the things they are
/// connected to"*.
impl UnluminousApp {
    pub(crate) fn cli_space(
        &mut self,
        request: &Request,
        verb: &str,
        ctx: &egui::Context,
    ) -> Outcome {
        match verb {
            "show" => {
                self.show_a_panel(dock::Panel::Space, true);
                self.take_the_keyboard_for_the_space();
                done(request, "The Base of Infinite Space is showing.")
            }
            "hide" => {
                self.show_a_panel(dock::Panel::Space, false);
                done(request, "Put the canvas away.")
            }
            "here" => self.cli_space_here(request),
            "view" => ok(request, "The canvas.", self.space.space.as_json()),
            "list" => self.cli_space_list(request),
            "manage" => {
                self.run_a_space_action(SpaceAction::Manage);
                done(request, "The space manager is open.")
            }
            "views" => self.cli_space_views(request),
            "open-view" => match self.a_named_view(request) {
                Ok(id) => {
                    self.space.space.show_view(id);
                    self.bring_the_current_view_to_life();
                    done(request, format!("Showing {}.", self.space.space.current().name))
                }
                Err(outcome) => outcome,
            },
            "new-view" => {
                let name = request.text("name").unwrap_or_else(|| "View".to_owned());
                let id = self.space.space.add_view(&name);
                self.space.space.show_view(id);
                ok(
                    request,
                    format!("Made {}.", self.space.space.current().name),
                    json!({ "view": id, "name": self.space.space.current().name }),
                )
            }
            "rename-view" => {
                let id = match self.a_named_view(request) {
                    Ok(id) => id,
                    Err(outcome) => return outcome,
                };
                let Some(name) = request.text("name") else {
                    return no(request, code::USAGE, "Say what to call it.");
                };
                self.space.space.rename_view(id, &name);
                let now = self.space.space.view(id).map(|view| view.name.clone()).unwrap_or_default();
                ok(request, format!("Called it {now}."), json!({ "view": id, "name": now }))
            }
            "duplicate-view" => {
                let id = match self.a_named_view(request) {
                    Ok(id) => id,
                    Err(outcome) => return outcome,
                };
                match self.space.space.duplicate_view(id) {
                    Some(copy) => {
                        self.space.space.show_view(copy);
                        self.bring_the_current_view_to_life();
                        ok(
                            request,
                            format!("Copied it to {}.", self.space.space.current().name),
                            json!({ "view": copy }),
                        )
                    }
                    None => no(request, code::NOT_FOUND, "There is no such view."),
                }
            }
            "delete-view" => {
                let id = match self.a_named_view(request) {
                    Ok(id) => id,
                    Err(outcome) => return outcome,
                };
                if self.space.space.views().len() < 2 {
                    return no(
                        request,
                        code::REFUSED,
                        "A canvas always has one view, so the last one cannot be deleted.",
                    );
                }
                self.delete_a_space_view(id);
                done(request, "Deleted it.")
            }
            "add" => self.cli_space_add(request),
            "move" => self.cli_space_move(request),
            "size" => self.cli_space_size(request),
            "title" => {
                let node = match self.a_named_node(request, "node") {
                    Ok(node) => node,
                    Err(outcome) => return outcome,
                };
                let title = request.text("title").unwrap_or_default();
                self.space.space.title_node(node, title.trim());
                done(request, format!("Called node {node} {title}."))
            }
            "remove" => {
                let node = match self.a_named_node(request, "node") {
                    Ok(node) => node,
                    Err(outcome) => return outcome,
                };
                self.close_a_space_node(node);
                done(request, format!("Took node {node} off the canvas."))
            }
            "focus" => {
                let node = match self.a_named_node(request, "node") {
                    Ok(node) => node,
                    Err(outcome) => return outcome,
                };
                self.show_a_panel(dock::Panel::Space, true);
                self.space.space.choose(Some(node));
                self.space.space.raise(node);
                self.take_the_keyboard_for_the_space();
                done(request, format!("Node {node} has the keyboard."))
            }
            "connect" => self.cli_space_connect(request),
            "disconnect" => {
                let Some(edge) = request.number("connection").map(|id| id as u64) else {
                    return no(request, code::USAGE, "Say which connection, by its id.");
                };
                match self.space.space.disconnect(edge) {
                    true => done(request, format!("Took connection {edge} away.")),
                    false => no(request, code::NOT_FOUND, format!("There is no connection {edge}.")),
                }
            }
            "connections" => self.cli_space_connections(request),
            "camera" => self.cli_space_camera(request),
            "send" => self.cli_space_send(request),
            "restart" => {
                let node = match self.a_named_node(request, "node") {
                    Ok(node) => node,
                    Err(outcome) => return outcome,
                };
                match self.start_a_space_terminal(node, request.switch("resume")) {
                    Ok(()) => done(request, format!("Started node {node} again.")),
                    Err(problem) => no(request, code::FAILED, problem),
                }
            }
            "font" => self.cli_space_font(request),
            "zoom" => self.cli_space_zoom(request),
            "address" => {
                let node = match self.a_reachable_node(request, "node", Kind::Browser) {
                    Ok(node) => node,
                    Err(outcome) => return outcome,
                };
                let Some(url) = request.text("url") else {
                    return no(request, code::USAGE, "Say which address.");
                };
                // Through the same function the field's own Enter reaches, which is `run_cli`'s rule.
                match self.send_a_space_browser_to(node, url.trim()) {
                    Ok(()) => {
                        self.space.space.change(node, |state| {
                            if let State::Browser(browser) = state {
                                browser.typed = url.trim().to_owned();
                            }
                        });
                        done(request, format!("Node {node} is going to {}.", url.trim()))
                    }
                    Err(problem) => no(request, code::FAILED, problem),
                }
            }
            "browser" => self.cli_space_browser(request, ctx),
            "folder" => self.cli_space_folder(request),
            "editor" => {
                let node = match self.a_reachable_node(request, "node", Kind::Editor) {
                    Ok(node) => node,
                    Err(outcome) => return outcome,
                };
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which file.");
                };
                match self.open_in_a_space_node(node, &path) {
                    Ok(()) => ok(
                        request,
                        format!("Opened {} in node {node}.", path.display()),
                        json!({ "node": node, "path": path.to_string_lossy() }),
                    ),
                    Err(problem) => no(request, code::FAILED, problem),
                }
            }
            _ => unknown(request),
        }
    }

    /// Which node this command came from, and what it may act on.
    ///
    /// **The one command whose answer depends on which process is asking**, which is why it is a command
    /// rather than a paragraph: `UNLUMINOUS_SPACE_NODE` is in the *client's* environment, so the client
    /// reads it and sends it, and the window answers about the node it names.
    ///
    /// `task-1905` is the report — an agent in a terminal node spent nine tool calls and two shell
    /// commands working out what one call could have told it, and then hedged its answer because it had
    /// never established what it was allowed to do. What makes this worth having is not the eight calls
    /// saved: it is that **the answer names the command for each thing it lists**, which `task-1695`
    /// measured as what decides whether a command is used at all rather than `bash`.
    ///
    /// **Outside a node it is an answer rather than a refusal.** The window's own agent runs it too, and a
    /// refusal there would be a refusal about nothing — which is `picture::from_the_clipboard`'s rule.
    fn cli_space_here(&self, request: &Request) -> Outcome {
        let view = self.space.space.current();
        // The client puts what its own environment said here; the window has no way to know.
        let asking = request.number("node").map(|id| id as u64);
        let Some(asking) = asking else {
            return lines(
                request,
                "Not running inside a node.",
                vec![
                    "This command is not running inside a node on the canvas, so it is the window's own"
                        .to_owned(),
                    "agent: every node is reachable and no --from is needed. `space list` is the nodes."
                        .to_owned(),
                ],
                json!({ "node": Value::Null, "inANode": false, "reaches": [], "reachedBy": [] }),
            );
        };
        let Some(found) = view.node(asking) else {
            return no(
                request,
                code::NOT_FOUND,
                format!(
                    "UNLUMINOUS_SPACE_NODE says node {asking}, and there is no such node on {}. The canvas may have moved to another view.",
                    view.name
                ),
            );
        };
        let describe = |id: NodeId| -> Option<Value> {
            let other = view.node(id)?;
            Some(json!({
                "node": id,
                "kind": other.kind().name(),
                "title": self.name_of_a_node(other),
                "command": drives(other.kind(), id, asking),
            }))
        };
        let reaches: Vec<Value> = view.reaches(asking).into_iter().filter_map(describe).collect();
        let reached_by: Vec<Value> =
            view.reached_by(asking).into_iter().filter_map(describe).collect();
        let mut rows = vec![format!(
            "node {asking}  {}  \"{}\"  on view {}",
            found.kind().name(),
            self.name_of_a_node(found),
            view.name,
        )];
        rows.push("wired to:".to_owned());
        match reaches.is_empty() {
            true => rows.push("  nothing".to_owned()),
            false => {
                for one in &reaches {
                    rows.push(format!(
                        "  {:<4} {:<9} {:<22} {}",
                        one["node"],
                        one["kind"].as_str().unwrap_or_default(),
                        one["title"].as_str().unwrap_or_default(),
                        one["command"].as_str().unwrap_or_default(),
                    ));
                }
            }
        }
        rows.push("wired from:".to_owned());
        match reached_by.is_empty() {
            true => rows.push("  nothing".to_owned()),
            false => {
                for one in &reached_by {
                    rows.push(format!(
                        "  {:<4} {:<9} {}",
                        one["node"],
                        one["kind"].as_str().unwrap_or_default(),
                        one["title"].as_str().unwrap_or_default(),
                    ));
                }
            }
        }
        lines(
            request,
            format!("node {asking} on {}", view.name),
            rows,
            json!({
                "node": asking,
                "inANode": true,
                "kind": found.kind().name(),
                "title": self.name_of_a_node(found),
                "view": view.name,
                "reaches": reaches,
                "reachedBy": reached_by,
            }),
        )
    }

    /// The nodes on the view that is showing, one a line.
    fn cli_space_list(&self, request: &Request) -> Outcome {
        let view = self.space.space.current();
        let rows: Vec<String> = view
            .nodes
            .iter()
            .map(|node| {
                let wired = view.reaches(node.id);
                format!(
                    "{}{:<4} {:<9} {:>6},{:<6} {:>4} x {:<4}  {}{}",
                    if view.chosen == Some(node.id) { "*" } else { " " },
                    node.id,
                    node.kind().name(),
                    node.at.x.round(),
                    node.at.y.round(),
                    node.size.x.round(),
                    node.size.y.round(),
                    self.name_of_a_node(node),
                    match wired.is_empty() {
                        true => String::new(),
                        false => format!(
                            "  -> {}",
                            wired.iter().map(u64::to_string).collect::<Vec<_>>().join(", ")
                        ),
                    },
                )
            })
            .collect();
        let message = format!("{} node{} on {}", rows.len(), if rows.len() == 1 { "" } else { "s" }, view.name);
        lines(request, message, rows, self.space.space.as_json())
    }

    /// Every view the canvas has.
    fn cli_space_views(&self, request: &Request) -> Outcome {
        let current = self.space.space.current_id();
        let rows: Vec<String> = self
            .space
            .space
            .views()
            .iter()
            .map(|view| {
                format!(
                    "{}{:<4} {:<24} {} node{}",
                    if view.id == current { "*" } else { " " },
                    view.id,
                    view.name,
                    view.nodes.len(),
                    if view.nodes.len() == 1 { "" } else { "s" },
                )
            })
            .collect();
        let views: Vec<Value> = self
            .space
            .space
            .views()
            .iter()
            .map(|view| {
                json!({
                    "view": view.id,
                    "name": view.name,
                    "nodes": view.nodes.len(),
                    "connections": view.edges.len(),
                    "showing": view.id == current,
                })
            })
            .collect();
        lines(request, format!("{} views", views.len()), rows, json!({ "views": views }))
    }

    /// Put a node on the canvas, and give it whatever its kind was told to hold.
    fn cli_space_add(&mut self, request: &Request) -> Outcome {
        let Some(name) = request.text("kind") else {
            return no(request, code::USAGE, "Say which kind of node.");
        };
        let Some(kind) = Kind::from_name(name.trim()) else {
            let names: Vec<&str> = Kind::ALL.into_iter().map(Kind::name).collect();
            return no(
                request,
                code::USAGE,
                format!("There is no node called {name}. Unluminous has {}.", names.join(", ")),
            );
        };
        self.show_a_panel(dock::Panel::Space, true);
        let middle = self.middle_of_the_canvas();
        let at = Pos2::new(
            request.number("x").map(|x| x as f32).unwrap_or(middle.x),
            request.number("y").map(|y| y as f32).unwrap_or(middle.y),
        );
        let node = self.add_a_space_node(kind, at);
        if let (Some(width), Some(height)) = (request.number("width"), request.number("height")) {
            self.space.space.resize_node(node, Vec2::new(width as f32, height as f32));
        }
        if let Some(title) = request.text("title") {
            self.space.space.title_node(node, title.trim());
        }
        // Whatever the kind was told to hold, applied through the same functions the window uses.
        let mut problem: Option<String> = None;
        if let Some(command) = request.text("command") {
            self.space.space.change(node, |state| {
                if let State::Terminal(terminal) = state {
                    terminal.command = command.trim().to_owned();
                }
            });
            if let Err(refusal) = self.start_a_space_terminal(node, false) {
                problem = Some(refusal);
            }
        }
        if let Some(url) = request.text("url") {
            if let Err(refusal) = self.open_a_space_browser(node, url.trim()) {
                problem = Some(refusal);
            }
        }
        if let Some(root) = self.cli_path_argument(request, "root") {
            self.space.space.change(node, |state| {
                if let State::Folder(folder) = state {
                    folder.root = Some(root.clone());
                }
            });
            self.space.live.forget(node);
        }
        if let Some(path) = self.cli_path_argument(request, "path") {
            if let Err(refusal) = self.open_in_a_space_node(node, &path) {
                problem = Some(refusal);
            }
        }
        // **The node is still there when part of what it was given failed**, and the reply says so:
        // a refusal that also took the node away would leave a caller with nothing to correct.
        let made = self.space.space.current().node(node).cloned();
        let where_it_is = made.as_ref().map(|node| node.at).unwrap_or(at);
        let message = match &problem {
            Some(refusal) => format!("Added node {node}, but {refusal}"),
            None => format!("Added {} node {node}.", kind.name()),
        };
        ok(
            request,
            message,
            json!({
                "node": node,
                "kind": kind.name(),
                "x": where_it_is.x,
                "y": where_it_is.y,
                "problem": problem,
            }),
        )
    }

    fn cli_space_move(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return outcome,
        };
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        let at = Pos2::new(
            request.number("x").map(|x| x as f32).unwrap_or(found.at.x),
            request.number("y").map(|y| y as f32).unwrap_or(found.at.y),
        );
        self.space.space.move_node(node, at);
        ok(request, format!("Moved node {node}."), json!({ "node": node, "x": at.x, "y": at.y }))
    }

    fn cli_space_size(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return outcome,
        };
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        let size = Vec2::new(
            request.number("width").map(|width| width as f32).unwrap_or(found.size.x),
            request.number("height").map(|height| height as f32).unwrap_or(found.size.y),
        );
        self.space.space.resize_node(node, size);
        // What it really came out, because a kind has a smallest size and a caller that asked for
        // less deserves to be told what it got rather than that it worked.
        let now = self.space.space.current().node(node).map(|node| node.size).unwrap_or(size);
        ok(
            request,
            format!("Node {node} is {} x {}.", now.x.round(), now.y.round()),
            json!({ "node": node, "width": now.x, "height": now.y }),
        )
    }

    fn cli_space_connect(&mut self, request: &Request) -> Outcome {
        let from = match self.a_named_node(request, "from") {
            Ok(node) => node,
            Err(outcome) => return outcome,
        };
        let to = match self.a_named_node(request, "to") {
            Ok(node) => node,
            Err(outcome) => return outcome,
        };
        let pipe = match request.text("pipe").as_deref().map(str::trim) {
            None => Pipe::Off,
            Some(name) => match Pipe::from_name(name) {
                Some(pipe) => pipe,
                None => {
                    return no(
                        request,
                        code::USAGE,
                        format!("A connection carries `lines` or `off`, not {name}."),
                    )
                }
            },
        };
        match self.space.space.connect(from, to, pipe) {
            Ok(edge) => {
                if pipe == Pipe::Lines {
                    self.space.live.follow_from_here(from);
                }
                ok(
                    request,
                    format!("Connected {from} to {to}."),
                    json!({ "connection": edge, "from": from, "to": to, "pipe": pipe.name() }),
                )
            }
            Err(problem) => no(request, code::REFUSED, problem),
        }
    }

    fn cli_space_connections(&self, request: &Request) -> Outcome {
        let only = request.number("from").map(|id| id as u64);
        let view = self.space.space.current();
        let found: Vec<&crate::services::space::Edge> = view
            .edges
            .iter()
            .filter(|edge| only.is_none_or(|from| edge.from == from))
            .collect();
        let rows: Vec<String> = found
            .iter()
            .map(|edge| {
                format!("{:<4} {:>4} -> {:<4}  {}", edge.id, edge.from, edge.to, edge.pipe.name())
            })
            .collect();
        let values: Vec<Value> = found
            .iter()
            .map(|edge| {
                json!({
                    "connection": edge.id,
                    "from": edge.from,
                    "to": edge.to,
                    "pipe": edge.pipe.name(),
                })
            })
            .collect();
        lines(
            request,
            format!("{} connection{}", values.len(), if values.len() == 1 { "" } else { "s" }),
            rows,
            json!({ "connections": values }),
        )
    }

    fn cli_space_camera(&mut self, request: &Request) -> Outcome {
        let body = self.space.body;
        if request.switch("fit") {
            let bounds = self.space.space.current().bounds();
            self.space.space.current_mut().camera.fit(bounds, body.size(), 32.0);
        }
        let camera = self.space.space.current().camera;
        let at = Pos2::new(
            request.number("x").map(|x| x as f32).unwrap_or(camera.at.x),
            request.number("y").map(|y| y as f32).unwrap_or(camera.at.y),
        );
        let zoom = request.number("zoom").map(|zoom| zoom as f32).unwrap_or(camera.zoom);
        {
            let camera = &mut self.space.space.current_mut().camera;
            camera.at = at;
            // Through `zoom_to` rather than by assignment, so the ladder's ends are kept in one place
            // and a caller that asked for ten gets 2.5 rather than a canvas nobody can read.
            camera.zoom_to(zoom, body.min, body.min);
            camera.at = at;
        }
        self.space.space.touch();
        let camera = self.space.space.current().camera;
        ok(
            request,
            format!("The canvas is at {:.0},{:.0} at {:.2}x.", camera.at.x, camera.at.y, camera.zoom),
            json!({ "x": camera.at.x, "y": camera.at.y, "zoom": camera.zoom }),
        )
    }

    fn cli_space_send(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Terminal) {
            Ok(node) => node,
            Err(outcome) => return outcome,
        };
        let Some(text) = request.text("text") else {
            return no(request, code::USAGE, "Say what to type.");
        };
        if !self.space.live.has_a_terminal(node) {
            return no(request, code::REFUSED, format!("Node {node} has no terminal running."));
        }
        // Remembered as something typed in, so its echo is not piped back out - the rule in
        // `services::space::pipe`, which a send by hand needs exactly as much as a pipe does.
        self.space.live.typed_into(node, text.trim_end());
        if let Some(session) = self.space.live.terminal(node) {
            session.send(format!("{}\r", text.trim_end()).into_bytes());
        }
        done(request, format!("Typed into node {node}."))
    }

    /// How big a terminal node's letters are.
    ///
    /// Through [`UnluminousApp::step_a_node_font`] and the node's own state, which is what the two
    /// buttons on its header press - one path, so a size set from the command line and one set by
    /// hand are the same size.
    fn cli_space_font(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Terminal) {
            Ok(node) => node,
            Err(outcome) => return outcome,
        };
        if request.switch("reset") {
            self.space.space.change(node, |state| {
                if let State::Terminal(terminal) = state {
                    terminal.font_size = 0.0;
                }
            });
        } else if let Some(size) = request.number("size") {
            let wanted = size as f32;
            // A range rather than the list `TERMINAL_FONT_SIZES` offers, because that list is what
            // the **buttons** walk and not what the setting allows: `terminal.font.size` is a number
            // somebody types, and this machine's is 34. A node that could not be given the size its
            // own terminal is set in would be a node that refused the only size that was wanted.
            if !(6.0..=96.0).contains(&wanted) {
                return no(request, code::USAGE, "A terminal is set in 6 to 96 point.");
            }
            self.space.space.change(node, |state| {
                if let State::Terminal(terminal) = state {
                    terminal.font_size = wanted;
                }
            });
        } else if request.switch("bigger") {
            self.step_a_node_font(node, 1);
        } else if request.switch("smaller") {
            self.step_a_node_font(node, -1);
        }
        let found = self.space.space.current().node(node).cloned();
        let size = found
            .as_ref()
            .map(|node| space_view::font_size_of(node, self.settings.terminal_font_size))
            .unwrap_or_default();
        let own = matches!(&found.map(|node| node.state), Some(State::Terminal(terminal)) if terminal.font_size > 0.0);
        ok(
            request,
            format!("Node {node} is set in {size:.0} point."),
            json!({ "node": node, "size": size, "itsOwn": own }),
        )
    }

    /// How big one node draws what it holds.
    ///
    /// Through [`UnluminousApp::zoom_a_node`], which is what the modifier wheel over a node presses — one
    /// path, so a size set from the command line and one set by hand are the same size.
    fn cli_space_zoom(&mut self, request: &Request) -> Outcome {
        let node = match self.a_named_node(request, "node") {
            Ok(node) => node,
            Err(outcome) => return outcome,
        };
        if let Some(from) = request.number("from").map(|id| id as u64) {
            if !self.space.space.may_reach(from, node) {
                return no(
                    request,
                    code::REFUSED,
                    format!("Node {from} is not connected to node {node}."),
                );
            }
        }
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        if request.switch("reset") {
            self.space.space.change(node, |state| match state {
                State::Terminal(terminal) => terminal.font_size = 0.0,
                State::Editor(editor) => editor.font_size = 0.0,
                State::Folder(folder) => folder.zoom = 1.0,
                State::Browser(_) => {}
            });
            if found.kind() == Kind::Browser {
                self.space.live.set_page_zoom(node, 1.0);
                if let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) {
                    let _ = self.browser.zoom(tab, 1.0);
                }
            }
        } else if let Some(asked) = request.number("factor") {
            let asked = asked as f32;
            if asked <= 0.0 {
                return no(request, code::USAGE, "A zoom is a number above zero.");
            }
            match found.kind() {
                Kind::Terminal | Kind::Editor => {
                    if !(6.0..=96.0).contains(&asked) {
                        return no(request, code::USAGE, "A point size is 6 to 96.");
                    }
                    self.space.space.change(node, |state| match state {
                        State::Terminal(terminal) => terminal.font_size = asked,
                        State::Editor(editor) => editor.font_size = asked,
                        _ => {}
                    });
                    if let Some(index) = self.files.tab_in_node(node) {
                        self.files.at_mut(index).cached.stale = true;
                    }
                }
                Kind::Folder => {
                    if !(0.25..=4.0).contains(&asked) {
                        return no(request, code::USAGE, "A folder node's zoom is 0.25 to 4.");
                    }
                    self.space.space.change(node, |state| {
                        if let State::Folder(folder) = state {
                            folder.zoom = asked;
                        }
                    });
                }
                Kind::Browser => {
                    if !(0.25..=4.0).contains(&asked) {
                        return no(request, code::USAGE, "A page's zoom is 0.25 to 4.");
                    }
                    self.space.live.set_page_zoom(node, asked);
                    if let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) {
                        if let Err(problem) = self.browser.zoom(tab, f64::from(asked)) {
                            self.message = Some(problem);
                        }
                    }
                }
            }
        } else if request.switch("bigger") {
            self.zoom_a_node(node, 1);
        } else if request.switch("smaller") {
            self.zoom_a_node(node, -1);
        }
        let found = self.space.space.current().node(node).cloned();
        let (factor, own) = match found.as_ref().map(|node| &node.state) {
            Some(State::Terminal(terminal)) => (
                space_view::font_size_of(found.as_ref().expect("it is there"), self.settings.terminal_font_size),
                terminal.font_size > 0.0,
            ),
            Some(State::Editor(editor)) => (
                space_view::editor_font_size_of(found.as_ref().expect("it is there"), self.settings.font_size),
                editor.font_size > 0.0,
            ),
            Some(State::Folder(folder)) => (folder.zoom, (folder.zoom - 1.0).abs() > 0.001),
            Some(State::Browser(_)) => {
                let zoom = self.space.live.page_zoom_of(node);
                (zoom, (zoom - 1.0).abs() > 0.001)
            }
            None => (1.0, false),
        };
        let walks = match found.as_ref().map(|node| node.kind()) {
            Some(Kind::Terminal) => "terminal.font.size",
            Some(Kind::Editor) => "appearance.font.size",
            Some(Kind::Folder) => "a multiplier over its rows",
            Some(Kind::Browser) => "the page's own zoom",
            None => "",
        };
        ok(
            request,
            format!("Node {node} is drawn at {factor:.2}."),
            json!({ "node": node, "factor": factor, "itsOwn": own, "walks": walks }),
        )
    }

    fn cli_space_browser(&mut self, request: &Request, ctx: &egui::Context) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Browser) {
            Ok(node) => node,
            Err(outcome) => return outcome,
        };
        let Some(command) = request.text("command") else {
            return no(request, code::USAGE, "Say what to do: go, back, forward, reload, url or shot.");
        };
        match command.trim() {
            "go" => {
                let Some(url) = request.text("url") else {
                    return no(request, code::USAGE, "Say where to go, with --url.");
                };
                match self.send_a_space_browser_to(node, url.trim()) {
                    Ok(()) => done(request, format!("Node {node} is going to {url}.")),
                    Err(problem) => no(request, code::FAILED, problem),
                }
            }
            step @ ("back" | "forward") => {
                let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) else {
                    return no(request, code::REFUSED, format!("Node {node} has no page open."));
                };
                let command = match step {
                    "back" => crate::services::browser::BrowserCommand::Back,
                    _ => crate::services::browser::BrowserCommand::Forward,
                };
                self.run_browser_command(tab, command);
                done(request, format!("Node {node} went {step}."))
            }
            "reload" => {
                let Some(tab) = self.space.live.browser(node).map(|tab| tab.id) else {
                    return no(request, code::REFUSED, format!("Node {node} has no page open."));
                };
                self.run_browser_command(tab, crate::services::browser::BrowserCommand::Reload);
                done(request, format!("Node {node} is reloading."))
            }
            "url" => match self.space.live.browser(node) {
                Some(tab) => ok(
                    request,
                    tab.current_url().to_owned(),
                    json!({ "node": node, "url": tab.current_url(), "loading": tab.loading }),
                ),
                None => no(request, code::REFUSED, format!("Node {node} has no page open.")),
            },
            "shot" => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say where to write the picture, with --path.");
                };
                let Some(found) = self.space.space.current().node(node).cloned() else {
                    return no(request, code::NOT_FOUND, format!("There is no node {node}."));
                };
                // The node has to be **showing** to be photographed, because a picture is of the
                // window as the operating system composited it and a native child view is part of
                // that rather than something Unluminous can render on its own.
                self.show_a_panel(dock::Panel::Space, true);
                self.space.space.choose(Some(node));
                self.space.space.raise(node);
                let camera = self.space.space.current().camera;
                let area = camera.rect_to_screen(self.space.body.min, found.rect());
                ctx.request_repaint();
                Outcome::Hold(crate::app::cli::Waiting::Screenshot {
                    path,
                    until: std::time::Instant::now() + std::time::Duration::from_secs(10),
                    settled: std::time::Instant::now() + std::time::Duration::from_millis(250),
                    asked: false,
                    crop: Some(area.intersect(self.space.body)),
                })
            }
            other => no(
                request,
                code::USAGE,
                format!("A browser node does go, back, forward, reload, url or shot, not {other}."),
            ),
        }
    }

    fn cli_space_folder(&mut self, request: &Request) -> Outcome {
        let node = match self.a_reachable_node(request, "node", Kind::Folder) {
            Ok(node) => node,
            Err(outcome) => return outcome,
        };
        let Some(command) = request.text("command") else {
            return no(request, code::USAGE, "Say what to do: expand, collapse, select, open, root or rows.");
        };
        let Some(found) = self.space.space.current().node(node).cloned() else {
            return no(request, code::NOT_FOUND, format!("There is no node {node}."));
        };
        self.make_sure_a_node_has_a_tree(&found);
        match command.trim() {
            open @ ("expand" | "collapse") => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which folder, with --path.");
                };
                let wanted = open == "expand";
                let Some(tree) = self.space.live.tree_mut(node) else {
                    return no(request, code::FAILED, "That node has no tree.");
                };
                let showing = tree.find(&path).map(|entry| entry.expanded).unwrap_or(false);
                if showing != wanted {
                    tree.toggle(&path);
                }
                self.remember_a_folder_nodes_open_folders(node);
                done(request, format!("{} {} in node {node}.", if wanted { "Opened" } else { "Shut" }, path.display()))
            }
            "select" => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which row, with --path.");
                };
                self.space.live.select_in_tree(node, Some(path.clone()));
                done(request, format!("Node {node} is on {}.", path.display()))
            }
            "open" => {
                let Some(path) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which file, with --path.");
                };
                // **The same rule a double click keeps**: into a wired File Editor node when there is one,
                // and into the editing area when there is not. One function, so the pointer and the agent
                // cannot come to different answers. `task-1905`.
                let wired = self.space.space.current().reaches(node).into_iter().find(|other| {
                    self.space
                        .space
                        .current()
                        .node(*other)
                        .is_some_and(|found| found.kind() == Kind::Editor)
                });
                match wired {
                    Some(editor) => match self.open_in_a_space_node(editor, &path) {
                        Ok(()) => ok(
                            request,
                            format!("Opened {} in node {editor}.", path.display()),
                            json!({ "node": editor, "path": path.to_string_lossy() }),
                        ),
                        Err(problem) => no(request, code::FAILED, problem),
                    },
                    None => match self.open_path_permanently(&path) {
                        Ok(()) => ok(
                            request,
                            format!("Opened {} in the editing area.", path.display()),
                            json!({ "node": Value::Null, "path": path.to_string_lossy() }),
                        ),
                        Err(problem) => no(request, code::FAILED, problem),
                    },
                }
            }
            "root" => {
                let Some(root) = self.cli_path_argument(request, "path") else {
                    return no(request, code::USAGE, "Say which folder, with --path.");
                };
                // Through the one function the `Choose Folder...` dialog calls, so a folder chosen by
                // hand and one named here are the same change.
                self.point_a_folder_node_at(node, &root);
                done(request, format!("Node {node} is showing {}.", root.display()))
            }
            "rows" => {
                let Some(tree) = self.space.live.tree(node) else {
                    return no(request, code::FAILED, "That node has no tree.");
                };
                let rows: Vec<String> = tree
                    .rows()
                    .iter()
                    .map(|row| {
                        format!(
                            "{}{}{}",
                            "  ".repeat(row.depth),
                            row.entry.path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default(),
                            if row.entry.is_directory { "/" } else { "" },
                        )
                    })
                    .collect();
                let paths: Vec<Value> = tree
                    .rows()
                    .iter()
                    .map(|row| {
                        json!({
                            "path": row.entry.path.to_string_lossy(),
                            "directory": row.entry.is_directory,
                            "expanded": row.entry.expanded,
                            "depth": row.depth,
                        })
                    })
                    .collect();
                lines(request, format!("{} rows", paths.len()), rows, json!({ "rows": paths }))
            }
            other => no(
                request,
                code::USAGE,
                format!("A folder node does expand, collapse, select, open, root or rows, not {other}."),
            ),
        }
    }

    // ------------------------------------------------------------------------------- naming things

    /// The view a command named, by its name or by its id.
    fn a_named_view(&self, request: &Request) -> Result<u64, Outcome> {
        let Some(name) = request.text("view") else {
            return Err(no(request, code::USAGE, "Say which view, by its name or its id."));
        };
        self.space.space.view_named(name.trim()).ok_or_else(|| {
            let names: Vec<&str> =
                self.space.space.views().iter().map(|view| view.name.as_str()).collect();
            no(
                request,
                code::NOT_FOUND,
                format!("There is no view called {name}. This canvas has {}.", names.join(", ")),
            )
        })
    }

    /// The node a command named, by its id.
    fn a_named_node(&self, request: &Request, argument: &str) -> Result<NodeId, Outcome> {
        let Some(id) = request.number(argument).map(|id| id as u64) else {
            return Err(no(
                request,
                code::USAGE,
                format!("Say which node, with `{argument}` and an id from `space list`."),
            ));
        };
        match self.space.space.current().node(id).is_some() {
            true => Ok(id),
            false => Err(no(
                request,
                code::NOT_FOUND,
                format!("There is no node {id} on {}.", self.space.space.current().name),
            )),
        }
    }

    /// The node a command named, checked against what the node asking is wired to.
    ///
    /// **This is the whole of the permission model.** A command carrying `--from` is acting as that
    /// node; one without it is the window's own agent. A node that is not wired to its target is
    /// refused with what it *is* wired to, so an agent that guessed is told what it may reach rather
    /// than left to guess again.
    fn a_reachable_node(
        &self,
        request: &Request,
        argument: &str,
        wanted: Kind,
    ) -> Result<NodeId, Outcome> {
        let node = self.a_named_node(request, argument)?;
        let found = self
            .space
            .space
            .current()
            .node(node)
            .ok_or_else(|| no(request, code::NOT_FOUND, format!("There is no node {node}.")))?;
        if found.kind() != wanted {
            return Err(no(
                request,
                code::REFUSED,
                format!("Node {node} is a {} node, not a {} one.", found.kind().name(), wanted.name()),
            ));
        }
        let Some(from) = request.number("from").map(|id| id as u64) else {
            return Ok(node);
        };
        if self.space.space.current().node(from).is_none() {
            return Err(no(request, code::NOT_FOUND, format!("There is no node {from}.")));
        }
        if self.space.space.may_reach(from, node) {
            return Ok(node);
        }
        let reaches = self.space.space.current().reaches(from);
        Err(no(
            request,
            code::REFUSED,
            match reaches.is_empty() {
                true => format!("Node {from} is not connected to anything."),
                false => format!(
                    "Node {from} is not connected to node {node}. It is connected to {}.",
                    reaches.iter().map(u64::to_string).collect::<Vec<_>>().join(", ")
                ),
            },
        ))
    }
}

/// The command that drives a node of `kind`, written out with the ids filled in.
///
/// **Naming the command is the point of `space here`.** `task-1695` measured a model handed a node id and
/// left to work out which of twenty-three `space` verbs applies to a browser: it reached for `bash`. One
/// handed the command line writes the command line. So this is a worked example rather than a verb name,
/// and `--from` is in it, because a command from a node that leaves it out is a command that reaches
/// everything and teaches the wrong habit.
fn drives(kind: Kind, node: NodeId, from: NodeId) -> String {
    match kind {
        Kind::Terminal => format!("space send {node} <text> --from {from}"),
        Kind::Browser => format!("space browser {node} go --url <address> --from {from}"),
        Kind::Folder => format!("space folder {node} rows --from {from}"),
        Kind::Editor => format!("space editor {node} <path> --from {from}"),
    }
}

/// Where a tab that lives on a node is put when its node has gone.
///
/// Nothing calls this today: [`UnluminousApp::close_a_space_node`] closes the tab, which writes it
/// first. It is here as the answer a later change would otherwise have to invent — a tab left with a
/// `Home::Node` naming a node that is not there.
pub fn rehome(file: &mut OpenFile) {
    if file.home.node().is_some() {
        file.home = Home::Pane(0);
    }
}
