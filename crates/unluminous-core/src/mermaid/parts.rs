//! The furniture a diagram is built from.
//!
//! `components::modal` is the same idea one layer up: the frame, the header and the buttons live in
//! one file so that a tenth modal is not a tenth answer to what a header looks like. This is that,
//! for diagrams. A box with words in it, an arrowhead, where a line meets the edge of a shape, an
//! axis, a legend, a title — twenty renderers want all of those, and each of them should mean the
//! same thing in all twenty.
//!
//! Everything here produces [`Item`]s and nothing here knows what a flowchart is.

use crate::metrics::FontMetrics;
use crate::style::{CharStyle, Color};

use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke, TextStyle};
use super::text::{self, Label};
use super::Options;

/// How thick an ordinary line is.
pub const LINE: f32 = 1.5;
/// How thick a line Mermaid calls thick is: `==>` in a flowchart, a critical task in a gantt.
pub const THICK: f32 = 3.0;
/// The marks and gaps of a dashed line.
pub const DASH: Dash = Dash::Dashed(6.0, 4.0);
/// How long an arrowhead is, from its tip back to its base.
pub const HEAD: f32 = 10.0;
/// How wide an arrowhead is across its base.
pub const HEAD_WIDTH: f32 = 8.0;
/// Space left round the whole diagram, so nothing touches the edge of the pane.
pub const MARGIN: f32 = 16.0;
/// Space between a box's border and the words inside it, across and down.
pub const PADDING_X: f32 = 12.0;
pub const PADDING_Y: f32 = 8.0;
/// A node's rounded corner, where it has one.
pub const CORNER: f32 = 5.0;

/// A shape's outline, which is what says where a line touching it should stop.
///
/// Three kinds rather than one, because a rectangle and a circle have exact answers that are worth
/// having: going through the polygon intersection for every edge of a hundred-node flowchart would
/// be arithmetic spent on a worse answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Outline {
    Rect(Rect),
    Circle(Point, f32),
    Polygon(Vec<Point>),
}

impl Outline {
    pub fn centre(&self) -> Point {
        match self {
            Outline::Rect(rect) => rect.centre(),
            Outline::Circle(centre, _) => *centre,
            Outline::Polygon(points) => {
                let count = points.len().max(1) as f32;
                Point::new(
                    points.iter().map(|p| p.x).sum::<f32>() / count,
                    points.iter().map(|p| p.y).sum::<f32>() / count,
                )
            }
        }
    }

    /// Where a line from the centre towards `target` leaves this shape.
    ///
    /// This is what stops an arrow being drawn under the box it points at: the line is cut where it
    /// meets the border, so the arrowhead sits against the edge rather than in the middle of the
    /// words.
    pub fn border_towards(&self, target: Point) -> Point {
        let centre = self.centre();
        let dx = target.x - centre.x;
        let dy = target.y - centre.y;
        if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
            return centre;
        }
        match self {
            Outline::Circle(_, radius) => {
                let length = (dx * dx + dy * dy).sqrt();
                Point::new(centre.x + dx / length * radius, centre.y + dy / length * radius)
            }
            Outline::Rect(rect) => {
                // The smallest step along the ray that reaches either the vertical or the horizontal
                // edge. Whichever is smaller is the one the ray actually crosses.
                let across = if dx.abs() < f32::EPSILON {
                    f32::INFINITY
                } else {
                    (rect.width / 2.0) / dx.abs()
                };
                let down = if dy.abs() < f32::EPSILON {
                    f32::INFINITY
                } else {
                    (rect.height / 2.0) / dy.abs()
                };
                let step = across.min(down);
                Point::new(centre.x + dx * step, centre.y + dy * step)
            }
            Outline::Polygon(points) => {
                polygon_border(points, centre, Point::new(dx, dy)).unwrap_or(centre)
            }
        }
    }
}

impl Outline {
    /// Whether `point` is inside the shape or on its border.
    pub fn contains(&self, point: Point) -> bool {
        match self {
            Outline::Rect(rect) => rect.contains(point),
            Outline::Circle(centre, radius) => centre.distance(point) <= *radius + 0.01,
            Outline::Polygon(points) => {
                let mut inside = false;
                for index in 0..points.len() {
                    let a = points[index];
                    let b = points[(index + 1) % points.len()];
                    if (a.y > point.y) != (b.y > point.y) {
                        let x = a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x);
                        if point.x < x {
                            inside = !inside;
                        }
                    }
                }
                inside
            }
        }
    }

    /// Where the straight line from `inside`, in the shape, to `outside`, out of it, crosses the
    /// border. Found by halving the line twenty times, which is a ten-thousandth of a point on any
    /// line a diagram draws and works the same for every one of the three kinds.
    fn crossing(&self, inside: Point, outside: Point) -> Point {
        let (mut near, mut far) = (0.0_f32, 1.0_f32);
        for _ in 0..20 {
            let middle = (near + far) / 2.0;
            if self.contains(inside.towards(outside, middle)) {
                near = middle;
            } else {
                far = middle;
            }
        }
        inside.towards(outside, (near + far) / 2.0)
    }
}

