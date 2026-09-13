//! The arithmetic the canvas is made of: the camera, the eight resize grips, and the curve a wire is.
//!
//! Every function here is pure and every one of them has a test, because all three are the kind of
//! thing that is wrong in a way nobody can see until they are dragging something. `unluminous-core`'s
//! layout tests are the same bargain: numbers a reader can check, on every machine.

use egui::{Pos2, Rect, Vec2};

use super::node::{Camera, MAX_ZOOM, MIN_ZOOM};

impl Camera {
    /// Where a world point is drawn, inside a pane whose top left corner is `origin`.
    pub fn to_screen(&self, origin: Pos2, world: Pos2) -> Pos2 {
        origin + (world - self.at) * self.zoom
    }

    /// The world point under a screen point. The inverse of [`Self::to_screen`].
    pub fn to_world(&self, origin: Pos2, screen: Pos2) -> Pos2 {
        self.at + (screen - origin) / self.zoom
    }

    /// A world rectangle as a screen one.
    pub fn rect_to_screen(&self, origin: Pos2, world: Rect) -> Rect {
        Rect::from_min_max(self.to_screen(origin, world.min), self.to_screen(origin, world.max))
    }

    /// Move the canvas by `by` **screen** points, which is what a drag gives.
    pub fn pan_by(&mut self, by: Vec2) {
        self.at -= by / self.zoom;
    }

    /// Zoom to `wanted`, keeping the world point that was under `pointer` under it.
    ///
    /// `task-1672`'s rule, which the editing area and the explorer already keep: the thing being
    /// looked at is what stays still. One subtraction, because the world point under a screen point
    /// is `at + (screen - origin) / zoom` and solving that for `at` at the new zoom is the whole of
    /// it.
    pub fn zoom_to(&mut self, wanted: f32, origin: Pos2, pointer: Pos2) {
        let under = self.to_world(origin, pointer);
        self.zoom = wanted.clamp(MIN_ZOOM, MAX_ZOOM);
        self.at = under - (pointer - origin) / self.zoom;
    }

    /// One notch of the wheel, or one press of the zoom keys.
    ///
    /// A factor rather than a step on a ladder, because a canvas is continuous where a font size is a
    /// list of sizes somebody chose from. 1.1 a notch is Chordical's own feel.
    pub fn zoom_by(&mut self, steps: i32, origin: Pos2, pointer: Pos2) {
        let wanted = self.zoom * 1.1_f32.powi(steps);
        self.zoom_to(wanted, origin, pointer);
    }

    /// Put every node in `bounds` on the screen at once, inside a pane of `size`.
    ///
    /// `padding` is how much room is left round the outside, in screen points. An empty canvas is
    /// left exactly as it was rather than being moved somewhere arbitrary, which is what happens if
    /// a zero sized bounding box is fitted.
    pub fn fit(&mut self, bounds: Option<Rect>, size: Vec2, padding: f32) {
        let Some(bounds) = bounds else { return };
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 || size.x <= 0.0 || size.y <= 0.0 {
            return;
        }
        let room = Vec2::new((size.x - padding * 2.0).max(1.0), (size.y - padding * 2.0).max(1.0));
        let zoom =
            (room.x / bounds.width()).min(room.y / bounds.height()).clamp(MIN_ZOOM, MAX_ZOOM);
        self.zoom = zoom;
        // The middle of what is showing lands on the middle of the pane.
        self.at = bounds.center() - size / (2.0 * zoom);
    }
}

