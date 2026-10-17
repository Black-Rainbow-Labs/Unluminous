//! Whiteboard shapes: what a shape node is, what its outline is, where its text goes, and where a wire
//! may join it. `task-2284`.
//!
//! `C:/jason/dev/unluminous/.designs/realm-whiteboard.html` is the design. A shape is a node of
//! [`super::Kind::Shape`] holding a [`Shape`]: which of nine outlines it is, the words in it, an optional
//! colour of its own, how the words are aligned and how big they are.
//!
//! Everything here is a pure function of a shape and the rectangle it fills, in world points, so the
//! arithmetic that has to be right is tested with no window. `components::realm::shape` draws it.
//!
//! ## Three things each outline answers for itself
//!
//! **The outline**, as a polygon. A rounded corner and a circle are flattened into enough straight pieces
//! that nobody can see the facets at the sizes a shape is drawn at.
//!
//! **Where the words go.** A triangle's text sits in its lower middle and a diamond's in the square inside
//! it, because the corners of the bounding rectangle are outside the shape.
//!
//! **Where a wire joins it.** The middle of each side, which is where the outline crosses the shape's own
//! centre lines on a rectangle, a circle and a hexagon. A diamond's four corners, a triangle's top point and
//! the middles of its other three sides, a parallelogram's four side middles, and a star's five points.
//! Each comes with the direction out of the outline at that point, so a wire leaves the shape at a right
//! angle to it rather than always going right.

use egui::{Pos2, Rect, Vec2};
use unluminous_core::Rgba;

/// Which outline a shape has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ShapeKind {
    #[default]
    Rectangle,
    Rounded,
    Circle,
    Triangle,
    Diamond,
    Hexagon,
    Parallelogram,
    Star,
    Sticky,
}

impl ShapeKind {
    /// Every shape, in the order the Add a node dialog lists them.
    pub const ALL: [ShapeKind; 9] = [
        ShapeKind::Rectangle,
        ShapeKind::Rounded,
        ShapeKind::Circle,
        ShapeKind::Triangle,
        ShapeKind::Diamond,
        ShapeKind::Hexagon,
        ShapeKind::Parallelogram,
        ShapeKind::Star,
        ShapeKind::Sticky,
    ];

    /// The name the command line and the realm file use.
    pub fn name(self) -> &'static str {
        match self {
            ShapeKind::Rectangle => "rectangle",
            ShapeKind::Rounded => "rounded",
            ShapeKind::Circle => "circle",
            ShapeKind::Triangle => "triangle",
            ShapeKind::Diamond => "diamond",
            ShapeKind::Hexagon => "hexagon",
            ShapeKind::Parallelogram => "parallelogram",
            ShapeKind::Star => "star",
            ShapeKind::Sticky => "sticky",
        }
    }

    /// What a person reads in the dialog's list of shapes.
    pub fn label(self) -> &'static str {
        match self {
            ShapeKind::Rectangle => "Rectangle",
            ShapeKind::Rounded => "Rounded rectangle",
            ShapeKind::Circle => "Circle",
            ShapeKind::Triangle => "Triangle",
            ShapeKind::Diamond => "Diamond",
            ShapeKind::Hexagon => "Hexagon",
            ShapeKind::Parallelogram => "Parallelogram",
            ShapeKind::Star => "Star",
            ShapeKind::Sticky => "Sticky note",
        }
    }

    /// The shape a command line word names. `ellipse`, `note` and `rounded-rectangle` are read too, because
    /// those are the words an agent guesses.
    pub fn from_name(name: &str) -> Option<ShapeKind> {
        let name = name.trim().to_lowercase();
        let known = ShapeKind::ALL.into_iter().find(|kind| kind.name() == name);
        known.or(match name.as_str() {
            "rect" | "square" | "box" => Some(ShapeKind::Rectangle),
            "rounded-rectangle" | "rounded rectangle" | "round" => Some(ShapeKind::Rounded),
            "ellipse" | "oval" => Some(ShapeKind::Circle),
            "sticky-note" | "sticky note" | "note" | "postit" => Some(ShapeKind::Sticky),
            _ => None,
        })
    }

    /// The size a new shape of this kind opens at, in world points. A circle and a star open square, a
    /// rectangle wider than it is tall.
    pub fn opens_at(self) -> Vec2 {
        match self {
            ShapeKind::Circle | ShapeKind::Star => Vec2::new(130.0, 130.0),
            ShapeKind::Triangle => Vec2::new(140.0, 125.0),
            ShapeKind::Diamond => Vec2::new(140.0, 140.0),
            ShapeKind::Sticky => Vec2::new(160.0, 150.0),
            _ => Vec2::new(170.0, 100.0),
        }
    }
}

