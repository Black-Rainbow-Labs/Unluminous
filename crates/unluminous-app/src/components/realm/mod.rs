//! Drawing the Realm.
//!
//! `tasks/task-1904-base-of-infinite-space-tdd.md` is the design and `services::realm` is the model.
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
pub mod colour_picker;
pub mod dialog;
pub mod manager;
pub mod recent;
pub mod shape;

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Sense, Vec2};

use crate::services::realm::geometry::{self, Grip};
use std::path::{Path, PathBuf};

use crate::services::realm::{Camera, Kind, Node, Pipe, Realm};
use crate::services::vello_canvas::{Chrome, Corners, Fill, Lift};
use crate::theme::crisp::CrispPainter;
use crate::theme::{color, icon};

/// The strip along the top of the pane that lists the project's realms: a 36 point track with room round it.
pub const VIEW_BAR: f32 = 50.0;
/// A node's own header, in world points.
pub const NODE_HEADER: f32 = 30.0;
/// How round a node's corners are, in world points: the Settings design's card.
pub const NODE_RADIUS: f32 = 16.0;
/// How far a port sticks out of a node's edge, and how big it is, in world points.
///
/// A port is drawn this size in the world, so it grows and shrinks with the canvas zoom like the node it
/// belongs to (`task-2291`). It used to be this many *screen* points at every zoom, which made the dots
/// look larger beside their node as the canvas was zoomed out and smaller as it was zoomed in.
pub const PORT: f32 = 6.0;
/// The smallest a port's hit area is, in screen points, so a canvas zoomed far out still has a port
/// somebody can take hold of although the dot drawn on it is small.
pub const PORT_SMALLEST_HIT: f32 = 14.0;
/// How near the pointer has to come to a port, in screen points, before the port is drawn (`task-2282`).
///
/// *"the connectors for nodes shouldn't always be visible. They should show up when my mouse gets within
/// 100 px or so to them, like a hover in/out."* A port is still there to be pulled while it is hidden, and
/// the pointer is always within this distance of it before it can be. Widened to 150 when Jason asked for
/// *"within 150px or so"*.
pub const PORT_REVEAL: f32 = 150.0;
/// How long a port takes to fade in or out, in seconds.
const PORT_FADE: f32 = 0.15;
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

/// What the realm bar reported.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct BarOutcome {
    /// A realm was chosen, named by its path in the project.
    pub show: Option<PathBuf>,
    /// The plus was pressed.
    pub add: bool,
    /// A realm was right clicked: where the pointer was, and which one.
    pub menu: Option<(Pos2, PathBuf)>,
    /// The zoom buttons were pressed: -1 or 1, in notches.
    pub zoom: i32,
    /// The reading between them was pressed, which puts the zoom back to one.
    pub reset_zoom: bool,
    /// The row saying how many realms are not listed was pressed, which opens the manager.
    pub manage: bool,
    /// The folder chip at the left was pressed, which opens or closes the list of realm files (`task-2293`).
    pub files: bool,
    /// Where the folder chip was drawn, which is what the list hangs from.
    pub files_chip: Option<Rect>,
}

/// How much room the overflow row takes, when there is one.
const MORE_ROW: f32 = 74.0;
/// How much room the zoom controls take at the right hand end of the bar: a track holding two 26 point chips
/// and the reading between them.
const ZOOM_CONTROLS: f32 = 128.0;
/// How much room the plus takes, which sits after the last chip rather than at the far right.
///
/// `task-2004`: *"The + sign to add a new view to base of infinite space should be on the right of the
/// last view/tab, similar to Firefox browser."* The chip is 30 points across with ten of gap in front of it.
const PLUS: f32 = 40.0;
/// The track the realms sit in, and a realm's pill inside it.
const TRACK: f32 = 36.0;
const PILL: f32 = 28.0;
const TRACK_PAD: f32 = 4.0;
const PILL_GAP: f32 = 3.0;
/// The bar's own margin at either end.
const BAR_SIDE: f32 = 14.0;
/// The folder chip at the left of the bar, and the gap after it (`task-2293`).
const FILES_CHIP: f32 = 30.0;
const FILES_GAP: f32 = 10.0;

