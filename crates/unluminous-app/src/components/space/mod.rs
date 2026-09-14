//! Drawing the Base of Infinite Space.
//!
//! `tasks/task-1904-base-of-infinite-space-tdd.md` is the design and `services::space` is the model.
//! Nothing here changes anything: each function takes a rectangle, draws, and reports what the person
//! did in it, which is `components/`'s own rule — the state changes in `app`, so two parts of the
//! canvas cannot disagree about what happened.
//!
//! ## Two coordinate systems, and which is which
//!
//! A node's **contents** are drawn in world points, into an `egui` layer of the node's own carrying a
//! `TSTransform` with the camera in it. That is what makes a zoom cost a matrix rather than a relayout:
//! a terminal keeps its cell count and an editor keeps its line breaks while the canvas is scaled. It
//! is also why a drag inside a node comes back in world points already — `Response::drag_delta`
//! divides by the layer's scale.
//!
//! A node's **decoration** is recorded in screen points, into the pane's own `Chrome`, because that
//! canvas is rasterised once over the pane and painted underneath every node layer. The Gaussians are
//! therefore drawn at the size they are seen at rather than being scaled up with everything else.
//!
//! The cost of the transform is stated rather than hidden: a layer's mesh is tessellated at its own
//! scale and then scaled, so text at a zoom other than 1.0 is scaled pixels rather than re-rasterised
//! glyphs. At 1.0, which is the default and where somebody reads code, it is exact.

pub mod add_modal;
pub mod manager;

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Vec2};

use crate::services::space::geometry::{self, Grip};
use crate::services::space::{Camera, Kind, Node, Pipe, View, ViewId};
use crate::services::vello_canvas::{Chrome, Fill, Lift};
use crate::theme::crisp::CrispPainter;
use crate::theme::{color, icon};

/// The strip along the top of the pane that holds the views.
pub const VIEW_BAR: f32 = 30.0;
/// A node's own header, in world points.
pub const NODE_HEADER: f32 = 26.0;
/// How far a port sticks out of a node's edge, and how big it is.
pub const PORT: f32 = 6.0;
/// How near an edge the pointer has to be to take hold of it, in screen points.
pub const GRIP_REACH: f32 = 6.0;
/// The dot grid's spacing, in world points.
const GRID: f32 = 20.0;
/// Below this zoom the grid is not drawn at all: the dots would be closer together than they are wide
/// and the canvas would read as a flat grey wash rather than as a grid.
const GRID_FADES_AT: f32 = 0.5;
/// How many straight pieces a wire is drawn as. See `geometry::curve_points`.
const WIRE_SEGMENTS: usize = 28;
/// How near a wire a right click has to be, in screen points.
pub const WIRE_REACH: f32 = 7.0;

/// What the strip of views reported.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct BarOutcome {
    /// A view was chosen.
    pub show: Option<ViewId>,
    /// The plus was pressed.
    pub add: bool,
    /// A view was right clicked: where the pointer was, and which one.
    pub menu: Option<(Pos2, ViewId)>,
    /// The zoom buttons were pressed: -1 or 1, in notches.
    pub zoom: i32,
    /// The reading between them was pressed, which puts the zoom back to one.
    pub reset_zoom: bool,
    /// The row saying how many views are not listed was pressed, which opens the manager.
    pub manage: bool,
}

/// How much room the overflow row takes, when there is one.
const MORE_ROW: f32 = 68.0;
/// How much room the zoom controls take at the right hand end of the bar, before the plus.
///
/// The bar already kept 30 points clear for the plus, so this is what the chips now stop before instead.
const ZOOM_CONTROLS: f32 = 116.0;

/// The strip of views along the top of the canvas: one chip a view, the current one lit, and a plus.
///
/// The ticket asks to *"have multiple projects/views"* and to *"Edit project/view names, delete, create
/// new, clone/duplicate"*. The four of those that are about one view are on its right click menu, which
/// is where the explorer, the tabs and every panel in Unluminous already put the things that are about
/// one row.
pub fn view_bar(
    ui: &mut egui::Ui,
    area: Rect,
    views: &[View],
    current: ViewId,
    zoom: f32,
    look: Look<'_>,
) -> BarOutcome {
    let mut outcome = BarOutcome::default();
    let painter = ui.painter_at(area);
    painter.rect_filled(
        area,
        CornerRadius::ZERO,
        crate::theme::faded(color::toolbar(), look.opacity),
    );
    painter.line_segment(
        [Pos2::new(area.left(), area.bottom() - 0.5), Pos2::new(area.right(), area.bottom() - 0.5)],
        egui::Stroke::new(1.0, color::divider()),
    );
    let mut pen = area.left() + 8.0;
    let mut drawn = 0usize;
    for view in views {
        let width = chip_width(&painter, &view.name);
        let chip = Rect::from_min_size(
            Pos2::new(pen, area.top() + 4.0),
            Vec2::new(width, area.height() - 9.0),
        );
        // **Room kept for the overflow row as well**, because a bar that filled itself to the edge and then
        // said "3 more" off the end of itself would be the fault it is there to fix.
        if chip.right() > area.right() - 30.0 - ZOOM_CONTROLS - MORE_ROW {
            break;
        }
        drawn += 1;
        let on = view.id == current;
        let response = ui.interact(chip, ui.id().with(("space-view", view.id)), Sense::click());
        if on {
            look.chrome.raised(chip, 6.0, Fill::Solid(look.card), Lift::Small);
        }
        if !on && response.hovered() {
            painter.rect_filled(chip, CornerRadius::same(6), color::control());
        }
        let tint = if on { color::text_strong() } else { color::text_dim() };
        painter.crisp_text(
            chip.center(),
            Align2::CENTER_CENTER,
            &view.name,
            FontId::proportional(11.5),
            tint,
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                true,
                on,
                format!("View: {}", view.name),
            )
        });
        if response.clicked() {
            outcome.show = Some(view.id);
        }
        if response.secondary_clicked() {
            if let Some(at) = response.interact_pointer_pos().or_else(|| response.hover_pos()) {
                outcome.menu = Some((at, view.id));
            }
        }
        pen = chip.right() + 6.0;
    }
    let plus =
        Rect::from_center_size(Pos2::new(area.right() - 18.0, area.center().y), Vec2::splat(22.0));
    if crate::components::controls::icon_button(ui, plus, "New view", icon::plus) {
        outcome.add = true;
    }
    // **How many are not listed, and a way to see them.** `view_bar` has always broken out of its loop when
    // a chip would not fit, so past about six views the rest were not merely hard to reach — they were not
    // drawn at all and nothing said so. `task-1906`.
    let left_out = manager::not_showing(views.len(), drawn);
    if left_out > 0 {
        let row = Rect::from_min_size(
            Pos2::new(pen, area.top() + 4.0),
            Vec2::new(MORE_ROW - 6.0, area.height() - 9.0),
        );
        let response = ui.interact(row, ui.id().with("space-views-more"), Sense::click());
        if response.hovered() {
            ui.painter_at(area).rect_filled(row, CornerRadius::same(6), color::control());
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let said = format!("+{left_out} more");
        ui.painter_at(area).crisp_text(
            row.center(),
            Align2::CENTER_CENTER,
            &said,
            FontId::proportional(11.0),
            color::text_dim(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Spaces, {said}"))
        });
        if response.clicked() {
            outcome.manage = true;
        }
    }
    show_the_zoom_controls(ui, area, zoom, &mut outcome);
    outcome
}