/// How the words in a shape sit across it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    Left,
    #[default]
    Centre,
    Right,
}

impl Align {
    pub const ALL: [Align; 3] = [Align::Left, Align::Centre, Align::Right];

    pub fn name(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Centre => "centre",
            Align::Right => "right",
        }
    }

    /// The menu row.
    pub fn label(self) -> &'static str {
        match self {
            Align::Left => "Align Left",
            Align::Centre => "Align Centre",
            Align::Right => "Align Right",
        }
    }

    /// `center` is read as well as `centre`, because that is the spelling most code uses.
    pub fn from_name(name: &str) -> Option<Align> {
        match name.trim().to_lowercase().as_str() {
            "left" => Some(Align::Left),
            "centre" | "center" | "middle" => Some(Align::Centre),
            "right" => Some(Align::Right),
            _ => None,
        }
    }

    pub fn egui(self) -> egui::Align {
        match self {
            Align::Left => egui::Align::Min,
            Align::Centre => egui::Align::Center,
            Align::Right => egui::Align::Max,
        }
    }
}

/// How the words in a shape sit down it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

impl VAlign {
    pub const ALL: [VAlign; 3] = [VAlign::Top, VAlign::Middle, VAlign::Bottom];

    pub fn name(self) -> &'static str {
        match self {
            VAlign::Top => "top",
            VAlign::Middle => "middle",
            VAlign::Bottom => "bottom",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            VAlign::Top => "Align Top",
            VAlign::Middle => "Align Middle",
            VAlign::Bottom => "Align Bottom",
        }
    }

    pub fn from_name(name: &str) -> Option<VAlign> {
        match name.trim().to_lowercase().as_str() {
            "top" => Some(VAlign::Top),
            "middle" | "centre" | "center" => Some(VAlign::Middle),
            "bottom" => Some(VAlign::Bottom),
            _ => None,
        }
    }

    pub fn egui(self) -> egui::Align {
        match self {
            VAlign::Top => egui::Align::Min,
            VAlign::Middle => egui::Align::Center,
            VAlign::Bottom => egui::Align::Max,
        }
    }
}

/// The text sizes the Text menu offers, in points. `Larger` and `Smaller` step through these.
pub const SIZES: [f32; 5] = [12.0, 14.0, 18.0, 24.0, 32.0];

/// The size a new shape's words are set at.
pub const DEFAULT_SIZE: f32 = 14.0;

/// One step of the Text menu's size rows.
///
/// **Whole points**, so a step is a value that can be compared exactly, which an action in a menu has to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SizeStep {
    /// A size in whole points, such as one of [`SIZES`].
    To(u8),
    Larger,
    Smaller,
}

impl SizeStep {
    /// The size this step gives a shape whose words are `now` points.
    ///
    /// `Larger` goes to the next size in [`SIZES`] above `now` and `Smaller` to the next below, so a size
    /// that is not on the list, which a hand edited file can hold, still steps to its neighbour.
    pub fn apply(self, now: f32) -> f32 {
        match self {
            SizeStep::To(size) => f32::from(size),
            SizeStep::Larger => SIZES.into_iter().find(|size| *size > now + 0.01).unwrap_or(now),
            SizeStep::Smaller => {
                SIZES.into_iter().rev().find(|size| *size < now - 0.01).unwrap_or(now)
            }
        }
    }