/// The realm bar along the top of the canvas: one pill a realm file in the project, the open one raised, a
/// round `+` chip after the last of them, and the zoom controls at the right.
///
/// `task-2202`: it replaced the strip of views one for one, because a realm file is what a view was.
/// `task-2250` drew it in the language of the Agent-Tasks board's header: the realms are pills in a carved
/// track with the one showing raised out of it, which is the board's view switch, and every button is a
/// chip.
///
/// `task-1904` asked to *"Edit project/view names, delete, create new, clone/duplicate"*. The four of those
/// that are about one realm are on its right click menu, which is where the explorer, the tabs and every
/// panel in Unluminous already put the things that are about one row.
/// `views` is a path and a name a realm, which is all the bar draws.
///
/// **Not `&[Realm]`** (`task-1984` A8): only one realm is read at a time, and two fields is what the bar
/// reads.
pub fn view_bar(
    ui: &mut egui::Ui,
    area: Rect,
    views: &[(PathBuf, String)],
    current: &Path,
    zoom: f32,
    look: Look<'_>,
) -> BarOutcome {
    let mut outcome = BarOutcome::default();
    let painter = ui.painter_at(area);
    // The bar is the canvas's own ground with a groove under it, rather than a strip of a second colour: the
    // pills and the chips are what say it is a bar.
    painter.rect_filled(
        area,
        CornerRadius::ZERO,
        crate::theme::pane_ground(look.page, look.opacity),
    );
    crate::components::chrome_kit::groove(
        &painter,
        Pos2::new(area.left(), area.bottom() - 1.5),
        Pos2::new(area.right(), area.bottom() - 1.5),
    );
    let middle = area.center().y;
    let style = rux::Style::sans(12.0).medium();
    crate::components::chrome_kit::layer(ui, "realm-bar", area, 16.0, |rux| {
        let theme = rux.theme();
        let chat = theme.chat;
        let limit = area.right() - BAR_SIDE - PLUS - ZOOM_CONTROLS - MORE_ROW;
        // **The folder chip first**, which lists every realm file in the project newest first, so a tab that was
        // closed is one press from coming back (`task-2293`).
        let files = Rect::from_min_size(
            Pos2::new(area.left() + BAR_SIDE, middle - FILES_CHIP / 2.0),
            Vec2::splat(FILES_CHIP),
        );
        outcome.files_chip = Some(files);
        if rux::components::ChipButton::new(rux::Icon::Folder, FILES_CHIP, "Realm files")
            .mark(14.0, Some(1.7))
            .show(rux, files)
            .clicked()
        {
            outcome.files = true;
        }
        let left = files.right() + FILES_GAP;
        // How many realms fit, measured before the track is drawn so the track is as long as what is in it.
        let mut widths = Vec::new();
        let mut right = left + TRACK_PAD;
        for (_, name) in views {
            let width = pill_width_for(rux.measure(style, name).x);
            // **Room kept for the overflow row as well**, because a bar that filled itself to the edge and
            // then said "3 more" off the end of itself would be the fault it is there to fix.
            if right + width + TRACK_PAD > limit {
                break;
            }
            widths.push(width);
            right += width + PILL_GAP;
        }
        let drawn = widths.len();
        // **How many are not listed, and a way to see them.** `task-1906`.
        let left_out = manager::not_showing(views.len(), drawn);
        let more_width = match left_out {
            0 => 0.0,
            _ => MORE_ROW - 10.0,
        };
        let track_right = match (drawn, left_out) {
            (0, 0) => left,
            _ => {
                right - PILL_GAP
                    + more_width
                    + TRACK_PAD
                    + if left_out > 0 { PILL_GAP } else { 0.0 }
            }
        };
        let track = Rect::from_min_max(
            Pos2::new(left, middle - TRACK / 2.0),
            Pos2::new(track_right, middle + TRACK / 2.0),
        );
        if track.width() > TRACK_PAD * 2.0 {
            rux.chrome.surface(track, TRACK / 2.0, chat.well, chat.carved_sm());
        }
        let mut pen = left + TRACK_PAD;
        for ((view_id, view_name), width) in views.iter().zip(widths) {
            let chip =
                Rect::from_min_size(Pos2::new(pen, middle - PILL / 2.0), Vec2::new(width, PILL));
            pen = chip.right() + PILL_GAP;
            let on = view_id == current;
            let response =
                rux.ui.interact(chip, rux.ui.id().with(("realm-view", view_id)), Sense::click());
            let hovered = response.hovered();
            if on {
                rux.chrome.surface(
                    chip,
                    PILL / 2.0,
                    rux::Fill::gradient(chat.chip, chip),
                    chat.raised_chip(),
                );
            } else if hovered {
                rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let ink = match (on, hovered) {
                (true, _) => theme.ink.i900,
                (false, true) => chat.sky,
                (false, false) => theme.ink.i500,
            };
            let galley = rux.elided(style, view_name, ink, width - 20.0);
            rux::text::draw_centred(rux.painter(), chip, galley, ink);
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    true,
                    on,
                    // Not `Realm: <name>`, which is the manager's row for the same realm: two controls
                    // must not share a name, and both can be on the screen at once.
                    format!("Realm tab: {}", view_name),
                )
            });
            if response.clicked() {
                outcome.show = Some(view_id.clone());
            }
            if response.secondary_clicked() {
                if let Some(at) = response.interact_pointer_pos().or_else(|| response.hover_pos()) {
                    outcome.menu = Some((at, view_id.clone()));
                }
            }
        }
        if left_out > 0 {
            let row = Rect::from_min_size(
                Pos2::new(pen, middle - PILL / 2.0),
                Vec2::new(more_width, PILL),
            );
            let response =
                rux.ui.interact(row, rux.ui.id().with("realm-views-more"), Sense::click());
            let ink = match response.hovered() {
                true => chat.sky,
                false => theme.ink.i500,
            };
            if response.hovered() {
                rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let said = format!("+{left_out} more");
            let galley = rux.text(rux::Style::sans(11.5), &said, ink);
            rux::text::draw_centred(rux.painter(), row, galley, ink);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Realms, {said}"))
            });
            if response.clicked() {
                outcome.manage = true;
            }
        }
        // **After the last realm, which is where a browser puts it** — `task-2004`. It is kept clear of the
        // zoom controls, so a bar full of realms puts the plus where they stopped rather than underneath the
        // zoom buttons.
        let plus_left =
            (track.right() + 10.0).min(area.right() - BAR_SIDE - ZOOM_CONTROLS - PLUS + 10.0);
        let plus = Rect::from_min_size(Pos2::new(plus_left, middle - 15.0), Vec2::splat(30.0));
        if rux::components::ChipButton::new(rux::Icon::Plus, 30.0, "New Realm")
            .mark(12.0, Some(1.8))
            .show(rux, plus)
            .clicked()
        {
            outcome.add = true;
        }
        show_the_zoom_controls(rux, area, zoom, &mut outcome);
    });
    outcome
}