/// The two zoom buttons and the reading between them, at the right hand end of the bar.
///
/// `task-1905` asks for *"classic - + buttons with cirlces around them"* at the top right, and the top
/// right of the canvas **is** the right hand end of this bar: a floating control over the ground would be
/// drawn under every node that happened to be there, because a node's layer is a sublayer composited
/// above the pane's.
///
/// Both buttons **dim at the ends of the ladder** rather than disappearing, because a zoom that cannot go
/// further is a control that will apply again the moment the other one is pressed. That is the dimmed half
/// of Unluminous's absent-control rule.
fn show_the_zoom_controls(ui: &mut egui::Ui, area: Rect, zoom: f32, outcome: &mut BarOutcome) {
    let middle = area.center().y;
    let mut right = area.right() - 34.0;
    let out = Rect::from_center_size(Pos2::new(right - 11.0, middle), Vec2::splat(22.0));
    // At the bottom of the ladder there is nowhere further out to go.
    let can_zoom_out = zoom > crate::services::space::node::MIN_ZOOM + 0.001;
    if dimmable_icon_button(ui, out, "Zoom out", icon::zoom_out, can_zoom_out) {
        outcome.zoom = -1;
    }
    right = out.left() - 2.0;
    // The reading, which is a button: pressing it puts the zoom back to one, which is the
    // `Reset Font Size` gesture every other zoom in Unluminous has.
    let said = format!("{}%", (zoom * 100.0).round());
    let reading = Rect::from_min_max(
        Pos2::new(right - 46.0, area.top() + 4.0),
        Pos2::new(right, area.bottom() - 5.0),
    );
    let response = ui.interact(reading, ui.id().with("space-zoom-reading"), Sense::click());
    if response.hovered() {
        ui.painter_at(area).rect_filled(reading, CornerRadius::same(6), color::control());
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter_at(area).crisp_text(
        reading.center(),
        Align2::CENTER_CENTER,
        &said,
        FontId::proportional(11.0),
        color::text_dim(),
    );
    // The number is in the name, so a test reads the zoom out of the accessibility tree rather than out
    // of a picture.
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Reset zoom, {said}"))
    });
    if response.clicked() {
        outcome.reset_zoom = true;
    }
    right = reading.left() - 2.0;
    let into = Rect::from_center_size(Pos2::new(right - 11.0, middle), Vec2::splat(22.0));
    let can_zoom_in = zoom < crate::services::space::node::MAX_ZOOM - 0.001;
    if dimmable_icon_button(ui, into, "Zoom in", icon::zoom_in, can_zoom_in) {
        outcome.zoom = 1;
    }
}

/// An icon button that is drawn quiet and answers nothing when it cannot apply.
///
/// `controls::icon_button` has no dimmed form, and the two here need one: a control at the end of its
/// ladder is dimmed rather than absent, because it applies again the moment the other one is pressed.
fn dimmable_icon_button(
    ui: &mut egui::Ui,
    area: Rect,
    name: &str,
    drawing: fn(&egui::Painter, Pos2, Color32),
    enabled: bool,
) -> bool {
    if enabled {
        return crate::components::controls::icon_button(ui, area, name, drawing);
    }
    let response = ui.interact(area, ui.id().with((name, "dimmed")), Sense::hover());
    drawing(&ui.painter_at(area), area.center(), color::text_faint());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, false, name));
    false
}

/// How wide a view's chip is: its name with room either side, floored so a one letter name is a target.
fn chip_width(painter: &egui::Painter, name: &str) -> f32 {
    let galley =
        painter.crisp_layout_no_wrap(name.to_owned(), FontId::proportional(11.5), color::text());
    chip_width_for(galley.size().x)
}