/// Where the ray from `from` in direction `along` crosses the polygon, furthest out.
fn polygon_border(points: &[Point], from: Point, along: Point) -> Option<Point> {
    let mut best: Option<(f32, Point)> = None;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        let Some((step, at)) = ray_meets_segment(from, along, a, b) else {
            continue;
        };
        if best.is_none_or(|(known, _)| step > known) {
            best = Some((step, at));
        }
    }
    best.map(|(_, at)| at)
}

/// Solve `from + step * along` against the segment `a`..`b`, for a `step` of zero or more.
fn ray_meets_segment(from: Point, along: Point, a: Point, b: Point) -> Option<(f32, Point)> {
    let edge = Point::new(b.x - a.x, b.y - a.y);
    let denominator = along.x * edge.y - along.y * edge.x;
    if denominator.abs() < 1e-6 {
        return None;
    }
    let offset = Point::new(a.x - from.x, a.y - from.y);
    let step = (offset.x * edge.y - offset.y * edge.x) / denominator;
    let along_edge = (offset.x * along.y - offset.y * along.x) / denominator;
    if step < 0.0 || !(0.0..=1.0).contains(&along_edge) {
        return None;
    }
    Some((step, Point::new(from.x + along.x * step, from.y + along.y * step)))
}

/// Draw `label`'s lines centred inside `rect`.
pub fn centred_label(scene: &mut Scene, label: &Label, rect: Rect, style: &TextStyle) {
    let top = rect.centre().y - label.height / 2.0;
    for (index, line) in label.lines.iter().enumerate() {
        scene.add(Item::Text {
            at: Point::new(rect.centre().x, top + label.line_height * index as f32),
            text: line.clone(),
            style: style.clone(),
            anchor: Anchor::Middle,
        });
    }
    scene.claim(rect);
}

/// Draw `label`'s lines starting at `at`, growing downwards, anchored as asked.
pub fn label_at(scene: &mut Scene, label: &Label, at: Point, style: &TextStyle, anchor: Anchor) {
    for (index, line) in label.lines.iter().enumerate() {
        scene.add(Item::Text {
            at: Point::new(at.x, at.y + label.line_height * index as f32),
            text: line.clone(),
            style: style.clone(),
            anchor,
        });
    }
    let width = label.width;
    let left = match anchor {
        Anchor::Start => at.x,
        Anchor::Middle => at.x - width / 2.0,
        Anchor::End => at.x - width,
    };
    scene.claim(Rect::new(left, at.y, width, label.height));
}

/// One line of text at a point, which is the common case.
pub fn one_line(
    scene: &mut Scene,
    words: &str,
    at: Point,
    style: &TextStyle,
    anchor: Anchor,
    width: f32,
) {
    scene.add(Item::Text { at, text: words.to_owned(), style: style.clone(), anchor });
    let left = match anchor {
        Anchor::Start => at.x,
        Anchor::Middle => at.x - width / 2.0,
        Anchor::End => at.x - width,
    };
    scene.claim(Rect::new(left, at.y, width, style.size * 1.4));
}

/// The triangle of an arrowhead whose tip is at `tip`, pointing along `direction`.
///
/// `direction` need not be a unit vector; a zero one gives a head pointing left, which is only ever
/// reached by an edge whose two ends are in the same place.
pub fn arrow_head(tip: Point, direction: Point, size: f32) -> Vec<Point> {
    let length = (direction.x * direction.x + direction.y * direction.y).sqrt();
    let (dx, dy) = if length < f32::EPSILON {
        (-1.0, 0.0)
    } else {
        (direction.x / length, direction.y / length)
    };
    let base = Point::new(tip.x - dx * size, tip.y - dy * size);
    let half = size * (HEAD_WIDTH / HEAD) / 2.0;
    vec![
        tip,
        Point::new(base.x - dy * half, base.y + dx * half),
        Point::new(base.x + dy * half, base.y - dx * half),
    ]
}