/// Which part of a node's edge a drag took hold of.
///
/// Eight of them, which is what `task-1904` asks for by "resizable from any edge or side" and what
/// `components::resize_edges` already gives the window itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Grip {
    pub const ALL: [Grip; 8] = [
        Grip::TopLeft,
        Grip::TopRight,
        Grip::BottomLeft,
        Grip::BottomRight,
        Grip::Left,
        Grip::Right,
        Grip::Top,
        Grip::Bottom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Grip::Left => "left",
            Grip::Right => "right",
            Grip::Top => "top",
            Grip::Bottom => "bottom",
            Grip::TopLeft => "top left",
            Grip::TopRight => "top right",
            Grip::BottomLeft => "bottom left",
            Grip::BottomRight => "bottom right",
        }
    }

    fn holds_left(self) -> bool {
        matches!(self, Grip::Left | Grip::TopLeft | Grip::BottomLeft)
    }

    fn holds_right(self) -> bool {
        matches!(self, Grip::Right | Grip::TopRight | Grip::BottomRight)
    }

    fn holds_top(self) -> bool {
        matches!(self, Grip::Top | Grip::TopLeft | Grip::TopRight)
    }

    fn holds_bottom(self) -> bool {
        matches!(self, Grip::Bottom | Grip::BottomLeft | Grip::BottomRight)
    }

    /// What the pointer looks like over this grip.
    pub fn cursor(self) -> egui::CursorIcon {
        match self {
            Grip::Left | Grip::Right => egui::CursorIcon::ResizeHorizontal,
            Grip::Top | Grip::Bottom => egui::CursorIcon::ResizeVertical,
            Grip::TopLeft | Grip::BottomRight => egui::CursorIcon::ResizeNwSe,
            Grip::TopRight | Grip::BottomLeft => egui::CursorIcon::ResizeNeSw,
        }
    }
}

/// Which grip a screen point is over, when it is over one.
///
/// **The grip is measured in screen points and the rectangle is converted to reach it.** A canvas
/// zoomed out to a quarter would otherwise have grips a point and a half wide, which nobody can hit.
/// The corners are asked about before the edges, so a corner drag is a corner drag.
pub fn grip_at(screen_rect: Rect, at: Pos2, reach: f32) -> Option<Grip> {
    let outside = screen_rect.expand(reach);
    if !outside.contains(at) {
        return None;
    }
    let left = (at.x - screen_rect.left()).abs() <= reach;
    let right = (at.x - screen_rect.right()).abs() <= reach;
    let top = (at.y - screen_rect.top()).abs() <= reach;
    let bottom = (at.y - screen_rect.bottom()).abs() <= reach;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(Grip::TopLeft),
        (_, true, true, _) => Some(Grip::TopRight),
        (true, _, _, true) => Some(Grip::BottomLeft),
        (_, true, _, true) => Some(Grip::BottomRight),
        (true, ..) => Some(Grip::Left),
        (_, true, ..) => Some(Grip::Right),
        (_, _, true, _) => Some(Grip::Top),
        (_, _, _, true) => Some(Grip::Bottom),
        _ => None,
    }
}

/// The rectangle a node becomes when `grip` is dragged by `by` world points.
///
/// **Dragging the left edge moves the left edge and leaves the right one where it is**, which is the
/// case that is wrong in every implementation that keeps a position and a size and forgets one of
/// them. A drag past the smallest size stops the edge being dragged and leaves the other one alone
/// rather than turning the rectangle inside out.
pub fn resized(rect: Rect, grip: Grip, by: Vec2, smallest: Vec2) -> Rect {
    let mut left = rect.left();
    let mut right = rect.right();
    let mut top = rect.top();
    let mut bottom = rect.bottom();
    if grip.holds_left() {
        left = (left + by.x).min(right - smallest.x);
    }
    if grip.holds_right() {
        right = (right + by.x).max(left + smallest.x);
    }
    if grip.holds_top() {
        top = (top + by.y).min(bottom - smallest.y);
    }
    if grip.holds_bottom() {
        bottom = (bottom + by.y).max(top + smallest.y);
    }
    Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, bottom))
}