/// The arithmetic behind it, with no fonts in it so a test can check the floor.
///
/// `unluminous-core`'s layout tests are the same bargain: measure through a stub, and the expected
/// numbers are arithmetic a reader can check on every machine.
fn chip_width_for(word: f32) -> f32 {
    (word + 22.0).max(54.0)
}

/// The colours and the canvas the canvas draws with.
///
/// A value rather than six arguments, for the reason `explorer::View` is one: the list had reached the
/// length at which a caller starts passing them in the wrong order.
#[derive(Clone, Copy)]
pub struct Look<'a> {
    pub opacity: f32,
    /// The ground behind the canvas — the pane's own, so the desktop shows through it exactly as much
    /// as it shows through the editing area.
    pub page: Color32,
    /// A node's surface.
    pub card: Color32,
    /// A node's header, a step above its body.
    pub header: Color32,
    pub chrome: &'a Chrome,
}

/// Fill the canvas and draw the dot grid over it.
///
/// The grid is the one thing here drawn in **screen** points from world ones, because it is the canvas
/// rather than anything on it: the dots are a fixed size and their spacing is what the zoom changes,
/// which is what makes a zoom legible.
pub fn ground(ui: &egui::Ui, area: Rect, camera: &Camera, look: Look<'_>) {
    let painter = ui.painter_at(area);
    painter.rect_filled(area, CornerRadius::ZERO, crate::theme::faded(look.page, look.opacity));
    if camera.zoom < GRID_FADES_AT {
        return;
    }
    let step = GRID * camera.zoom;
    let dot = color::text_faint().gamma_multiply(0.35);
    let first = camera.to_screen(area.min, snapped(camera.at));
    let mut y = first.y;
    while y < area.top() {
        y += step;
    }
    while y < area.bottom() {
        let mut x = first.x;
        while x < area.left() {
            x += step;
        }
        while x < area.right() {
            painter.rect_filled(Rect::from_center_size(Pos2::new(x, y), Vec2::splat(1.0)), 0, dot);
            x += step;
        }
        y += step;
    }
}

/// The first grid point at or before a world position.
fn snapped(at: Pos2) -> Pos2 {
    Pos2::new((at.x / GRID).floor() * GRID, (at.y / GRID).floor() * GRID)
}

/// What a wire reported.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct WireOutcome {
    /// A wire was right clicked: where the pointer was, and which connection it is.
    pub menu: Option<(Pos2, u64)>,
}

/// Draw every connection, under every node.
///
/// In the pane's own layer rather than a node's, so a wire never crosses a terminal's text — which is
/// what the sublayers do for nothing: a node's layer is moved directly above the pane's at the end of
/// the frame, so everything drawn here is underneath all of them.
pub fn wires(
    ui: &mut egui::Ui,
    area: Rect,
    view: &View,
    camera: &Camera,
    look: Look<'_>,
) -> WireOutcome {
    let mut outcome = WireOutcome::default();
    let pointer = ui.ctx().pointer_latest_pos().filter(|at| area.contains(*at));
    let clicked = ui.input(|input| input.pointer.secondary_clicked());
    let mut nearest: Option<(f32, u64)> = None;
    for edge in &view.edges {
        let (Some(from), Some(to)) = (view.node(edge.from), view.node(edge.to)) else { continue };
        let start = camera.to_screen(area.min, from.output_port());
        let end = camera.to_screen(area.min, to.input_port());
        // A wire whose two ends are both far off the pane is not drawn: the curve between them cannot
        // cross it. `task-1666`'s rule, applied to a line.
        let span = Rect::from_two_pos(start, end).expand(240.0);
        if !span.intersects(area) {
            continue;
        }
        let points = geometry::curve_points(start, end, WIRE_SEGMENTS);
        let colour = match edge.pipe {
            Pipe::Lines => look_accent(),
            Pipe::Off => color::text_faint(),
        };
        let width = if edge.pipe == Pipe::Lines { 2.0 } else { 1.5 };
        for pair in points.windows(2) {
            look.chrome.line(pair[0], pair[1], width, colour);
        }
        // Flat as well as recorded, so a canvas with the decoration switched off still has its wires.
        if !look.chrome.is_recording() {
            ui.painter_at(area).add(egui::Shape::line(points, egui::Stroke::new(width, colour)));
        }
        if let Some(at) = pointer {
            let near = geometry::distance_to_curve(start, end, at);
            if near <= WIRE_REACH && nearest.is_none_or(|(so_far, _)| near < so_far) {
                nearest = Some((near, edge.id));
            }
        }
    }
    if let (true, Some((_, edge)), Some(at)) = (clicked, nearest, pointer) {
        outcome.menu = Some((at, edge));
    }
    outcome
}

/// The blue the canvas uses for a live wire and a lit port, which is the board's own.
fn look_accent() -> Color32 {
    color::board_accent()
}

/// The wire that follows the pointer while a connection is being made.
///
/// Dashed while it is looking for somewhere to land and solid when it is over a port that will take
/// it, which is Chordical's own `connectionStatus`. Drawn in the pane's layer with `egui` rather than
/// into the chrome, because it changes on every frame of the drag and the chrome is cached against a
/// list that would then never match.
pub fn wire_in_the_air(ui: &egui::Ui, area: Rect, from: Pos2, to: Pos2, landing: bool) {
    let points = geometry::curve_points(from, to, WIRE_SEGMENTS);
    let colour = if landing { look_accent() } else { look_accent().gamma_multiply(0.7) };
    let painter = ui.painter_at(area);
    if landing {
        painter.add(egui::Shape::line(points, egui::Stroke::new(2.5, colour)));
    } else {
        // Dashed, which is what says it has not landed yet.
        for pair in points.chunks(2) {
            if pair.len() == 2 {
                painter.line_segment([pair[0], pair[1]], egui::Stroke::new(2.0, colour));
            }
        }
    }
    painter.circle_filled(to, 4.0, colour);
}