    /// The name the command line and an action use: `14`, `larger` or `smaller`.
    pub fn name(self) -> String {
        match self {
            SizeStep::To(size) => size.to_string(),
            SizeStep::Larger => "larger".to_owned(),
            SizeStep::Smaller => "smaller".to_owned(),
        }
    }

    /// A step from its name. A number is any size from 6 to 96, which is what a person can type.
    pub fn from_name(name: &str) -> Option<SizeStep> {
        let name = name.trim().trim_end_matches("pt").trim().to_lowercase();
        match name.as_str() {
            "larger" | "bigger" => Some(SizeStep::Larger),
            "smaller" => Some(SizeStep::Smaller),
            number => number
                .parse::<f32>()
                .ok()
                .filter(|size| (6.0..=96.0).contains(size))
                .map(|size| SizeStep::To(size.round() as u8)),
        }
    }
}

/// What a shape node has written down about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub shape: ShapeKind,
    /// The words in it. Empty draws the shape's name in the quiet colour, which is the design's hint.
    pub text: String,
    /// A colour of its own, or `None` for the theme's own card.
    ///
    /// **`None` follows the theme**, so a shape nobody has recoloured is drawn in Unluminous Light's card on a
    /// light ground and in the dark card on a dark one.
    pub fill: Option<Rgba>,
    pub align: Align,
    pub valign: VAlign,
    /// How big the words are, in points at a zoom of one.
    pub size: f32,
}

impl Default for Shape {
    fn default() -> Self {
        Shape {
            shape: ShapeKind::Rectangle,
            text: String::new(),
            fill: None,
            align: Align::Centre,
            valign: VAlign::Middle,
            size: DEFAULT_SIZE,
        }
    }
}

impl Shape {
    pub fn of(shape: ShapeKind) -> Shape {
        Shape { shape, ..Shape::default() }
    }
}

/// One place a wire may join a node, in world points, with the direction out of the node there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub at: Pos2,
    /// A unit vector pointing away from the node.
    pub out: Vec2,
}

/// What the point a wire joins is called, for the command line: `top`, `right`, `bottom` and `left` for a
/// shape with four, and `point-1` to `point-5` for a star.
pub fn point_name(shape: ShapeKind, index: usize) -> String {
    match shape {
        ShapeKind::Star => format!("point-{}", index + 1),
        _ => ["top", "right", "bottom", "left"].get(index).copied().unwrap_or("top").to_owned(),
    }
}

/// The index of the point a command line word names, on a shape of this kind.
pub fn point_from_name(shape: ShapeKind, name: &str) -> Option<usize> {
    let name = name.trim().to_lowercase();
    let count = anchors(shape, Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0))).len();
    if let Ok(number) = name.parse::<usize>() {
        return (number < count).then_some(number);
    }
    (0..count).find(|index| point_name(shape, *index) == name)
}

/// The middle of the drawing a star is built round, as a fraction of its height. A five pointed star is
/// taller above its middle than below it, so its middle sits a little below the rectangle's.
const STAR_MIDDLE: f32 = 0.53;
/// How far in a star's inner corners are, as a fraction of its outer reach.
const STAR_INNER: f32 = 0.42;
/// How big a sticky note's folded corner is, in world points, before it is capped for a small note.
const FOLD: f32 = 22.0;