/// How one end of an edge is finished off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ending {
    /// Nothing at all: an open link.
    #[default]
    None,
    /// A filled triangle.
    Arrow,
    /// An unfilled triangle, which is what inheritance and realisation use.
    Hollow,
    /// A filled diamond: composition.
    Diamond,
    /// An unfilled diamond: aggregation.
    HollowDiamond,
    /// A small circle, which a flowchart's `--o` draws.
    Circle,
    /// A cross, which a flowchart's `--x` draws.
    Cross,
}

/// Draw whatever `ending` is at `tip`, pointing along `direction`.
pub fn ending(
    scene: &mut Scene,
    ending: Ending,
    tip: Point,
    direction: Point,
    colour: Color,
    fill: Paint,
) {
    let stroke = Stroke::new(colour, LINE);
    match ending {
        Ending::None => {}
        Ending::Arrow => scene.add(Item::Polygon {
            points: arrow_head(tip, direction, HEAD),
            fill: Some(Paint::solid(colour)),
            stroke: None,
        }),
        Ending::Hollow => scene.add(Item::Polygon {
            points: arrow_head(tip, direction, HEAD + 2.0),
            fill: Some(fill),
            stroke: Some(stroke),
        }),
        Ending::Diamond | Ending::HollowDiamond => {
            let filled = ending == Ending::Diamond;
            scene.add(Item::Polygon {
                points: diamond_head(tip, direction, HEAD + 4.0),
                fill: Some(if filled { Paint::solid(colour) } else { fill }),
                stroke: Some(stroke),
            });
        }
        Ending::Circle => scene.add(Item::Circle {
            centre: back_from(tip, direction, HEAD / 2.0),
            radius: HEAD / 2.0,
            fill: Some(fill),
            stroke: Some(stroke),
        }),
        Ending::Cross => {
            let centre = back_from(tip, direction, HEAD / 2.0);
            let arm = HEAD / 2.0;
            for (dx, dy) in [(1.0, 1.0), (1.0, -1.0)] {
                scene.add(Item::Line {
                    points: vec![
                        Point::new(centre.x - arm * dx, centre.y - arm * dy),
                        Point::new(centre.x + arm * dx, centre.y + arm * dy),
                    ],
                    stroke,
                    dash: Dash::Solid,
                });
            }
        }
    }
}

/// How far back along an edge its line should stop, so it does not show through its own ending.
pub fn ending_inset(ending: Ending) -> f32 {
    match ending {
        Ending::None => 0.0,
        Ending::Arrow => HEAD * 0.9,
        Ending::Hollow => HEAD + 1.0,
        Ending::Diamond | Ending::HollowDiamond => HEAD + 4.0,
        Ending::Circle | Ending::Cross => HEAD,
    }
}

/// A point `distance` back from `tip` against `direction`.
fn back_from(tip: Point, direction: Point, distance: f32) -> Point {
    let length = (direction.x * direction.x + direction.y * direction.y).sqrt();
    if length < f32::EPSILON {
        return tip;
    }
    Point::new(tip.x - direction.x / length * distance, tip.y - direction.y / length * distance)
}

/// The four points of a diamond whose forward point is at `tip`.
fn diamond_head(tip: Point, direction: Point, size: f32) -> Vec<Point> {
    let length = (direction.x * direction.x + direction.y * direction.y).sqrt();
    let (dx, dy) = if length < f32::EPSILON {
        (-1.0, 0.0)
    } else {
        (direction.x / length, direction.y / length)
    };
    let half = size * 0.32;
    let middle = Point::new(tip.x - dx * size / 2.0, tip.y - dy * size / 2.0);
    vec![
        tip,
        Point::new(middle.x - dy * half, middle.y + dx * half),
        Point::new(tip.x - dx * size, tip.y - dy * size),
        Point::new(middle.x + dy * half, middle.y - dx * half),
    ]
}

/// How far round a corner an edge's bend begins: the radius the corner is rounded to.
pub const BEND: f32 = 12.0;

/// An edge's line from the layered layout, finished: cut back to the borders of the two shapes it
/// joins, and its corners rounded.
///
/// `task-2194` asked for arrows that are slightly rounded where they bend. The cut comes first, so the
/// rounding is measured on the part of the line that is actually drawn, and the last stretch before
/// each shape is kept straight for at least an arrowhead's length so the head points the way the line
/// arrives rather than along a curve.
pub fn edge_path(path: &[Point], start: &Outline, end: &Outline) -> Vec<Point> {
    rounded(&clipped(path, start, end), BEND, HEAD)
}