/// Where a node's two parts are, in world points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parts {
    pub header: Rect,
    pub body: Rect,
}

/// A node's header and body, which the window needs before it draws the body.
pub fn parts_of(node: &Node) -> Parts {
    let rect = node.rect();
    let header =
        Rect::from_min_size(rect.min, Vec2::new(rect.width(), NODE_HEADER.min(rect.height())));
    let body = Rect::from_min_max(Pos2::new(rect.left(), header.bottom()), rect.max);
    Parts { header, body }
}

/// What one node reported this frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct NodeOutcome {
    /// The node was clicked anywhere, so it is the chosen one and comes to the front.
    pub chose: bool,
    /// The header is being dragged: how far, in world points.
    pub moved: Option<Vec2>,
    /// An edge or a corner is being dragged: which, and how far, in world points.
    pub resized: Option<(Grip, Vec2)>,
    /// A wire is being pulled out of the output port; where its free end is, in **world** points.
    ///
    /// World rather than screen, because what it is compared against is a node's rectangle — see
    /// `settle_the_wire_in_the_air`, which asks the model which node the free end is over. The two position
    /// fields in this value are in **different spaces**, and nothing said so until `task-1906`, which is how
    /// the menu came to be opened at a world point.
    pub wiring: Option<Pos2>,
    /// The close cross was pressed.
    pub closed: bool,
    /// The font buttons were pressed: -1 or 1. A terminal node only.
    pub font_step: i32,
    /// The header was right clicked: where the pointer was, in **screen** points.
    ///
    /// Screen rather than world, because what it is handed to is `egui::Popup`, which places a menu on the
    /// window's own layer. See [`wiring`](Self::wiring) for the other half of the pair, and
    /// `show_the_header` for where the conversion happens.
    pub menu: Option<Pos2>,
}

/// Everything the frame needs to know about the canvas around this node.
#[derive(Debug, Clone, Copy)]
pub struct Framing<'a> {
    pub chosen: bool,
    /// True while the canvas has the keyboard and this is the chosen node.
    pub keyboard: bool,
    /// The rectangle this node covers on the screen, for the decoration and the grips.
    pub on_screen: Rect,
    /// The canvas's own body, in screen points.
    ///
    /// Kept apart from [`Framing::on_screen`] because that one is also what the scale is worked out from,
    /// and a rectangle cut to the pane would make a node hanging off the edge think it had been zoomed.
    /// What it is **for** is the grips: a node dragged past the canvas's edge must not offer a resize band
    /// out there, over the window's rail.
    pub visible: Rect,
    /// True while a wire is in the air looking for an input port.
    pub wire_is_looking: bool,
    /// Which node that wire would land on, so a port lights up while it is the one.
    pub landing: Option<crate::services::space::NodeId>,
    /// What the header says when the node has not been renamed.
    pub fallback_title: &'a str,
}

/// Draw a node's header, its ports and its resize grips, and report what was done to it.
///
/// Called **after** the body has been drawn into the same `Ui`, so the header, the ports and the grips
/// take the points they cover: egui gives a pointer to the last widget that asked for it, which is the
/// rule `components::splitter` and `components::resize_edges` are both written around.
pub fn frame(ui: &mut egui::Ui, node: &Node, framing: Framing<'_>, look: Look<'_>) -> NodeOutcome {
    let mut outcome = NodeOutcome::default();
    let parts = parts_of(node);
    let rect = node.rect();

    // The surface, in screen points, into the pane's canvas. A node is a raised card in the board's
    // own ladder, with its header a step above its body.
    look.chrome.raised(framing.on_screen, 8.0, Fill::Solid(look.card), Lift::Small);
    let header_on_screen = Rect::from_min_max(
        framing.on_screen.min,
        Pos2::new(
            framing.on_screen.right(),
            framing.on_screen.top() + NODE_HEADER * scale_of(rect, framing.on_screen),
        ),
    );
    look.chrome.rect(
        header_on_screen,
        crate::services::vello_canvas::Corners { nw: 8.0, ne: 8.0, se: 0.0, sw: 0.0 },
        Fill::Solid(look.header),
    );
    show_the_header(ui, node, parts.header, framing, &mut outcome);
    // **The grips before the ports**, because a grip's band runs the whole length of an edge and a
    // port sits in the middle of one: egui gives a pointer to the last widget that asked for it, so
    // the other way round the resize band swallowed every attempt to pull a wire out. The band is
    // twelve points wide and a port's target is eighteen, so a port that wins is a port somebody can
    // hit and the edge either side of it still resizes.
    show_the_grips(ui, node, framing, &mut outcome);
    show_the_ports(ui, node, framing, look, &mut outcome);

    // The chosen node's ring, drawn with `egui` over everything: `Decor` has no rounded outline and a
    // one point rectangle is what every list in Unluminous draws round the row with the keyboard.
    if framing.chosen {
        let tint =
            if framing.keyboard { color::accent() } else { color::accent().gamma_multiply(0.45) };
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(8),
            egui::Stroke::new(1.0, tint),
            egui::StrokeKind::Inside,
        );
    }
    outcome
}

/// How many screen points one world point is, worked out from the two rectangles the frame was given.
fn scale_of(world: Rect, on_screen: Rect) -> f32 {
    match world.width() > 0.0 {
        true => on_screen.width() / world.width(),
        false => 1.0,
    }
}