/// The outline of a shape filling `rect`, as a polygon going round it clockwise on the screen.
pub fn outline(shape: ShapeKind, rect: Rect) -> Vec<Pos2> {
    let (w, h) = (rect.width(), rect.height());
    let at = |x: f32, y: f32| Pos2::new(rect.left() + x, rect.top() + y);
    match shape {
        ShapeKind::Rectangle => rounded(rect, 6.0),
        ShapeKind::Rounded => rounded(rect, w.min(h) * 0.22),
        ShapeKind::Circle => (0..72)
            .map(|step| {
                let angle =
                    -std::f32::consts::FRAC_PI_2 + step as f32 * std::f32::consts::TAU / 72.0;
                Pos2::new(
                    rect.center().x + angle.cos() * w / 2.0,
                    rect.center().y + angle.sin() * h / 2.0,
                )
            })
            .collect(),
        ShapeKind::Triangle => vec![at(w / 2.0, 0.0), at(w, h), at(0.0, h)],
        ShapeKind::Diamond => {
            vec![at(w / 2.0, 0.0), at(w, h / 2.0), at(w / 2.0, h), at(0.0, h / 2.0)]
        }
        ShapeKind::Hexagon => vec![
            at(w * 0.22, 0.0),
            at(w * 0.78, 0.0),
            at(w, h / 2.0),
            at(w * 0.78, h),
            at(w * 0.22, h),
            at(0.0, h / 2.0),
        ],
        ShapeKind::Parallelogram => vec![at(w * 0.2, 0.0), at(w, 0.0), at(w * 0.8, h), at(0.0, h)],
        ShapeKind::Star => (0..10)
            .map(|step| {
                let angle = -std::f32::consts::FRAC_PI_2 + step as f32 * std::f32::consts::PI / 5.0;
                let reach = if step % 2 == 0 { 1.0 } else { STAR_INNER };
                at(
                    w / 2.0 + angle.cos() * (w / 2.0) * reach,
                    h * STAR_MIDDLE + angle.sin() * (h * STAR_MIDDLE) * reach,
                )
            })
            .collect(),
        ShapeKind::Sticky => {
            let fold = fold_of(rect);
            vec![at(0.0, 0.0), at(w, 0.0), at(w, h - fold), at(w - fold, h), at(0.0, h)]
        }
    }
}

/// How big a sticky note's folded corner is: [`FOLD`], or a third of the note's smaller side when that is
/// less, so a small note is still a note rather than a triangle.
pub fn fold_of(rect: Rect) -> f32 {
    FOLD.min(rect.width().min(rect.height()) / 3.0)
}

/// A rectangle with round corners, as a polygon. Each corner is eight straight pieces.
fn rounded(rect: Rect, radius: f32) -> Vec<Pos2> {
    let radius = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let corners = [
        (Pos2::new(rect.right() - radius, rect.top() + radius), -90.0_f32),
        (Pos2::new(rect.right() - radius, rect.bottom() - radius), 0.0),
        (Pos2::new(rect.left() + radius, rect.bottom() - radius), 90.0),
        (Pos2::new(rect.left() + radius, rect.top() + radius), 180.0),
    ];
    let mut points = Vec::with_capacity(36);
    for (centre, from) in corners {
        for step in 0..=8 {
            let angle = (from + step as f32 * 90.0 / 8.0).to_radians();
            points.push(centre + Vec2::new(angle.cos(), angle.sin()) * radius);
        }
    }
    points
}

/// A point inside the shape that sees every point of its outline in a straight line.
///
/// Every outline here is star shaped about this point, which is what lets the drawing fill it as a fan of
/// triangles from it, the concave star included.
pub fn kernel(shape: ShapeKind, rect: Rect) -> Pos2 {
    match shape {
        ShapeKind::Star => Pos2::new(rect.center().x, rect.top() + rect.height() * STAR_MIDDLE),
        ShapeKind::Triangle => Pos2::new(rect.center().x, rect.top() + rect.height() * 2.0 / 3.0),
        _ => rect.center(),
    }
}