/// Cut a line back to where it leaves `start` and where it reaches `end`.
///
/// Wherever along the line that happens, rather than only on its first and last segment: an edge
/// that starts at a point inside a node beside its middle runs straight down out of it, and the first
/// point outside the shape is not always the second point of the line.
pub fn clipped(path: &[Point], start: &Outline, end: &Outline) -> Vec<Point> {
    if path.len() < 2 {
        return path.to_vec();
    }
    let mut points = path.to_vec();
    if start.contains(points[0]) {
        if let Some(leaving) = (0..points.len() - 1).find(|&at| !start.contains(points[at + 1])) {
            let cross = start.crossing(points[leaving], points[leaving + 1]);
            points.drain(..=leaving);
            points.insert(0, cross);
        }
    }
    let last = points.len() - 1;
    if points.len() >= 2 && end.contains(points[last]) {
        if let Some(entering) = (1..points.len()).rev().find(|&at| !end.contains(points[at - 1])) {
            let cross = end.crossing(points[entering], points[entering - 1]);
            points.truncate(entering);
            points.push(cross);
        }
    }
    points
}

/// The same line with each corner replaced by a short curve of radius up to `radius`.
///
/// A corner takes at most half of each segment beside it, so two corners never overlap, and the first
/// and last segments keep at least `straight` of their length uncurved. The curve is a quadratic
/// Bézier with the corner as its control point, flattened into a few segments, because the painter
/// draws lines and nothing else.
pub fn rounded(points: &[Point], radius: f32, straight: f32) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let last = points.len() - 1;
    let mut out = vec![points[0]];
    for at in 1..last {
        let (before, corner, after) = (points[at - 1], points[at], points[at + 1]);
        let incoming = before.distance(corner);
        let outgoing = corner.distance(after);
        // An end segment shorter than `straight` still gives up half of itself, so a short one is a
        // gentler corner rather than a sharp one.
        let room_in =
            if at == 1 { (incoming - straight).max(incoming / 2.0) } else { incoming / 2.0 };
        let room_out =
            if at + 1 == last { (outgoing - straight).max(outgoing / 2.0) } else { outgoing / 2.0 };
        let cut = radius.min(room_in).min(room_out);
        if cut < 1.0 || incoming < f32::EPSILON || outgoing < f32::EPSILON {
            out.push(corner);
            continue;
        }
        let enter = corner.towards(before, cut / incoming);
        let leave = corner.towards(after, cut / outgoing);
        const STEPS: usize = 6;
        for step in 0..=STEPS {
            let t = step as f32 / STEPS as f32;
            let first = enter.towards(corner, t);
            let second = corner.towards(leave, t);
            out.push(first.towards(second, t));
        }
    }
    out.push(points[last]);
    out
}

/// Shorten a polyline at each end, so the line stops where its endings begin.
///
/// Measured along the line, so a shortening longer than the last segment carries on into the one
/// before it rather than being skipped. A line shorter than both shortenings together is left alone.
pub fn trimmed(points: &[Point], start: f32, end: f32) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let total: f32 = points.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
    if start + end >= total {
        return points.to_vec();
    }
    let mut points = cut_front(points, start);
    points.reverse();
    let mut points = cut_front(&points, end);
    points.reverse();
    points
}

/// The line with `distance` taken off its start.
fn cut_front(points: &[Point], distance: f32) -> Vec<Point> {
    if distance <= 0.0 {
        return points.to_vec();
    }
    let mut walked = 0.0;
    for at in 0..points.len() - 1 {
        let length = points[at].distance(points[at + 1]);
        if walked + length > distance {
            let mut out = vec![points[at].towards(points[at + 1], (distance - walked) / length)];
            out.extend_from_slice(&points[at + 1..]);
            return out;
        }
        walked += length;
    }
    points.to_vec()
}

/// The direction the last segment of a polyline is going in.
pub fn heading(points: &[Point]) -> Point {
    if points.len() < 2 {
        return Point::new(1.0, 0.0);
    }
    let last = points[points.len() - 1];
    let before = points[points.len() - 2];
    Point::new(last.x - before.x, last.y - before.y)
}

/// The direction the first segment of a polyline came from, pointing backwards out of it.
pub fn tail_heading(points: &[Point]) -> Point {
    if points.len() < 2 {
        return Point::new(-1.0, 0.0);
    }
    Point::new(points[0].x - points[1].x, points[0].y - points[1].y)
}