/// The header: a kind mark, the name, the terminal's two font buttons, and a close cross.
fn show_the_header(
    ui: &mut egui::Ui,
    node: &Node,
    header: Rect,
    framing: Framing<'_>,
    outcome: &mut NodeOutcome,
) {
    let salt = ("space-node-header", node.id);
    let response = ui.interact(header, ui.id().with(salt), Sense::click_and_drag());
    if response.dragged() {
        outcome.moved = Some(response.drag_delta());
    }
    if response.clicked() || response.drag_started() {
        outcome.chose = true;
    }
    if response.secondary_clicked() {
        // **Converted here, because this is where the position leaves the node's layer.** A `Response`'s
        // pointer position is in the layer's *own* coordinates, and a node's layer carries a `TSTransform`
        // with the camera in it — so what is read here is a **world** point, while `egui::Popup` places a
        // menu in **screen** points. At the default camera the two differ by exactly the node's own place on
        // the canvas, so a node near the origin looked right and one at `(700, 460)` opened its menu that far
        // down and left of itself. `task-1906`.
        //
        // `layer_transform_to_global` rather than the camera, so a menu cannot drift from the drawing even
        // if the two ever came apart — and it is the inverse of the call `show_the_ports` already makes,
        // which is what makes the pair legible.
        outcome.menu = response.interact_pointer_pos().or_else(|| response.hover_pos()).map(|at| {
            match ui.ctx().layer_transform_to_global(ui.layer_id()) {
                Some(out) => out * at,
                None => at,
            }
        });
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let painter = ui.painter_at(header);
    let mark = Pos2::new(header.left() + 15.0, header.center().y);
    kind_mark(node.kind())(&painter, mark, color::icon());
    let name = match node.title.trim().is_empty() {
        true => framing.fallback_title.to_owned(),
        false => node.title.clone(),
    };
    let mut right = header.right() - 6.0;
    let close =
        Rect::from_center_size(Pos2::new(right - 9.0, header.center().y), Vec2::splat(18.0));
    if crate::components::controls::icon_button(ui, close, &format!("Close {name}"), icon::cross) {
        outcome.closed = true;
    }
    right = close.left() - 2.0;
    if node.kind() == Kind::Terminal {
        let smaller =
            Rect::from_center_size(Pos2::new(right - 9.0, header.center().y), Vec2::splat(18.0));
        if crate::components::controls::icon_button(
            ui,
            smaller,
            &format!("Smaller text in {name}"),
            icon::collapse,
        ) {
            outcome.font_step = -1;
        }
        let bigger = Rect::from_center_size(
            Pos2::new(smaller.left() - 11.0, header.center().y),
            Vec2::splat(18.0),
        );
        if crate::components::controls::icon_button(
            ui,
            bigger,
            &format!("Bigger text in {name}"),
            icon::plus,
        ) {
            outcome.font_step = 1;
        }
        right = bigger.left() - 2.0;
    }
    let room = (right - (mark.x + 12.0)).max(10.0);
    let galley =
        painter.crisp_layout(name.clone(), FontId::proportional(12.0), color::text(), room);
    painter.crisp_galley(
        Pos2::new(mark.x + 12.0, header.center().y - galley.size().y / 2.0),
        galley,
        color::text(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("Node: {name}"))
    });
}

/// The drawn mark for each kind, which is the same picture the rail uses for the pane it stands for.
fn kind_mark(kind: Kind) -> fn(&egui::Painter, Pos2, Color32) {
    match kind {
        Kind::Terminal => icon::terminal,
        Kind::Browser => icon::image,
        Kind::Folder => icon::folder,
        Kind::Editor => icon::editing_area,
        Kind::Chat => icon::chat,
        Kind::Tasks => icon::board,
    }
}

/// The input port on the left hand edge and the output port on the right.
///
/// A wire is pulled out of the **output** port and let go over an **input** one, which is what the
/// ticket's second capture shows and what every node editor does.
fn show_the_ports(
    ui: &mut egui::Ui,
    node: &Node,
    framing: Framing<'_>,
    look: Look<'_>,
    outcome: &mut NodeOutcome,
) {
    let scale = scale_of(node.rect(), framing.on_screen);
    let output = node.output_port();
    let hit = Rect::from_center_size(output, Vec2::splat(PORT * 3.0));
    let response =
        ui.interact(hit, ui.id().with(("space-port-out", node.id)), Sense::click_and_drag());
    if response.dragged() {
        outcome.wiring = ui.ctx().pointer_latest_pos().and_then(|at| {
            ui.ctx().layer_transform_from_global(ui.layer_id()).map(|back| back * at)
        });
    }
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Connect from this node")
    });

    // **Where a wire lands is the window's decision, not a port's.** A port reports only whether it
    // is lit: `Ui::rect_contains_pointer` answers about the pointer's *interaction* position, which
    // during a drag is where the press began, so a port asked about it while a wire was in the air
    // always said no and no wire could ever be connected with the pointer. `settle_the_wire_in_the_air`
    // asks the model which node the free end is over instead, which is also what draws the curve, so
    // the picture and the drop cannot come apart. It makes the whole node the target rather than its
    // port, which is the more forgiving of the two and is what every node editor does.
    let input = node.input_port();
    let over = framing.wire_is_looking && framing.landing == Some(node.id);

    // Drawn into the pane's canvas in screen points, so a port is a disc of the same size however far
    // the canvas is zoomed out — a port a point and a half wide is a port nobody can hit.
    let on_screen = |world: Pos2| framing.on_screen.min + (world - node.rect().min) * scale;
    look.chrome.disc(on_screen(output), PORT, Fill::Solid(look_accent()));
    look.chrome.ring(on_screen(output), PORT, 1.5, look.card);
    let input_fill = if over { look_accent() } else { color::text_faint() };
    look.chrome.disc(on_screen(input), PORT, Fill::Solid(input_fill));
    look.chrome.ring(on_screen(input), PORT, 1.5, look.card);
    if over {
        look.chrome.glow(
            Rect::from_center_size(on_screen(input), Vec2::splat(PORT * 2.0)),
            PORT,
            look_accent(),
            6.0,
        );
    }
    // And flat as well, for a canvas with the decoration switched off.
    if !look.chrome.is_recording() {
        ui.painter().circle_filled(output, PORT, look_accent());
        ui.painter().circle_filled(input, PORT, input_fill);
    }
}