/// How far out the control points of a wire's curve reach.
///
/// Two cases, and they are genuinely different shapes rather than one formula with a floor on it.
///
/// **Forward**, the reach is half the gap and never more, so the two handles cannot pass each other:
/// a floor larger than half the gap is what makes a wire between two nodes that are nearly touching
/// double back on itself in a small S, which is exactly what the first canvas screenshot showed. A
/// small floor is still kept, or two nodes at the same height and touching get a dead straight line
/// with no shape to follow.
///
/// **Backward**, the target is to the left of the source and there is no gap to halve. The reach has
/// to be large enough for the curve to bow out round the two nodes, so it grows with the distance and
/// starts well clear of zero — which is the case a plain "half the gap" reaches zero on and inverts.
fn reach_of(from: Pos2, to: Pos2) -> f32 {
    const LEAST: f32 = 8.0;
    const BACKWARD: f32 = 60.0;
    const MOST: f32 = 220.0;
    let across = to.x - from.x;
    match across >= 0.0 {
        true => (across * 0.5).clamp(LEAST, MOST),
        false => (across.abs() * 0.5 + BACKWARD).min(MOST),
    }
}

/// The four control points of the cubic Bézier a wire is drawn along.
///
/// Horizontal handles, which is what makes a wire leave an output port going right and arrive at an
/// input port going right — the shape every node editor draws and the one Chordical's `getBezierPath`
/// produces.
pub fn curve(from: Pos2, to: Pos2) -> [Pos2; 4] {
    let reach = reach_of(from, to);
    [from, Pos2::new(from.x + reach, from.y), Pos2::new(to.x - reach, to.y), to]
}

/// The curve as a polyline of `segments` straight pieces.
///
/// `vello_canvas` draws a line rather than a path, and a cubic sampled at a few dozen points is
/// indistinguishable from one at the sizes a node is drawn at. It is also what makes the wire cost a
/// fixed number of points however long it is.
pub fn curve_points(from: Pos2, to: Pos2, segments: usize) -> Vec<Pos2> {
    let [a, b, c, d] = curve(from, to);
    let segments = segments.max(1);
    (0..=segments)
        .map(|step| {
            let t = step as f32 / segments as f32;
            let u = 1.0 - t;
            let x =
                u * u * u * a.x + 3.0 * u * u * t * b.x + 3.0 * u * t * t * c.x + t * t * t * d.x;
            let y =
                u * u * u * a.y + 3.0 * u * u * t * b.y + 3.0 * u * t * t * c.y + t * t * t * d.y;
            Pos2::new(x, y)
        })
        .collect()
}

/// How near a point is to the curve, in the units the points are in.
///
/// What a right click on a wire is answered with. The curve is walked as the polyline it is drawn as,
/// so what is asked about is exactly what is on the screen.
pub fn distance_to_curve(from: Pos2, to: Pos2, at: Pos2) -> f32 {
    let points = curve_points(from, to, 32);
    points
        .windows(2)
        .map(|pair| distance_to_segment(pair[0], pair[1], at))
        .fold(f32::INFINITY, f32::min)
}