/// Flatten a circular arc into points, for a pie slice or a rounded join.
///
/// Angles are in radians, clockwise from twelve o'clock, which is the direction a pie is read in.
pub fn arc(centre: Point, radius: f32, from: f32, to: f32, steps: usize) -> Vec<Point> {
    let steps = steps.max(2);
    (0..=steps)
        .map(|step| {
            let angle = from + (to - from) * step as f32 / steps as f32;
            Point::new(centre.x + radius * angle.sin(), centre.y - radius * angle.cos())
        })
        .collect()
}

/// How many segments an arc of this size should be flattened into.
///
/// One every four degrees, which is under half a point of error on a two hundred point radius and is
/// far below what anybody can see.
pub fn arc_steps(sweep: f32) -> usize {
    ((sweep.abs() / (std::f32::consts::PI / 45.0)).ceil() as usize).clamp(2, 360)
}

/// A box with a border and a label in it, sized to the words.
pub struct Boxed {
    pub rect: Rect,
    pub label: Label,
}

/// Measure a box big enough for `words`, with the usual padding and a smallest size.
pub fn measure_box(
    words: &str,
    style: &CharStyle,
    metrics: &dyn FontMetrics,
    smallest: Size,
) -> Boxed {
    let label = text::measure(words, style, metrics, text::WRAP);
    let width = (label.width + PADDING_X * 2.0).max(smallest.width);
    let height = (label.height + PADDING_Y * 2.0).max(smallest.height);
    Boxed { rect: Rect::new(0.0, 0.0, width, height), label }
}

/// The text style for a diagram's ordinary label text.
pub fn text_style(options: &Options, scale: f32, bold: bool, colour: Color) -> TextStyle {
    TextStyle {
        family: options.base.family.to_string(),
        size: options.base.size * scale,
        bold,
        italic: false,
        color: colour,
    }
}

/// Draw a title across the top of a diagram and say how much room it took.
///
/// Returns zero when there is no title, so a caller can add it to its own top margin without asking
/// whether there was one.
pub fn title(scene: &mut Scene, source: &super::Source, options: &Options, width: f32) -> f32 {
    let Some(words) = source.title.as_deref().filter(|words| !words.trim().is_empty()) else {
        return 0.0;
    };
    let style = options.style(1.25, true);
    let label = text::measure_unwrapped(words, &style, options.metrics);
    let at = Point::new((width / 2.0).max(label.width / 2.0), MARGIN);
    one_line(
        scene,
        words,
        at,
        &text_style(options, 1.25, true, options.theme.text),
        Anchor::Middle,
        label.width,
    );
    label.height + MARGIN
}

/// Draw a legend of coloured swatches with names beside them, down the right of a chart.
///
/// Returns how wide it is, so the caller can leave room for it.
pub fn legend(scene: &mut Scene, entries: &[(String, Color)], at: Point, options: &Options) -> f32 {
    let style = options.style(0.9, false);
    let text = text_style(options, 0.9, false, options.theme.text);
    let swatch = style.size * 0.9;
    let row = style.size * 1.9;
    let mut widest: f32 = 0.0;
    for (index, (name, colour)) in entries.iter().enumerate() {
        let y = at.y + row * index as f32;
        scene.add(Item::Rect {
            rect: Rect::new(at.x, y, swatch, swatch),
            radius: 2.0,
            fill: Some(Paint::solid(*colour)),
            stroke: None,
        });
        let width = text::width_of(name, &style, options.metrics);
        widest = widest.max(width);
        one_line(
            scene,
            name,
            Point::new(at.x + swatch + 8.0, y - 1.0),
            &text,
            Anchor::Start,
            width,
        );
    }
    swatch + 8.0 + widest
}