/// The eight invisible grips round a node's edge.
///
/// Added last, so they take the points they cover from the body underneath them — the rule
/// `components::resize_edges` is written around, applied to a node instead of to the window.
fn show_the_grips(ui: &mut egui::Ui, node: &Node, framing: Framing<'_>, outcome: &mut NodeOutcome) {
    let rect = node.rect();
    let scale = scale_of(rect, framing.on_screen).max(0.01);
    // The reach is measured in **screen** points and converted, so a canvas zoomed out to a quarter
    // still has grips somebody can hit.
    let reach = GRIP_REACH / scale;
    // **The grips are cut to the canvas**, so a node hanging past its edge offers no resize band out
    // there. Every other part of a node is clipped by its own layer or by the decoration's canvas, both of
    // which are already the pane; a grip is an `interact` rather than a drawing, and an interaction is not
    // clipped by either. `task-1905`.
    let inside = framing.visible;
    for grip in Grip::ALL {
        let area = grip_rect(rect, grip, reach);
        let on_screen = Rect::from_min_max(
            framing.on_screen.min + (area.min - rect.min) * scale,
            framing.on_screen.min + (area.max - rect.min) * scale,
        );
        if !on_screen.intersects(inside) {
            continue;
        }
        let response =
            ui.interact(area, ui.id().with(("space-grip", node.id, grip.name())), Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(grip.cursor());
        }
        if response.dragged() {
            outcome.resized = Some((grip, response.drag_delta()));
            outcome.chose = true;
        }
    }
}

/// The rectangle one grip takes, in the same points the node is in.
fn grip_rect(rect: Rect, grip: Grip, reach: f32) -> Rect {
    let corner = Vec2::splat(reach * 2.0);
    match grip {
        Grip::TopLeft => Rect::from_center_size(rect.left_top(), corner),
        Grip::TopRight => Rect::from_center_size(rect.right_top(), corner),
        Grip::BottomLeft => Rect::from_center_size(rect.left_bottom(), corner),
        Grip::BottomRight => Rect::from_center_size(rect.right_bottom(), corner),
        Grip::Left => Rect::from_min_max(
            Pos2::new(rect.left() - reach, rect.top() + reach * 2.0),
            Pos2::new(rect.left() + reach, rect.bottom() - reach * 2.0),
        ),
        Grip::Right => Rect::from_min_max(
            Pos2::new(rect.right() - reach, rect.top() + reach * 2.0),
            Pos2::new(rect.right() + reach, rect.bottom() - reach * 2.0),
        ),
        Grip::Top => Rect::from_min_max(
            Pos2::new(rect.left() + reach * 2.0, rect.top() - reach),
            Pos2::new(rect.right() - reach * 2.0, rect.top() + reach),
        ),
        Grip::Bottom => Rect::from_min_max(
            Pos2::new(rect.left() + reach * 2.0, rect.bottom() - reach),
            Pos2::new(rect.right() - reach * 2.0, rect.bottom() + reach),
        ),
    }
}

/// Light the rectangle a thing being dragged would land in.
///
/// **Where an insertion mark cannot be drawn.** `file_tabs::insertion_mark` puts a bar in a strip, which is
/// the right answer when the thing being carried is joining a row of tabs; a File Editor node showing one
/// file draws no strip at all, and a file dropped on the empty canvas is not joining anything. Both of those
/// are answered by saying where the thing would be instead, which is what every window manager's own drop
/// preview does and what `dock::regions` already draws for a panel being moved to an edge.
///
/// A rectangle with no room in it is not drawn: a node scrolled almost off the canvas would otherwise be a
/// line of accent colour along the pane's edge.
pub fn landing_mark(painter: &egui::Painter, area: Rect) {
    if area.width() < 4.0 || area.height() < 4.0 {
        return;
    }
    painter.rect(
        area,
        egui::CornerRadius::same(6),
        look_accent().gamma_multiply(0.12),
        egui::Stroke::new(1.5, look_accent()),
        egui::StrokeKind::Inside,
    );
}

/// What an empty canvas says, so that a pane nobody has put anything on does not look broken.
pub fn nothing_here_yet(ui: &egui::Ui, area: Rect) {
    let painter = ui.painter_at(area);
    let galley = painter.crisp_layout(
        "Right click to add a node.\n\nA terminal, a web browser, a folder tree or a file editor. Wire one to another to let an agent in a terminal drive it.".to_owned(),
        FontId::proportional(13.0),
        color::text_faint(),
        360.0,
    );
    let at = area.center() - galley.size() / 2.0;
    painter.crisp_galley(at, galley, color::text_faint());
}

/// Which node's id a widget belongs to, for the tests that read the window back.
pub fn node_name(node: &Node, fallback: &str) -> String {
    match node.title.trim().is_empty() {
        true => format!("Node: {fallback}"),
        false => format!("Node: {}", node.title),
    }
}