/// The two zoom buttons and the reading between them, at the right hand end of the bar, in a carved track.
///
/// `task-1905` asks for *"classic - + buttons with cirlces around them"* at the top right, and the top
/// right of the canvas **is** the right hand end of this bar: a floating control over the ground would be
/// drawn under every node that happened to be there, because a node's layer is a sublayer composited
/// above the pane's.
///
/// Both buttons **dim at the ends of the ladder** rather than disappearing, because a zoom that cannot go
/// further is a control that will apply again the moment the other one is pressed. That is the dimmed half
/// of Unluminous's absent-control rule.
fn show_the_zoom_controls(rux: &mut rux::Rux<'_>, area: Rect, zoom: f32, outcome: &mut BarOutcome) {
    let chat = rux.theme().chat;
    let ink = rux.theme().ink;
    let middle = area.center().y;
    let track = Rect::from_min_max(
        Pos2::new(area.right() - BAR_SIDE - ZOOM_CONTROLS, middle - TRACK / 2.0),
        Pos2::new(area.right() - BAR_SIDE, middle + TRACK / 2.0),
    );
    rux.chrome.surface(track, TRACK / 2.0, chat.well, chat.carved_sm());
    // Minus on the left and plus on the right (`task-2287`), the order a number line reads in.
    let chip = 28.0;
    let out = Rect::from_center_size(
        Pos2::new(track.left() + TRACK_PAD + chip / 2.0, middle),
        Vec2::splat(chip),
    );
    let into = Rect::from_center_size(
        Pos2::new(track.right() - TRACK_PAD - chip / 2.0, middle),
        Vec2::splat(chip),
    );
    let can_zoom_in = zoom < crate::services::realm::node::MAX_ZOOM - 0.001;
    if zoom_chip(rux, into, "Zoom in", can_zoom_in, true) {
        outcome.zoom = 1;
    }
    // At the bottom of the ladder there is nowhere further out to go.
    let can_zoom_out = zoom > crate::services::realm::node::MIN_ZOOM + 0.001;
    if zoom_chip(rux, out, "Zoom out", can_zoom_out, false) {
        outcome.zoom = -1;
    }
    // The reading, which is a button: pressing it puts the zoom back to one, which is the `Reset Font Size`
    // gesture every other zoom in Unluminous has.
    let said = format!("{}%", (zoom * 100.0).round());
    let reading = Rect::from_min_max(
        Pos2::new(out.right() + 2.0, middle - PILL / 2.0),
        Pos2::new(into.left() - 2.0, middle + PILL / 2.0),
    );
    let response = rux.ui.interact(reading, rux.ui.id().with("realm-zoom-reading"), Sense::click());
    let tint = match response.hovered() {
        true => chat.sky,
        false => ink.i700,
    };
    if response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let galley = rux.text(rux::Style::sans(11.5).medium(), &said, tint);
    rux::text::draw_centred(rux.painter(), reading, galley, tint);
    // The number is in the name, so a test reads the zoom out of the accessibility tree rather than out of a
    // picture.
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Reset zoom, {said}"))
    });
    if response.clicked() {
        outcome.reset_zoom = true;
    }
}