/// How far `at` is from the straight piece between `a` and `b`.
fn distance_to_segment(a: Pos2, b: Pos2, at: Pos2) -> f32 {
    let along = b - a;
    let length = along.length_sq();
    if length <= f32::EPSILON {
        return (at - a).length();
    }
    let t = (((at - a).dot(along)) / length).clamp(0.0, 1.0);
    (at - (a + along * t)).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: Pos2 = Pos2::new(40.0, 90.0);

    #[test]
    fn a_world_point_survives_the_round_trip_at_every_zoom() {
        // The two functions are inverses of each other, and a canvas where they are not is a canvas
        // where a click lands somewhere other than where it was aimed.
        for zoom in [MIN_ZOOM, 0.5, 0.75, 1.0, 1.5, 2.0, MAX_ZOOM] {
            let camera = Camera { at: Pos2::new(-120.5, 340.25), zoom };
            for world in [Pos2::ZERO, Pos2::new(1000.0, -500.0), Pos2::new(-3.25, 7.75)] {
                let there = camera.to_screen(ORIGIN, world);
                let back = camera.to_world(ORIGIN, there);
                assert!(
                    (back - world).length() < 0.01,
                    "{world:?} at {zoom} came back as {back:?}"
                );
            }
        }
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer_under_the_pointer() {
        // `task-1672`'s rule. The point being looked at is what stays still, whatever the zoom does.
        let mut camera = Camera { at: Pos2::new(10.0, 20.0), zoom: 1.0 };
        let pointer = Pos2::new(300.0, 400.0);
        let before = camera.to_world(ORIGIN, pointer);
        for steps in [1, 1, 1, -1, -5, 12] {
            camera.zoom_by(steps, ORIGIN, pointer);
            let after = camera.to_world(ORIGIN, pointer);
            assert!(
                (after - before).length() < 0.01,
                "{steps} steps moved {before:?} to {after:?}"
            );
        }
    }

    #[test]
    fn the_zoom_is_clamped_to_the_ladders_ends_however_hard_it_is_pushed() {
        let mut camera = Camera::default();
        camera.zoom_by(100, ORIGIN, Pos2::ZERO);
        assert_eq!(camera.zoom, MAX_ZOOM);
        camera.zoom_by(-100, ORIGIN, Pos2::ZERO);
        assert_eq!(camera.zoom, MIN_ZOOM);
    }

    #[test]
    fn panning_moves_the_canvas_by_the_screen_points_it_was_dragged() {
        // A drag of ten screen points moves the canvas ten screen points whatever the zoom, which is
        // what "the thing under my finger came with me" means.
        for zoom in [0.25, 1.0, 2.5] {
            let mut camera = Camera { at: Pos2::ZERO, zoom };
            let held = camera.to_world(ORIGIN, Pos2::new(200.0, 200.0));
            camera.pan_by(Vec2::new(10.0, -6.0));
            let now = camera.to_world(ORIGIN, Pos2::new(210.0, 194.0));
            assert!((now - held).length() < 0.01);
        }
    }

    #[test]
    fn fitting_puts_everything_on_the_screen_and_leaves_an_empty_canvas_alone() {
        let mut camera = Camera { at: Pos2::new(7.0, 9.0), zoom: 1.7 };
        let was = camera;
        camera.fit(None, Vec2::new(800.0, 600.0), 24.0);
        assert_eq!(camera, was, "there was nothing to fit");

        let bounds = Rect::from_min_max(Pos2::new(-200.0, -100.0), Pos2::new(600.0, 500.0));
        camera.fit(Some(bounds), Vec2::new(800.0, 600.0), 24.0);
        let top_left = camera.to_screen(ORIGIN, bounds.min);
        let bottom_right = camera.to_screen(ORIGIN, bounds.max);
        assert!(top_left.x >= ORIGIN.x - 0.5 && top_left.y >= ORIGIN.y - 0.5);
        assert!(bottom_right.x <= ORIGIN.x + 800.5 && bottom_right.y <= ORIGIN.y + 600.5);
    }

    #[test]
    fn dragging_one_edge_leaves_the_other_three_exactly_where_they_were() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 260.0));
        let smallest = Vec2::new(40.0, 30.0);

        let left = resized(rect, Grip::Left, Vec2::new(-25.0, 90.0), smallest);
        assert_eq!(left.min, Pos2::new(75.0, 100.0), "only the left edge moved");
        assert_eq!(left.max, rect.max);

        let bottom = resized(rect, Grip::Bottom, Vec2::new(-40.0, 12.0), smallest);
        assert_eq!(bottom.min, rect.min);
        assert_eq!(bottom.max, Pos2::new(300.0, 272.0), "only the bottom edge moved");

        let corner = resized(rect, Grip::TopRight, Vec2::new(30.0, -20.0), smallest);
        assert_eq!(corner.min, Pos2::new(100.0, 80.0));
        assert_eq!(corner.max, Pos2::new(330.0, 260.0));
    }

    #[test]
    fn a_resize_past_the_smallest_size_stops_rather_than_turning_the_node_inside_out() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 120.0));
        let smallest = Vec2::new(60.0, 40.0);
        let squashed = resized(rect, Grip::Left, Vec2::new(1000.0, 0.0), smallest);
        assert_eq!(squashed.width(), smallest.x);
        assert_eq!(squashed.right(), rect.right(), "the edge that was not dragged did not move");
        let flattened = resized(rect, Grip::Top, Vec2::new(0.0, 1000.0), smallest);
        assert_eq!(flattened.height(), smallest.y);
        assert_eq!(flattened.bottom(), rect.bottom());
    }

    #[test]
    fn a_corner_is_found_before_an_edge() {
        let rect = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 260.0));
        assert_eq!(grip_at(rect, Pos2::new(101.0, 101.0), 6.0), Some(Grip::TopLeft));
        assert_eq!(grip_at(rect, Pos2::new(299.0, 259.0), 6.0), Some(Grip::BottomRight));
        assert_eq!(grip_at(rect, Pos2::new(100.0, 180.0), 6.0), Some(Grip::Left));
        assert_eq!(grip_at(rect, Pos2::new(200.0, 260.0), 6.0), Some(Grip::Bottom));
        assert_eq!(grip_at(rect, Pos2::new(200.0, 180.0), 6.0), None, "the middle is not a grip");
        assert_eq!(grip_at(rect, Pos2::new(500.0, 180.0), 6.0), None);
        for grip in Grip::ALL {
            assert!(!grip.name().is_empty());
        }
    }

    #[test]
    fn a_wire_between_two_nodes_that_are_nearly_touching_does_not_double_back() {
        // With the reach floored above half the gap the two handles pass each other and the curve
        // folds into a small S, which is what the first picture of the canvas showed.
        let from = Pos2::new(300.0, 100.0);
        let to = Pos2::new(322.0, 150.0);
        let [a, b, c, d] = curve(from, to);
        assert!(b.x <= c.x, "the handles must not cross: {b:?} and {c:?}");
        assert!(b.x >= a.x && c.x <= d.x);
        // And the curve stays inside the two ends horizontally, so it reads as one arc.
        for point in curve_points(from, to, 24) {
            assert!(point.x >= from.x - 0.01 && point.x <= to.x + 0.01, "{point:?} is outside");
        }
    }

    #[test]
    fn a_wire_curves_the_same_way_whichever_side_the_target_is_on() {
        // The fault this is about: with the control point offset set to half the gap, a target to the
        // **left** of the source gets a negative reach and the curve inverts into a spike.
        let from = Pos2::new(300.0, 100.0);
        for to in [Pos2::new(600.0, 100.0), Pos2::new(100.0, 300.0), Pos2::new(300.0, 400.0)] {
            let [a, b, c, d] = curve(from, to);
            assert_eq!(a, from);
            assert_eq!(d, to);
            assert!(b.x > a.x, "the wire leaves the output port going right: {to:?}");
            assert!(c.x < d.x, "and arrives at the input port going right: {to:?}");
            assert_eq!(b.y, a.y, "the handles are horizontal");
            assert_eq!(c.y, d.y);
        }
    }

    #[test]
    fn the_polyline_starts_and_ends_on_the_ports_and_the_distance_reads_zero_on_it() {
        let from = Pos2::new(0.0, 0.0);
        let to = Pos2::new(400.0, 200.0);
        let points = curve_points(from, to, 24);
        assert_eq!(points.len(), 25);
        assert_eq!(points[0], from);
        assert_eq!(points[24], to);
        for point in &points {
            assert!(distance_to_curve(from, to, *point) < 1.0);
        }
        assert!(distance_to_curve(from, to, Pos2::new(200.0, -400.0)) > 100.0);
    }
}