/// Where a node's contents are clipped to, in world points.
///
/// **The pane**, converted, and shrunk by the window's own resize grips wherever the pane touches the
/// edge of the window. A node layer is moved directly above the pane's at the end of the frame, so
/// without this a node docked against the window's edge would take the drag that resizes the window —
/// which is the one place `components::resize_edges`'s "added last" rule cannot reach, since it is added
/// last within a layer rather than above one.
///
/// **`pane.intersect(inside)` and not the other way round**, which is what this always meant to be: the
/// pane is the ceiling and the grips take a little more off it. `task-1905` reported a folder node drawn
/// over the window's rail, and this is why every part of a node that reaches left of the canvas was drawn
/// there — the rail is inside the *window*, so shrinking the window by six points does not exclude it.
/// The clip has to be the pane's own rectangle first.
pub fn clip_for_nodes(pane: Rect, window: Rect, camera: &Camera) -> Rect {
    let inside = window.shrink(crate::components::resize_edges::EDGE);
    let cut = pane.intersect(inside);
    Rect::from_min_max(camera.to_world(pane.min, cut.min), camera.to_world(pane.min, cut.max))
}

/// Whether a node is worth drawing at all.
///
/// `task-1666`'s rule, which the board already keeps: a canvas with forty nodes on it draws the six
/// that are showing, and the rest cost a rectangle comparison each.
pub fn is_showing(node: &Node, pane: Rect, camera: &Camera) -> bool {
    camera.rect_to_screen(pane.min, node.rect()).expand(24.0).intersects(pane)
}

/// The size a node's letters are set in, which is the terminal's own setting unless the node has been
/// asked for something else.
pub fn font_size_of(node: &Node, fallback: f32) -> f32 {
    match &node.state {
        crate::services::space::State::Terminal(terminal) if terminal.font_size > 0.0 => {
            terminal.font_size
        }
        _ => fallback,
    }
}

/// The size an editor node's letters are set in, which is the window's own setting unless the node has
/// been given one of its own.
///
/// The same shape as [`font_size_of`] for a terminal, and for the same reason: `0.0` means "follow the
/// setting", so a node nobody has zoomed follows `appearance.font.size` and one that has been zoomed keeps
/// its own. `task-1905`.
pub fn editor_font_size_of(node: &Node, fallback: f32) -> f32 {
    match &node.state {
        crate::services::space::State::Editor(editor) if editor.font_size > 0.0 => editor.font_size,
        _ => fallback,
    }
}

/// How much bigger or smaller than its usual size a folder node draws its rows.
pub fn folder_zoom_of(node: &Node) -> f32 {
    match &node.state {
        crate::services::space::State::Folder(folder) if folder.zoom > 0.0 => folder.zoom,
        _ => 1.0,
    }
}

/// One row of the canvas's right click menu, for the window to build its menu from.
pub fn add_entries() -> Vec<(&'static str, Kind)> {
    Kind::ALL.into_iter().map(|kind| (kind.label(), kind)).collect()
}