/// Grow the scene by a margin on the right and the bottom, so nothing sits against the edge.
pub fn finish(scene: &mut Scene) {
    scene.size.width += MARGIN;
    scene.size.height += MARGIN;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::FixedMetrics;

    #[test]
    fn a_line_stops_on_a_rectangles_edge_rather_than_in_its_middle() {
        let rect = Rect::new(0.0, 0.0, 100.0, 40.0);
        let outline = Outline::Rect(rect);
        // Straight to the right: it leaves through the right edge, halfway down.
        assert_eq!(outline.border_towards(Point::new(500.0, 20.0)), Point::new(100.0, 20.0));
        // Straight down: through the bottom edge, halfway across.
        assert_eq!(outline.border_towards(Point::new(50.0, 500.0)), Point::new(50.0, 40.0));
    }

    #[test]
    fn a_line_to_a_circle_stops_one_radius_out() {
        let outline = Outline::Circle(Point::new(50.0, 50.0), 20.0);
        let at = outline.border_towards(Point::new(150.0, 50.0));
        assert_eq!(at, Point::new(70.0, 50.0));
        let diagonal = outline.border_towards(Point::new(150.0, 150.0));
        assert!((diagonal.distance(Point::new(50.0, 50.0)) - 20.0).abs() < 0.01);
    }

    #[test]
    fn a_line_to_a_diamond_stops_on_its_sloping_side() {
        // A diamond a hundred across and forty down, centred on (50, 20). Going right, its point is
        // at x = 100; going diagonally it must stop well inside that.
        let diamond = vec![
            Point::new(50.0, 0.0),
            Point::new(100.0, 20.0),
            Point::new(50.0, 40.0),
            Point::new(0.0, 20.0),
        ];
        let outline = Outline::Polygon(diamond);
        assert_eq!(outline.border_towards(Point::new(400.0, 20.0)), Point::new(100.0, 20.0));
        let corner = outline.border_towards(Point::new(150.0, 120.0));
        assert!(corner.x < 100.0 && corner.y < 40.0, "it stops on the slope, at {corner:?}");
    }

    #[test]
    fn an_arrowhead_points_the_way_it_is_going() {
        let head = arrow_head(Point::new(100.0, 50.0), Point::new(1.0, 0.0), 10.0);
        assert_eq!(head[0], Point::new(100.0, 50.0), "the tip is where it was asked for");
        // The other two are behind the tip, one either side.
        assert!(head[1].x < 100.0 && head[2].x < 100.0);
        assert!((head[1].y - 50.0).signum() != (head[2].y - 50.0).signum());
    }

    #[test]
    fn an_arrowhead_with_no_direction_still_produces_a_triangle() {
        // Only reached by an edge whose two ends are in the same place, which a self-loop is before
        // it is routed. It must not produce NaN, because a NaN point poisons the scene's size.
        let head = arrow_head(Point::new(10.0, 10.0), Point::new(0.0, 0.0), 10.0);
        assert_eq!(head.len(), 3);
        assert!(head.iter().all(|point| point.x.is_finite() && point.y.is_finite()));
    }

    #[test]
    fn trimming_shortens_a_line_at_both_ends_without_moving_the_middle() {
        let points = vec![Point::new(0.0, 0.0), Point::new(50.0, 0.0), Point::new(100.0, 0.0)];
        let trimmed = trimmed(&points, 10.0, 20.0);
        assert_eq!(trimmed[0], Point::new(10.0, 0.0));
        assert_eq!(trimmed[1], Point::new(50.0, 0.0));
        assert_eq!(trimmed[2], Point::new(80.0, 0.0));
    }

    #[test]
    fn trimming_a_segment_shorter_than_the_trim_leaves_it_alone() {
        // Otherwise the line turns back on itself, which draws an arrow pointing the wrong way.
        let points = vec![Point::new(0.0, 0.0), Point::new(4.0, 0.0)];
        assert_eq!(trimmed(&points, 10.0, 10.0), points);
    }

    #[test]
    fn an_arc_starts_at_twelve_o_clock_and_goes_clockwise() {
        let points = arc(Point::new(0.0, 0.0), 10.0, 0.0, std::f32::consts::FRAC_PI_2, 8);
        assert!(
            (points[0].x).abs() < 0.001 && (points[0].y + 10.0).abs() < 0.001,
            "starts at the top"
        );
        let last = points[points.len() - 1];
        assert!(
            (last.x - 10.0).abs() < 0.001 && last.y.abs() < 0.001,
            "a quarter turn is to the right"
        );
    }

    #[test]
    fn a_box_is_big_enough_for_its_words_and_never_smaller_than_asked() {
        let metrics = FixedMetrics::default();
        let style = CharStyle { size: 14.0, ..CharStyle::default() };
        let boxed = measure_box("Start", &style, &metrics, Size::new(0.0, 0.0));
        assert_eq!(boxed.rect.width, 50.0 + PADDING_X * 2.0);
        assert_eq!(boxed.rect.height, 20.0 + PADDING_Y * 2.0);
        let smallest = measure_box("a", &style, &metrics, Size::new(120.0, 60.0));
        assert_eq!(smallest.rect.size(), Size::new(120.0, 60.0));
    }
}