/// The part of the shape that can hold words, in world points.
///
/// The design's own numbers: the lower middle of a triangle, the square inside a diamond and a circle, the
/// part of a hexagon and a parallelogram between their slanted sides, the middle of a star, and a sticky
/// note less its margin and its folded corner.
pub fn text_rect(shape: ShapeKind, rect: Rect) -> Rect {
    let (w, h) = (rect.width(), rect.height());
    let part = |x: f32, y: f32, width: f32, height: f32| {
        Rect::from_min_size(rect.min + Vec2::new(x, y), Vec2::new(width.max(1.0), height.max(1.0)))
    };
    match shape {
        ShapeKind::Triangle => part(w * 0.22, h * 0.42, w * 0.56, h * 0.5),
        ShapeKind::Diamond => part(w * 0.22, h * 0.22, w * 0.56, h * 0.56),
        ShapeKind::Circle => part(w * 0.15, h * 0.15, w * 0.7, h * 0.7),
        ShapeKind::Hexagon | ShapeKind::Parallelogram => part(w * 0.18, 4.0, w * 0.64, h - 8.0),
        ShapeKind::Star => part(w * 0.3, h * 0.36, w * 0.4, h * 0.36),
        ShapeKind::Sticky => {
            let fold = fold_of(rect);
            part(10.0, 10.0, w - 20.0, h - 14.0 - fold)
        }
        ShapeKind::Rectangle | ShapeKind::Rounded => part(8.0, 8.0, w - 16.0, h - 16.0),
    }
}

/// Where a wire may join a shape filling `rect`, each with the way out of the outline there.
///
/// Four for every shape but the star, in the order top, right, bottom, left; five for the star, from its
/// top point clockwise. See the module's own comment for which points each shape has.
pub fn anchors(shape: ShapeKind, rect: Rect) -> Vec<Anchor> {
    let (w, h) = (rect.width(), rect.height());
    let at = |x: f32, y: f32| Pos2::new(rect.left() + x, rect.top() + y);
    let straight = |top: Pos2, right: Pos2, bottom: Pos2, left: Pos2| {
        vec![
            Anchor { at: top, out: Vec2::new(0.0, -1.0) },
            Anchor { at: right, out: Vec2::new(1.0, 0.0) },
            Anchor { at: bottom, out: Vec2::new(0.0, 1.0) },
            Anchor { at: left, out: Vec2::new(-1.0, 0.0) },
        ]
    };
    match shape {
        ShapeKind::Star => (0..5)
            .map(|index| {
                let angle =
                    -std::f32::consts::FRAC_PI_2 + index as f32 * std::f32::consts::TAU / 5.0;
                let direction = Vec2::new(angle.cos(), angle.sin());
                Anchor {
                    at: at(
                        w / 2.0 + direction.x * w / 2.0,
                        h * STAR_MIDDLE + direction.y * h * STAR_MIDDLE,
                    ),
                    out: direction,
                }
            })
            .collect(),
        // The top point, the middle of the bottom edge, and the middle of each sloping side, leaving each
        // side at a right angle to it.
        ShapeKind::Triangle => {
            let right_side = side_normal(at(w / 2.0, 0.0), at(w, h), rect.center());
            let left_side = side_normal(at(0.0, h), at(w / 2.0, 0.0), rect.center());
            vec![
                Anchor { at: at(w / 2.0, 0.0), out: Vec2::new(0.0, -1.0) },
                Anchor { at: at(w * 0.75, h / 2.0), out: right_side },
                Anchor { at: at(w / 2.0, h), out: Vec2::new(0.0, 1.0) },
                Anchor { at: at(w * 0.25, h / 2.0), out: left_side },
            ]
        }
        // The middle of each of the four sides, which on the slanted ones is not the rectangle's middle.
        ShapeKind::Parallelogram => {
            let right_side = side_normal(at(w, 0.0), at(w * 0.8, h), rect.center());
            let left_side = side_normal(at(0.0, h), at(w * 0.2, 0.0), rect.center());
            vec![
                Anchor { at: at(w * 0.6, 0.0), out: Vec2::new(0.0, -1.0) },
                Anchor { at: at(w * 0.9, h / 2.0), out: right_side },
                Anchor { at: at(w * 0.4, h), out: Vec2::new(0.0, 1.0) },
                Anchor { at: at(w * 0.1, h / 2.0), out: left_side },
            ]
        }
        _ => straight(at(w / 2.0, 0.0), at(w, h / 2.0), at(w / 2.0, h), at(0.0, h / 2.0)),
    }
}