/// The corner radius a node's surface is drawn with, which is the style guide's own control corner
/// doubled — a node is a card rather than a button.
pub const NODE_CORNER: f32 = 8.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::space::{Space, State};

    #[test]
    fn a_node_is_drawn_only_when_it_can_be_seen() {
        let pane = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let camera = Camera::default();
        let mut space = Space::new();
        let near = space.add_node(Kind::Folder, Pos2::new(100.0, 100.0), None);
        let far = space.add_node(Kind::Folder, Pos2::new(9000.0, 9000.0), None);
        let view = space.current();
        assert!(is_showing(view.node(near).expect("it is there"), pane, &camera));
        assert!(!is_showing(view.node(far).expect("it is there"), pane, &camera));
    }

    #[test]
    fn a_nodes_header_is_taken_out_of_its_top_and_never_out_of_more_than_it_has() {
        let mut space = Space::new();
        let id = space.add_node(Kind::Terminal, Pos2::new(10.0, 20.0), None);
        space.resize_node(id, Vec2::new(400.0, 300.0));
        let node = space.current().node(id).expect("it is there").clone();
        let parts = parts_of(&node);
        assert_eq!(parts.header.height(), NODE_HEADER);
        assert_eq!(parts.body.top(), parts.header.bottom());
        assert_eq!(parts.body.bottom(), node.rect().bottom());

        // A node squashed below its own header still has a header rather than a negative body.
        let squashed = Node { size: Vec2::new(400.0, 12.0), ..node };
        let parts = parts_of(&squashed);
        assert_eq!(parts.header.height(), 12.0);
        assert!(parts.body.height() >= 0.0);
    }

    #[test]
    fn the_nodes_are_clipped_clear_of_the_windows_own_resize_grips() {
        // A node layer is drawn above the pane's, so without this a node against the window's edge
        // would take the drag that resizes the window.
        let window = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0));
        let pane = Rect::from_min_max(Pos2::new(36.0, 400.0), Pos2::new(1000.0, 800.0));
        let camera = Camera::default();
        let clip = clip_for_nodes(pane, window, &camera);
        let back = Rect::from_min_max(
            camera.to_screen(pane.min, clip.min),
            camera.to_screen(pane.min, clip.max),
        );
        let edge = crate::components::resize_edges::EDGE;
        assert!(back.right() <= window.right() - edge + 0.01);
        assert!(back.bottom() <= window.bottom() - edge + 0.01);
        assert_eq!(back.left(), pane.left(), "an edge the window does not own is untouched");
    }

    #[test]
    fn a_terminal_node_uses_its_own_font_size_only_once_it_has_been_given_one() {
        let mut space = Space::new();
        let id = space.add_node(Kind::Terminal, Pos2::ZERO, None);
        let node = space.current().node(id).expect("it is there").clone();
        assert_eq!(font_size_of(&node, 14.0), 14.0, "the terminal's own setting");
        space.change(id, |state| {
            if let State::Terminal(terminal) = state {
                terminal.font_size = 20.0;
            }
        });
        let node = space.current().node(id).expect("it is there").clone();
        assert_eq!(font_size_of(&node, 14.0), 20.0);
    }

    #[test]
    fn every_kind_is_offered_in_the_add_menu_and_has_a_mark_of_its_own() {
        let entries = add_entries();
        assert_eq!(entries.len(), Kind::ALL.len());
        #[allow(clippy::fn_to_numeric_cast_any)]
        let address = |drawing: fn(&egui::Painter, Pos2, Color32)| drawing as usize;
        let marks: Vec<usize> =
            Kind::ALL.into_iter().map(|kind| address(kind_mark(kind))).collect();
        for (at, mark) in marks.iter().enumerate() {
            assert!(!marks[..at].contains(mark), "two kinds share one mark");
        }
    }

    #[test]
    fn a_grip_is_a_band_along_its_own_edge_and_a_corner_is_a_square() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 120.0));
        let left = grip_rect(rect, Grip::Left, 6.0);
        assert!(left.height() < rect.height(), "the corners are left for the corner grips");
        assert!((left.width() - 12.0).abs() < 0.01);
        let corner = grip_rect(rect, Grip::BottomRight, 6.0);
        assert_eq!(corner.center(), rect.right_bottom());
    }

    /// A node's menu is reported in screen points, so it opens where the pointer is.
    ///
    /// `task-1906`: *"the modal popup is way too the left bottom of the node. it should appear where i
    /// clicked."* The header read `interact_pointer_pos`, which is in the **layer's** own coordinates, and a
    /// node's layer carries the camera — so a node at `(700, 460)` reported that as a screen point and
    /// `egui::Popup` opened the menu that far down and left of it.
    ///
    /// The measurement is the value the component reports, read out of a context with no window and no
    /// graphics card, which is what `editor_view`'s own painting tests use.
    #[test]
    fn a_nodes_menu_is_reported_where_the_pointer_is_on_the_screen() {
        let mut space = Space::new();
        // A node well away from the canvas's origin, which is the case that showed the fault.
        let id = space.add_node(Kind::Folder, Pos2::new(700.0, 460.0), None);
        let node = space.current().node(id).expect("it is there").clone();
        // The pane, and a camera looking at the world origin — so world and screen differ by the pane's own
        // corner plus the node's place.
        let pane = Rect::from_min_size(Pos2::new(36.0, 216.0), Vec2::new(1100.0, 500.0));
        let camera = Camera::default();
        let on_screen = camera.rect_to_screen(pane.min, node.rect());
        // A right click a little in from the header's left edge, in screen points, which is what a person's
        // pointer really is.
        let pressed = Pos2::new(on_screen.left() + 40.0, on_screen.top() + 8.0);

        let context = egui::Context::default();
        let chrome = crate::services::vello_canvas::Chrome::off();
        let look = Look {
            opacity: 1.0,
            page: color::editor(),
            card: color::code_panel(),
            header: color::explorer(),
            chrome: &chrome,
        };
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0));
        // **Three passes, and the click is on the last.** egui answers about a widget from the rectangles the
        // *previous* pass recorded, so the first two put the header in the widget list and settle the pointer
        // over it; the third is the one that presses. A secondary click needs the release as well as the
        // press, which is what `Node::click_button` in the test harness sends too.
        let passes = [
            vec![egui::Event::PointerMoved(pressed)],
            vec![egui::Event::PointerMoved(pressed)],
            vec![
                egui::Event::PointerMoved(pressed),
                egui::Event::PointerButton {
                    pos: pressed,
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: egui::Modifiers::default(),
                },
                egui::Event::PointerButton {
                    pos: pressed,
                    button: egui::PointerButton::Secondary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                },
            ],
        ];
        let mut reported = None;
        for events in passes {
            let input = egui::RawInput { events, screen_rect: Some(screen), ..Default::default() };
            let output = context.run_ui(input, |ui| {
                let layer =
                    egui::LayerId::new(egui::Order::Background, egui::Id::new("probe-node"));
                ui.ctx().set_transform_layer(
                    layer,
                    egui::emath::TSTransform::new(pane.min.to_vec2(), camera.zoom),
                );
                let mut node_ui =
                    ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(node.rect()));
                let framing = Framing {
                    chosen: false,
                    keyboard: false,
                    on_screen,
                    visible: pane,
                    wire_is_looking: false,
                    landing: None,
                    fallback_title: "a folder",
                };
                let outcome = frame(&mut node_ui, &node, framing, look);
                if outcome.menu.is_some() {
                    reported = outcome.menu;
                }
            });
            output.drop_without_applying_deltas();
        }

        let reported = reported.expect("the header reported a right click");
        assert!(
            (reported - pressed).length() < 2.0,
            "the menu was reported at {reported:?} and the pointer was at {pressed:?}",
        );
        // And it is nowhere near the world point, which is what it used to answer with.
        let world = Pos2::new(node.at.x + 40.0, node.at.y + 8.0);
        assert!(
            (reported - world).length() > 100.0,
            "the menu is still being reported in world points, at {reported:?}",
        );
    }

    #[test]
    fn a_view_chip_is_wide_enough_to_be_a_target_however_short_its_name() {
        // The floor is what makes a one letter name a target somebody can hit; past it the chip is
        // the word with room either side.
        assert_eq!(chip_width_for(4.0), 54.0);
        assert_eq!(chip_width_for(0.0), 54.0);
        assert_eq!(chip_width_for(100.0), 122.0);
        assert!(chip_width_for(200.0) > chip_width_for(100.0));
    }
}