/// One of the two zoom chips: a raised chip with a plus or a minus, drawn quiet and answering nothing when it
/// cannot apply.
fn zoom_chip(rux: &mut rux::Rux<'_>, rect: Rect, name: &str, enabled: bool, plus: bool) -> bool {
    let chat = rux.theme().chat;
    let ink = rux.theme().ink;
    let response = match enabled {
        true => rux.ui.interact(rect, rux.ui.id().with(("realm-zoom", name)), Sense::click()),
        false => rux.ui.interact(rect, rux.ui.id().with((name, "dimmed")), Sense::hover()),
    };
    let held = enabled && response.is_pointer_button_down_on();
    match held {
        true => rux.chrome.surface(rect, rect.width() / 2.0, chat.well, chat.carved_sm()),
        false => rux.chrome.surface(
            rect,
            rect.width() / 2.0,
            rux::Fill::gradient(chat.chip, rect),
            chat.raised_chip(),
        ),
    }
    let tint = match (enabled, response.hovered()) {
        (false, _) => ink.i300,
        (true, true) => chat.sky,
        (true, false) => ink.i500,
    };
    if enabled && response.hovered() {
        rux.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let centre = rect.center();
    let stroke = egui::Stroke::new(1.7, tint);
    rux.painter()
        .line_segment([centre - Vec2::new(5.0, 0.0), centre + Vec2::new(5.0, 0.0)], stroke);
    if plus {
        rux.painter()
            .line_segment([centre - Vec2::new(0.0, 5.0), centre + Vec2::new(0.0, 5.0)], stroke);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    enabled && response.clicked()
}

/// How wide a realm's pill is for words this wide, floored so a one letter name is a target.
///
/// The arithmetic on its own, with no fonts in it so a test can check the floor: `unluminous-core`'s layout
/// tests are the same bargain.
fn pill_width_for(word: f32) -> f32 {
    (word + 28.0).max(58.0)
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
    painter.rect_filled(
        area,
        CornerRadius::ZERO,
        crate::theme::pane_ground(look.page, look.opacity),
    );
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
    /// The primary button went down on a wire, over no node: which connection, where, and whether the press
    /// was nearer the end it arrives at than the end it leaves from (`task-2293`). The window picks that end up
    /// once the pointer moves, so a click on a wire does nothing to it.
    pub pressed: Option<(u64, Pos2, bool)>,
}

/// Draw every connection, under every node.
///
/// In the pane's own layer rather than a node's, so a wire never crosses a terminal's text — which is
/// what the sublayers do for nothing: a node's layer is moved directly above the pane's at the end of
/// the frame, so everything drawn here is underneath all of them.
pub fn wires(
    ui: &mut egui::Ui,
    area: Rect,
    view: &Realm,
    camera: &Camera,
    look: Look<'_>,
) -> WireOutcome {
    let mut outcome = WireOutcome::default();
    let pointer = ui.ctx().pointer_latest_pos().filter(|at| area.contains(*at));
    let clicked = ui.input(|input| input.pointer.secondary_clicked());
    let pressed = ui.input(|input| input.pointer.primary_pressed());
    let mut nearest: Option<(f32, u64, bool)> = None;
    for edge in &view.edges {
        let Some(points) = wire_points(view, edge, camera, area.min) else { continue };
        let (start, end) = (points[0], points[points.len() - 1]);
        // A wire whose two ends are both far off the pane is not drawn: the curve between them cannot
        // cross it. `task-1666`'s rule, applied to a line.
        let span = Rect::from_two_pos(start, end).expand(240.0);
        if !span.intersects(area) {
            continue;
        }
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
            let near =
                geometry::distance_to_points(&wire_hit_points(view, edge, camera, area.min), at);
            if near <= WIRE_REACH && nearest.is_none_or(|(so_far, _, _)| near < so_far) {
                let arriving_end = at.distance(end) <= at.distance(start);
                nearest = Some((near, edge.id, arriving_end));
            }
        }
    }
    if let (true, Some((_, edge, _)), Some(at)) = (clicked, nearest, pointer) {
        outcome.menu = Some((at, edge));
    }
    // **Not over a node**, because a wire is drawn under every node and a press on a node is the node's.
    let over_a_node =
        pointer.is_some_and(|at| view.node_at(camera.to_world(area.min, at)).is_some());
    if let (true, false, Some((_, edge, arriving_end)), Some(at)) =
        (pressed, over_a_node, nearest, pointer)
    {
        outcome.pressed = Some((edge, at, arriving_end));
    }
    outcome
}

/// The line one wire is drawn along, in screen points.
///
/// **A wire between two nodes that are not shapes is the curve it always was**, leaving the output port going
/// right and arriving at the input port going right. A wire with a shape at either end leaves and arrives
/// along the way out of the point it joins, because a shape is joined on all four sides. `task-2284`.
pub fn wire_points(
    view: &Realm,
    edge: &crate::services::realm::Edge,
    camera: &Camera,
    origin: Pos2,
) -> Option<Vec<Pos2>> {
    wire_points_at(view, edge, camera, origin, WIRE_SEGMENTS)
}

/// The same line sampled finely, which is what a right click is measured against.
fn wire_hit_points(
    view: &Realm,
    edge: &crate::services::realm::Edge,
    camera: &Camera,
    origin: Pos2,
) -> Vec<Pos2> {
    wire_points_at(view, edge, camera, origin, 32).unwrap_or_default()
}

fn wire_points_at(
    view: &Realm,
    edge: &crate::services::realm::Edge,
    camera: &Camera,
    origin: Pos2,
    segments: usize,
) -> Option<Vec<Pos2>> {
    let (from, to) = (view.node(edge.from)?, view.node(edge.to)?);
    let (leaving, arriving) = view.wire_ends(edge)?;
    let start = camera.to_screen(origin, leaving.at);
    let end = camera.to_screen(origin, arriving.at);
    Some(match from.shape().is_none() && to.shape().is_none() {
        true => geometry::curve_points(start, end, segments),
        false => geometry::directed_points(start, leaving.out, end, arriving.out, segments),
    })
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
    wire_in_the_air_along(ui, area, geometry::curve_points(from, to, WIRE_SEGMENTS), landing);
}

/// A wire being pulled out of a shape's point, which leaves along the way out of that point and, when it is
/// over somewhere it would land, arrives along the way out of the point it would land on. `task-2284`.
pub fn directed_wire_in_the_air(
    ui: &egui::Ui,
    area: Rect,
    from: crate::services::realm::Anchor,
    to: Pos2,
    arriving: Option<Vec2>,
    landing: bool,
) {
    let points = geometry::directed_points(
        from.at,
        from.out,
        to,
        arriving.unwrap_or(Vec2::ZERO),
        WIRE_SEGMENTS,
    );
    wire_in_the_air_along(ui, area, points, landing);
}

/// Draw a wire in the air along `points`, dashed until it is over somewhere it would land.
fn wire_in_the_air_along(ui: &egui::Ui, area: Rect, points: Vec<Pos2>, landing: bool) {
    let to = points[points.len() - 1];
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

/// Paint a node's surface into the node's own layer, under everything else the node draws.
///
/// **This is what makes a node hide the nodes behind it** (`task-2200`). The raised surface and its
/// shadows go into the pane's canvas, which is rasterised once *underneath every node layer*, so a node
/// whose surface lived only there covered nothing a node behind it drew into its own layer: the text of a
/// File Editor node sent behind another node showed through it. The same two colours the canvas paints,
/// flat and opaque, so where nothing overlaps the picture is exactly what it was. Painted in world points,
/// because the layer carries the camera. Under glass the colours are translucent and the bent picture is
/// painted under them, which is what covers (`task-2281`).
pub fn cover(ui: &egui::Ui, node: &Node, look: Look<'_>) {
    let _ = look;
    let parts = parts_of(node);
    let chat = crate::components::chrome_kit::chat();
    // Under glass the card's tint is translucent, so the bent picture goes under it at full strength:
    // that is what still hides the nodes behind (`task-2281`). With no picture to bend, an opaque
    // ground under the tint does the hiding instead.
    if crate::theme::is_glass() {
        let bent = crate::theme::glass::paint_glass(ui.painter(), node.rect(), NODE_RADIUS);
        if !bent {
            let round = CornerRadius::same(NODE_RADIUS as u8);
            ui.painter().rect_filled(node.rect(), round, color::editor());
        }
    }
    gradient_card(ui.painter(), node.rect(), NODE_RADIUS, chat.card);
    // The groove under the header, which is what says where the header ends now that the header is the
    // card's own surface rather than a band of a second colour (`task-2250`).
    let inset = 12.0;
    crate::components::chrome_kit::groove(
        ui.painter(),
        Pos2::new(parts.header.left() + inset, parts.header.bottom() - 1.0),
        Pos2::new(parts.header.right() - inset, parts.header.bottom() - 1.0),
    );
}

/// A rounded rectangle filled with a `rux` gradient, painted with `egui`.
///
/// `egui` has no gradient fill, so the rectangle is tessellated flat and each vertex is given the colour the
/// gradient has at that point. The tessellator's own feathering is kept by multiplying by the alpha it gave
/// the vertex, so the edge is as smooth as a flat fill's.
fn gradient_card(painter: &egui::Painter, rect: Rect, radius: f32, gradient: rux::theme::Gradient) {
    let (from, to) = gradient.axis(rect);
    let along = to - from;
    let length = along.length_sq().max(f32::EPSILON);
    let shape =
        egui::epaint::RectShape::filled(rect, CornerRadius::same(radius as u8), Color32::WHITE); // any ground: every vertex is given the gradient's colour below
    let mut mesh = egui::epaint::Mesh::default();
    let mut tessellator = egui::epaint::Tessellator::new(
        painter.ctx().pixels_per_point(),
        egui::epaint::TessellationOptions::default(),
        [1, 1],
        Vec::new(),
    );
    tessellator.tessellate_rect(&shape, &mut mesh);
    for vertex in &mut mesh.vertices {
        let t = ((vertex.pos - from).dot(along) / length).clamp(0.0, 1.0);
        let colour = lerp_colour(gradient.from, gradient.to, t);
        vertex.color = colour.gamma_multiply(vertex.color.a() as f32 / 255.0);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// The colour a node's card reads as in the middle, which is what the ring round a port is drawn in so the
/// port looks cut out of the card.
pub fn card_colour() -> Color32 {
    let gradient = crate::components::chrome_kit::chat().card;
    lerp_colour(gradient.from, gradient.to, 0.5)
}

/// The colour a fraction of the way from one colour to another.
fn lerp_colour(from: Color32, to: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
        mix(from.a(), to.a()),
    )
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
    /// One of a note node's three view buttons was pressed. `task-2202`.
    pub view: Option<crate::services::realm::NoteView>,
    /// Which of a shape's points the wire in [`wiring`](Self::wiring) is being pulled from. `None` is a
    /// node's output port. `task-2284`.
    pub point: Option<usize>,
    /// A shape was double clicked, which starts typing in it.
    pub edit: bool,
    /// Typing in a shape finished this frame: Escape, Ctrl+Enter, or a click outside it.
    pub finished: bool,
    /// A shape's text box holds the keyboard.
    pub typing: bool,
    /// Shift was held while the shape was resized, which keeps it square.
    pub square: bool,
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
    pub landing: Option<crate::services::realm::NodeId>,
    /// What the header says when the node has not been renamed.
    pub fallback_title: &'a str,
    /// How the tab a File Editor node is showing is viewed, and which kind of preview it has, when it is a
    /// document with a preview (`task-2270`). `None` for every other node and every other tab, which draws no
    /// view buttons.
    pub file_view:
        Option<(crate::services::realm::NoteView, crate::services::file_kind::PreviewKind)>,
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

    // The surface's shadow, in screen points, into the pane's canvas. A node is a raised card in the
    // Settings design's values (`task-2250`): the card's gradient, which [`cover`] paints into the node's own
    // layer over this, and a medium lift, which is what lifts it off the dotted ground.
    let scale = scale_of(rect, framing.on_screen);
    let chat = crate::components::chrome_kit::chat();
    let (from, to) = chat.card.axis(framing.on_screen);
    look.chrome.raised(
        framing.on_screen,
        Corners::from(NODE_RADIUS * scale),
        Fill::Linear { from, to, start: chat.card.from, end: chat.card.to },
        Lift::Medium,
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
            CornerRadius::same(NODE_RADIUS as u8),
            egui::Stroke::new(1.5, tint),
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
    let salt = ("realm-node-header", node.id);
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
    let chat = crate::components::chrome_kit::chat();
    let mark = Pos2::new(header.left() + 20.0, header.center().y);
    // The kind's mark in a small carved disc, which is how the design marks what a card is about.
    painter.circle(mark, 10.0, chat.well, egui::Stroke::new(1.0, chat.dark.gamma_multiply(0.6)));
    kind_mark(node.kind())(&painter, mark, chat.sky);
    let name = match node.title.trim().is_empty() {
        true => framing.fallback_title.to_owned(),
        false => node.title.clone(),
    };
    let mut right = header.right() - 8.0;
    let close =
        Rect::from_center_size(Pos2::new(right - 9.0, header.center().y), Vec2::splat(18.0));
    // Prefixed for `no_two_controls_share_a_name`'s reason -- see `file_tabs::show`.
    if crate::components::controls::icon_button(
        ui,
        close,
        &format!("Close node {name}"),
        icon::cross,
    ) {
        outcome.closed = true;
    }
    right = close.left() - 2.0;
    // **A note's three views, at the right of its header**, which is where the title bar draws the same
    // three for a Markdown tab. `task-2202`. Named after the note, `Raw Markdown in Plan.md`, because the
    // title bar's own three carry the bare names and two controls must not share one.
    if let crate::services::realm::State::Note(note) = &node.state {
        for view in crate::services::realm::NoteView::ALL.into_iter().rev() {
            let button = Rect::from_center_size(
                Pos2::new(right - 10.0, header.center().y),
                Vec2::splat(20.0),
            );
            if note_view_button(
                ui,
                button,
                view,
                note.view == view,
                &name,
                crate::services::file_kind::PreviewKind::Markdown,
            ) {
                outcome.view = Some(view);
            }
            right = button.left() - 2.0;
        }
    }
    // **And a File Editor node's, for the tab it is showing** (`task-2270`): *"File should have preview buttons for
    // md files, etc."* The same three, the same names, for a Markdown or a Mermaid tab, and for no other.
    if let (Kind::Editor, Some((showing, kind))) = (node.kind(), framing.file_view) {
        for view in crate::services::realm::NoteView::ALL.into_iter().rev() {
            let button = Rect::from_center_size(
                Pos2::new(right - 10.0, header.center().y),
                Vec2::splat(20.0),
            );
            if note_view_button(ui, button, view, showing == view, &name, kind) {
                outcome.view = Some(view);
            }
            right = button.left() - 2.0;
        }
    }
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
    let room = (right - (mark.x + 18.0)).max(10.0);
    let font = title_font(ui.ctx());
    let galley = painter.crisp_layout(name.clone(), font, color::text_strong(), room);
    painter.crisp_galley(
        Pos2::new(mark.x + 18.0, header.center().y - galley.size().y / 2.0),
        galley,
        color::text_strong(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("Node: {name}"))
    });
}

/// The design's semibold for a node's name, or the interface's own face in a window that has not bound
/// `rux`'s families, which is a test's bare context.
fn title_font(ctx: &egui::Context) -> FontId {
    let font = rux::Style::sans(12.5).semibold().font();
    let bound = ctx.fonts(|fonts| fonts.definitions().families.contains_key(&font.family));
    match bound {
        true => font,
        false => FontId::proportional(12.5),
    }
}

/// The view mode a note's view is, which is what the title bar's own buttons and `OpenFile::view_mode` use.
pub fn view_mode_of(view: crate::services::realm::NoteView) -> crate::app::ViewMode {
    match view {
        crate::services::realm::NoteView::Raw => crate::app::ViewMode::Raw,
        crate::services::realm::NoteView::Side => crate::app::ViewMode::SideBySide,
        crate::services::realm::NoteView::Preview => crate::app::ViewMode::Preview,
    }
}

/// The note view a tab's view mode is.
pub fn note_view_of(mode: crate::app::ViewMode) -> crate::services::realm::NoteView {
    match mode {
        crate::app::ViewMode::Raw => crate::services::realm::NoteView::Raw,
        crate::app::ViewMode::SideBySide => crate::services::realm::NoteView::Side,
        crate::app::ViewMode::Preview => crate::services::realm::NoteView::Preview,
    }
}

/// One of a node's three view buttons: the title bar's drawing, named after the node.
fn note_view_button(
    ui: &mut egui::Ui,
    area: Rect,
    view: crate::services::realm::NoteView,
    active: bool,
    note: &str,
    kind: crate::services::file_kind::PreviewKind,
) -> bool {
    let mode = view_mode_of(view);
    let label = mode.label_for(kind);
    let name = format!("{label} in {note}");
    let response = ui.interact(area, ui.id().with(("realm-note-view", &name)), Sense::click());
    let painter = ui.painter();
    if active {
        painter.rect_filled(area, egui::CornerRadius::same(4), color::accent());
    } else if response.hovered() {
        painter.rect_filled(area, egui::CornerRadius::same(4), color::control());
    }
    let tint = if active { color::on_accent() } else { color::text_control() };
    icon::view_mode(painter, area.shrink(5.0), mode, tint);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, &name));
    response.clicked()
}

/// The drawn mark for each kind, for the Add a node dialog, which shows each kind with its mark.
pub fn kind_mark_of(kind: Kind) -> fn(&egui::Painter, Pos2, Color32) {
    kind_mark(kind)
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
        Kind::Image => icon::photo,
        Kind::Audio => icon::audio,
        Kind::Video => icon::video,
        Kind::Note => icon::text_page,
        Kind::Shape => shape::kind_mark,
        Kind::Unknown => icon::unknown,
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
    let hit = Rect::from_center_size(
        output,
        Vec2::splat((PORT * 3.0).max(PORT_SMALLEST_HIT / scale.max(0.01))),
    );
    let response =
        ui.interact(hit, ui.id().with(("realm-port-out", node.id)), Sense::click_and_drag());
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

    // Drawn into the pane's canvas in screen points at the canvas's own scale, so a port keeps its size
    // beside its node at every zoom. Its hit area has a floor in screen points instead.
    let on_screen = |world: Pos2| framing.on_screen.min + (world - node.rect().min) * scale;
    // **Shown only while the pointer is near it** (`task-2282`), fading in and out. The output port also
    // stays while its wire is being pulled, and every input port shows while a wire is in the air, so
    // there is always somewhere visible to let it go.
    let pointer = ui.ctx().pointer_hover_pos();
    let near = |at: Pos2| pointer.is_some_and(|pointer| pointer.distance(at) <= PORT_REVEAL);
    let out_shown = port_fade(
        ui,
        ("realm-port-out-shown", node.id),
        near(on_screen(output)) || response.hovered() || response.dragged(),
    );
    let in_shown = port_fade(
        ui,
        ("realm-port-in-shown", node.id),
        near(on_screen(input)) || framing.wire_is_looking,
    );
    let input_fill = if over { look_accent() } else { color::text_faint() };
    paint_a_port(ui, look, on_screen(output), output, scale, look_accent(), out_shown);
    paint_a_port(ui, look, on_screen(input), input, scale, input_fill, in_shown);
    if over {
        look.chrome.glow(
            Rect::from_center_size(on_screen(input), Vec2::splat(PORT * 2.0 * scale)),
            PORT * scale,
            look_accent(),
            6.0 * scale,
        );
    }
}

/// How far a port has faded in, from 0 hidden to 1 shown, moving towards `shown` over [`PORT_FADE`].
///
/// @param ui - the node's `Ui`, whose context keeps the fade between frames
/// @param id - the port's own name
/// @param shown - whether the port should be showing
fn port_fade(ui: &egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, shown: bool) -> f32 {
    ui.ctx().animate_bool_with_time(ui.id().with(id), shown, PORT_FADE)
}

/// One port: a disc with a ring the card's colour, at `opacity`, or nothing while it is faded right out.
///
/// It is [`PORT`] world points across, so it zooms with the canvas (`task-2291`). It goes into the pane's
/// canvas in screen points, and **into the node's own layer as well** (`task-2200`): the canvas is
/// underneath every node layer, and since a node paints its card into its layer (see [`cover`]) the half
/// of each port inside the node's edge was hidden by the node's own card. The same disc and ring in the
/// layer's world points is also the flat form for a canvas with the decoration switched off.
///
/// @param ui - the node's `Ui`
/// @param look - the canvas being recorded into
/// @param screen - the port's centre in screen points
/// @param world - the port's centre in world points
/// @param scale - screen points to a world point
/// @param fill - the disc's colour
/// @param opacity - how far it has faded in
fn paint_a_port(
    ui: &egui::Ui,
    look: Look<'_>,
    screen: Pos2,
    world: Pos2,
    scale: f32,
    fill: Color32,
    opacity: f32,
) {
    if opacity <= 0.01 {
        return;
    }
    let fill = fill.gamma_multiply(opacity);
    let card = look.card.gamma_multiply(opacity);
    look.chrome.disc(screen, PORT * scale, Fill::Solid(fill));
    look.chrome.ring(screen, PORT * scale, 1.5 * scale, card);
    ui.painter().circle(world, PORT, fill, egui::Stroke::new(1.5, card));
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
            ui.interact(area, ui.id().with(("realm-grip", node.id, grip.name())), Sense::drag());
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
///
/// **Under glass it is the pane's glass panel**, which stands in from the pane by
/// `theme::glass::PANE_GAP` (`task-2287`). Clipped to the pane itself, a node pushed against its edge was
/// drawn over the panel's rim and into the gap between two panels.
pub fn clip_for_nodes(pane: Rect, window: Rect, camera: &Camera) -> Rect {
    let inside = window.shrink(crate::components::resize_edges::EDGE);
    let panel = match crate::theme::is_glass() {
        true => pane.shrink(crate::theme::glass::PANE_GAP),
        false => pane,
    };
    let cut = panel.intersect(inside);
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
        crate::services::realm::State::Terminal(terminal) if terminal.font_size > 0.0 => {
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
        crate::services::realm::State::Editor(editor) if editor.font_size > 0.0 => editor.font_size,
        // A note is an editor on one Markdown file, and keeps its own size the same way. `task-2202`.
        crate::services::realm::State::Note(note) if note.font_size > 0.0 => note.font_size,
        _ => fallback,
    }
}

/// How much bigger or smaller than its usual size a folder node draws its rows.
pub fn folder_zoom_of(node: &Node) -> f32 {
    match &node.state {
        crate::services::realm::State::Folder(folder) if folder.zoom > 0.0 => folder.zoom,
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
    use crate::services::realm::{Realm, State};

    #[test]
    fn a_node_is_drawn_only_when_it_can_be_seen() {
        let pane = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        let camera = Camera::default();
        let mut realm = Realm::default();
        let near = realm.add_node(Kind::Folder, Pos2::new(100.0, 100.0), None);
        let far = realm.add_node(Kind::Folder, Pos2::new(9000.0, 9000.0), None);
        let view = realm;
        assert!(is_showing(view.node(near).expect("it is there"), pane, &camera));
        assert!(!is_showing(view.node(far).expect("it is there"), pane, &camera));
    }

    #[test]
    fn a_nodes_header_is_taken_out_of_its_top_and_never_out_of_more_than_it_has() {
        let mut realm = Realm::default();
        let id = realm.add_node(Kind::Terminal, Pos2::new(10.0, 20.0), None);
        realm.resize_node(id, Vec2::new(400.0, 300.0));
        let node = realm.node(id).expect("it is there").clone();
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
        let mut realm = Realm::default();
        let id = realm.add_node(Kind::Terminal, Pos2::ZERO, None);
        let node = realm.node(id).expect("it is there").clone();
        assert_eq!(font_size_of(&node, 14.0), 14.0, "the terminal's own setting");
        realm.change(id, |state| {
            if let State::Terminal(terminal) = state {
                terminal.font_size = 20.0;
            }
        });
        let node = realm.node(id).expect("it is there").clone();
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
        let mut realm = Realm::default();
        // A node well away from the canvas's origin, which is the case that showed the fault.
        let id = realm.add_node(Kind::Folder, Pos2::new(700.0, 460.0), None);
        let node = realm.node(id).expect("it is there").clone();
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
                    file_view: None,
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
        assert_eq!(pill_width_for(4.0), 58.0);
        assert_eq!(pill_width_for(0.0), 58.0);
        assert_eq!(pill_width_for(100.0), 128.0);
        assert!(pill_width_for(200.0) > pill_width_for(100.0));
    }
}