/// The unit vector at right angles to the side from `a` to `b`, on the side away from `inside`.
fn side_normal(a: Pos2, b: Pos2, inside: Pos2) -> Vec2 {
    let along = b - a;
    let normal = Vec2::new(along.y, -along.x).normalized();
    let middle = a + along / 2.0;
    match (middle + normal - inside).length() > (middle - normal - inside).length() {
        true => normal,
        false => -normal,
    }
}

/// The index of the point nearest `to`.
pub fn nearest_anchor(shape: ShapeKind, rect: Rect, to: Pos2) -> usize {
    anchors(shape, rect)
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.at
                .distance(to)
                .partial_cmp(&right.at.distance(to))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

/// Whether a world point is inside the shape's outline, rather than only inside its rectangle.
///
/// Even odd crossing, which is right for every outline here because none of them crosses itself.
pub fn contains(shape: ShapeKind, rect: Rect, point: Pos2) -> bool {
    if !rect.contains(point) {
        return false;
    }
    let polygon = outline(shape, rect);
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let (a, b) = (polygon[current], polygon[previous]);
        if (a.y > point.y) != (b.y > point.y) {
            let crosses = (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x;
            if point.x < crosses {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

/// A shape made square from a rectangle being resized by `grip`, which is what Shift does while resizing.
///
/// The side is the larger of the two, and the corner opposite the grip stays where it is, so a square grows
/// away from the corner that is not being held.
pub fn squared(rect: Rect, grip: super::Grip) -> Rect {
    use super::Grip;
    let side = rect.width().max(rect.height());
    let size = Vec2::splat(side);
    match grip {
        Grip::TopLeft => Rect::from_min_size(rect.max - size, size),
        Grip::TopRight | Grip::Top => {
            Rect::from_min_size(Pos2::new(rect.left(), rect.bottom() - side), size)
        }
        Grip::BottomLeft | Grip::Left => {
            Rect::from_min_size(Pos2::new(rect.right() - side, rect.top()), size)
        }
        Grip::BottomRight | Grip::Right | Grip::Bottom => Rect::from_min_size(rect.min, size),
    }
}

/// The words of a shape as a realm file line holds them.
///
/// A realm file is one `name = value` a line with the value trimmed, and a `#` followed by a space starts a
/// comment, so a line break, a hash, a backslash and a space at either end are each written as a backslash
/// and a letter.
pub fn encode_text(text: &str) -> String {
    let mut written = String::with_capacity(text.len());
    let last = text.chars().count().saturating_sub(1);
    for (index, character) in text.chars().enumerate() {
        match character {
            '\\' => written.push_str("\\\\"),
            '\n' => written.push_str("\\n"),
            '\r' => {}
            '#' => written.push_str("\\h"),
            ' ' if index == 0 || index == last => written.push_str("\\s"),
            other => written.push(other),
        }
    }
    written
}

/// The words of a shape from a realm file line. The inverse of [`encode_text`].
pub fn decode_text(written: &str) -> String {
    let mut text = String::with_capacity(written.len());
    let mut characters = written.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => text.push('\n'),
            Some('h') => text.push('#'),
            Some('s') => text.push(' '),
            Some('\\') => text.push('\\'),
            Some(other) => {
                text.push('\\');
                text.push(other);
            }
            None => text.push('\\'),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::realm::Grip;

    fn a_box() -> Rect {
        Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(160.0, 120.0))
    }

    #[test]
    fn every_shape_is_named_once_and_reads_back() {
        for shape in ShapeKind::ALL {
            assert_eq!(ShapeKind::from_name(shape.name()), Some(shape));
            assert!(!shape.label().is_empty());
        }
        assert_eq!(ShapeKind::from_name("Ellipse"), Some(ShapeKind::Circle));
        assert_eq!(ShapeKind::from_name("note"), Some(ShapeKind::Sticky));
        assert_eq!(ShapeKind::from_name("blob"), None);
    }

    /// Item 14 and 15 of the ticket: four points on every shape but the star, five on the star.
    #[test]
    fn every_shape_has_four_connection_points_and_a_star_has_five() {
        for shape in ShapeKind::ALL {
            let points = anchors(shape, a_box());
            let wanted = if shape == ShapeKind::Star { 5 } else { 4 };
            assert_eq!(points.len(), wanted, "{}", shape.name());
            for point in &points {
                assert!((point.out.length() - 1.0).abs() < 0.001, "a direction is a unit vector");
            }
        }
    }

    /// The middle of each edge, and a diamond's four corners, which are the same four places.
    #[test]
    fn a_rectangles_points_are_the_middles_of_its_edges_and_a_diamonds_are_its_corners() {
        let rect = a_box();
        let points: Vec<Pos2> = anchors(ShapeKind::Rectangle, rect).iter().map(|a| a.at).collect();
        assert_eq!(
            points,
            vec![
                Pos2::new(rect.center().x, rect.top()),
                Pos2::new(rect.right(), rect.center().y),
                Pos2::new(rect.center().x, rect.bottom()),
                Pos2::new(rect.left(), rect.center().y),
            ]
        );
        let diamond = outline(ShapeKind::Diamond, rect);
        for point in anchors(ShapeKind::Diamond, rect) {
            assert!(diamond.contains(&point.at), "{:?} is a corner of the diamond", point.at);
        }
    }

    /// A triangle's sloping sides are joined at their middles and left at a right angle to them.
    #[test]
    fn a_triangles_side_points_are_on_its_sides_and_leave_them_square() {
        let rect = a_box();
        let points = anchors(ShapeKind::Triangle, rect);
        let top = Pos2::new(rect.center().x, rect.top());
        let right = rect.right_bottom();
        assert_eq!(points[1].at, top + (right - top) / 2.0, "the middle of the right side");
        assert!(points[1].out.dot(right - top).abs() < 0.001, "at a right angle to the side");
        assert!(points[1].out.x > 0.0 && points[1].out.y < 0.0, "pointing out, up and right");
        assert!(points[3].out.x < 0.0 && points[3].out.y < 0.0, "and the left side up and left");
        assert_eq!(points[2].at, Pos2::new(rect.center().x, rect.bottom()));
    }

    #[test]
    fn a_parallelograms_points_are_the_middles_of_its_four_sides() {
        let rect = a_box();
        let corners = outline(ShapeKind::Parallelogram, rect);
        let points = anchors(ShapeKind::Parallelogram, rect);
        let middle = |a: Pos2, b: Pos2| a + (b - a) / 2.0;
        assert_eq!(points[0].at, middle(corners[0], corners[1]));
        assert!((points[1].at - middle(corners[1], corners[2])).length() < 0.01);
        assert_eq!(points[2].at, middle(corners[2], corners[3]));
        assert!((points[3].at - middle(corners[3], corners[0])).length() < 0.01);
    }

    #[test]
    fn a_stars_points_are_its_five_tips() {
        let rect = a_box();
        let star = outline(ShapeKind::Star, rect);
        for (index, point) in anchors(ShapeKind::Star, rect).iter().enumerate() {
            assert!((point.at - star[index * 2]).length() < 0.01, "tip {index}");
        }
    }

    #[test]
    fn the_nearest_point_is_the_one_a_wire_lands_on() {
        let rect = a_box();
        assert_eq!(nearest_anchor(ShapeKind::Rectangle, rect, rect.right_center()), 1);
        // The left middle is 60 points from the bottom left corner and the bottom middle is 80.
        assert_eq!(nearest_anchor(ShapeKind::Rectangle, rect, rect.left_bottom()), 3);
        assert_eq!(nearest_anchor(ShapeKind::Rectangle, rect, Pos2::new(0.0, rect.center().y)), 3);
    }

    #[test]
    fn a_point_is_inside_the_outline_and_not_merely_the_rectangle() {
        let rect = a_box();
        assert!(contains(ShapeKind::Diamond, rect, rect.center()));
        assert!(
            !contains(ShapeKind::Diamond, rect, rect.min + Vec2::splat(3.0)),
            "a diamond's corner is empty"
        );
        assert!(contains(ShapeKind::Rectangle, rect, rect.min + Vec2::splat(8.0)));
        assert!(!contains(ShapeKind::Triangle, rect, rect.min + Vec2::splat(3.0)));
        assert!(!contains(ShapeKind::Circle, rect, Pos2::new(500.0, 500.0)));
    }

    /// The text of a triangle sits in its lower middle and inside the outline.
    #[test]
    fn every_shapes_words_sit_inside_its_outline() {
        let rect = a_box();
        for shape in ShapeKind::ALL {
            let room = text_rect(shape, rect);
            assert!(rect.contains_rect(room), "{}", shape.name());
            assert!(contains(shape, rect, room.center()), "{}'s words are inside it", shape.name());
        }
        let triangle = text_rect(ShapeKind::Triangle, rect);
        assert!(triangle.center().y > rect.center().y, "a triangle's words are in its lower half");
    }

    #[test]
    fn the_kernel_sees_every_point_of_the_outline() {
        let rect = a_box();
        for shape in ShapeKind::ALL {
            assert!(contains(shape, rect, kernel(shape, rect)), "{}", shape.name());
        }
    }

    #[test]
    fn larger_and_smaller_step_through_the_menus_sizes() {
        assert_eq!(SizeStep::Larger.apply(14.0), 18.0);
        assert_eq!(SizeStep::Larger.apply(32.0), 32.0, "the largest stays");
        assert_eq!(SizeStep::Smaller.apply(14.0), 12.0);
        assert_eq!(SizeStep::Smaller.apply(12.0), 12.0);
        assert_eq!(
            SizeStep::Larger.apply(16.0),
            18.0,
            "a size off the list steps to its neighbour"
        );
        assert_eq!(SizeStep::from_name("24 pt"), Some(SizeStep::To(24)));
        assert_eq!(SizeStep::To(24).apply(14.0), 24.0);
        assert_eq!(SizeStep::from_name("bigger"), Some(SizeStep::Larger));
        assert_eq!(SizeStep::from_name("400"), None);
    }

    #[test]
    fn shift_makes_a_resize_square_and_keeps_the_far_corner_still() {
        let rect = Rect::from_min_max(Pos2::new(10.0, 10.0), Pos2::new(110.0, 70.0));
        let square = squared(rect, Grip::BottomRight);
        assert_eq!(square.size(), Vec2::splat(100.0));
        assert_eq!(square.min, rect.min);
        let square = squared(rect, Grip::TopLeft);
        assert_eq!(square.max, rect.max, "the corner not being held stays");
    }

    #[test]
    fn a_shapes_words_survive_a_realm_file_line() {
        for text in
            ["", "plain", " spaced ", "two\nlines", "a # comment", "back\\slash", "#tag", "\\n"]
        {
            let written = encode_text(text);
            assert!(!written.contains('\n') && !written.contains('#'), "{written:?}");
            assert_eq!(written.trim(), written, "nothing at the ends is trimmed away");
            assert_eq!(decode_text(&written), text);
        }
    }

    #[test]
    fn a_points_name_reads_back() {
        for shape in ShapeKind::ALL {
            let count = anchors(shape, a_box()).len();
            for index in 0..count {
                assert_eq!(point_from_name(shape, &point_name(shape, index)), Some(index));
            }
            assert_eq!(point_from_name(shape, &count.to_string()), None);
        }
        assert_eq!(point_from_name(ShapeKind::Diamond, "Right"), Some(1));
    }
}
