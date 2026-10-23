//! The shapes a node can be, and how each one is drawn and touched.
//!
//! Mermaid's flowchart has fourteen bracket shapes and forty more that `@{ shape: … }` names, its
//! mindmap has six, and every one of them needs the same four answers: how much bigger than its words
//! it has to be, what to draw, where its words go, and where a line pointing at it should stop.
//! Keeping those together is what stops a shape being added with a border that fits and an arrow
//! that lands in the middle of the text.
//!
//! The named shapes are `task-2339`'s, taken from Mermaid's own `rendering-elements/shapes.ts`, and
//! their geometry is at the bottom of this file under [`draw_named`].

use super::parts::{self, Outline};
use super::scene::{Item, Paint, Point, Rect, Scene, Size, Stroke};

/// What a node looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    /// `id[text]`
    #[default]
    Rect,
    /// `id(text)`
    Round,
    /// `id([text])`
    Stadium,
    /// `id[[text]]`
    Subroutine,
    /// `id[(text)]`
    Cylinder,
    /// `id((text))`
    Circle,
    /// `id(((text)))`
    DoubleCircle,
    /// `id>text]`
    Asymmetric,
    /// `id{text}`
    Diamond,
    /// `id{{text}}`
    Hexagon,
    /// `id[/text/]`
    Parallelogram,
    /// `id[\text\]`
    ParallelogramAlt,
    /// `id[/text\]`
    Trapezoid,
    /// `id[\text/]`
    TrapezoidAlt,
    /// A mindmap's `id))text((`
    Bang,
    /// A mindmap's `id)text(`
    Cloud,
    /// A state diagram's start or end marker. Drawn from its own code, and here so that a state
    /// diagram can put one in the same list as its ordinary states.
    Marker,
    // The shapes only `@{ shape: … }` can name. Each comment starts with Mermaid's short name.
    /// `doc`: a page with a wavy bottom edge.
    Document,
    /// `lin-doc`: a document with a line down its left side.
    LinedDocument,
    /// `docs`: a stack of documents.
    Documents,
    /// `notch-rect`: a card, its top left corner cut off.
    NotchedRect,
    /// `lin-rect`: a process with a line down its left side.
    LinedRect,
    /// `st-rect`: a stack of processes.
    StackedRect,
    /// `bolt`: a lightning bolt, a communication link.
    Bolt,
    /// `delay`: square on the left and round on the right.
    Delay,
    /// `h-cyl`: a cylinder on its side, direct access storage.
    HorizontalCylinder,
    /// `lin-cyl`: a cylinder with a second line under its top, disk storage.
    LinedCylinder,
    /// `curv-trap`: a display, pointed on the left and round on the right.
    Display,
    /// `div-rect`: a process with a band across its top.
    DividedRect,
    /// `tri`: a triangle pointing up, extract.
    Triangle,
    /// `flip-tri`: a triangle pointing down, a manual file.
    FlippedTriangle,
    /// `win-pane`: a line across the top and one down the left, internal storage.
    WindowPane,
    /// `f-circ`: a small filled circle, a junction.
    FilledCircle,
    /// `sm-circ`: a small circle, a start.
    SmallCircle,
    /// `fr-circ`: a small circle inside a ring, a stop.
    FramedCircle,
    /// `cross-circ`: a circle with a cross in it, a summary.
    CrossedCircle,
    /// `sl-rect`: a top that slopes up to the right, manual input.
    SlopedRect,
    /// `hourglass`: two triangles point to point, collate.
    Hourglass,
    /// `brace`: a comment with a curly brace on its left.
    Brace,
    /// `brace-r`: a comment with a curly brace on its right.
    BraceRight,
    /// `braces`: a comment between two curly braces.
    Braces,
    /// `text`: words with no shape round them.
    Text,
    /// `notch-pent`: the top corners cut off, a loop limit.
    NotchedPentagon,
    /// `fork`: a filled bar, a fork or a join.
    Fork,
    /// `tag-rect`: a process with a folded corner at its bottom right.
    TaggedRect,
    /// `tag-doc`: a document with a folded corner at its bottom right.
    TaggedDocument,
    /// `bow-rect`: stored data, round on the left and hollowed on the right.
    BowTie,
    /// `flag`: paper tape, wavy at the top and at the bottom.
    Flag,
    /// `datastore`: a line above and a line below and nothing at the sides.
    DataStore,
    /// `folder`: a folder with a tab.
    Folder,
    /// `bucket`: wider at the top than at the bottom.
    Bucket,
    /// `console`: a terminal window.
    Console,
    /// `browser`: a browser window.
    Browser,
    /// `person`: a head on top of a body.
    Person,
}

/// How far in a document's, a lined process's or a lined cylinder's extra line is drawn.
const LINED: f32 = 10.0;
/// How far each sheet behind the front one of a stack is moved up and to the right.
const STACK: f32 = 5.0;
/// How far in a window pane's two lines are.
const PANE: f32 = 12.0;
/// How wide a curly brace is.
const BRACE: f32 = 14.0;
/// How tall a folder's tab is.
const TAB: f32 = 10.0;
/// How tall a console's or a browser's title bar is.
const BAR: f32 = 16.0;

/// How deep a document's wavy edge is.
fn wave(rect: Rect) -> f32 {
    (rect.height * 0.12).min(9.0)
}

/// How deep a cylinder's disc is.
fn lip(rect: Rect) -> f32 {
    (rect.height * 0.18).min(12.0)
}

/// How wide the disc at each end of a cylinder on its side is.
fn side_lip(rect: Rect) -> f32 {
    (rect.width * 0.1).min(rect.height * 0.3)
}

/// How tall the band across a divided process is.
fn band(rect: Rect) -> f32 {
    (rect.height * 0.25).min(14.0)
}

/// How large a person's head is, as a radius.
fn head(rect: Rect) -> f32 {
    (rect.height * 0.16).min(13.0)
}

impl Shape {
    /// How much room this shape needs beyond its words, across and down.
    ///
    /// A diamond is the expensive one: its corners are empty, so the words have to sit inside a
    /// shape half the area of the box round it, and Mermaid's own diamonds are correspondingly
    /// generous. A circle has to hold its words on a diagonal, which is the same problem again.
    pub fn padding(self) -> Size {
        match self {
            Shape::Rect | Shape::Subroutine => Size::new(24.0, 16.0),
            Shape::Round | Shape::Stadium => Size::new(28.0, 16.0),
            Shape::Cylinder => Size::new(28.0, 38.0),
            Shape::Circle | Shape::DoubleCircle => Size::new(46.0, 40.0),
            Shape::Diamond => Size::new(60.0, 34.0),
            Shape::Hexagon => Size::new(46.0, 16.0),
            Shape::Asymmetric => Size::new(38.0, 16.0),
            Shape::Parallelogram | Shape::ParallelogramAlt => Size::new(46.0, 16.0),
            Shape::Trapezoid | Shape::TrapezoidAlt => Size::new(58.0, 16.0),
            Shape::Bang | Shape::Cloud => Size::new(40.0, 24.0),
            Shape::Marker => Size::new(0.0, 0.0),
            Shape::Document | Shape::TaggedDocument => Size::new(30.0, 30.0),
            Shape::LinedDocument => Size::new(30.0 + LINED, 30.0),
            Shape::Documents => Size::new(30.0 + STACK * 2.0, 30.0 + STACK * 2.0),
            Shape::NotchedRect | Shape::TaggedRect => Size::new(32.0, 16.0),
            Shape::LinedRect => Size::new(24.0 + LINED, 16.0),
            Shape::StackedRect => Size::new(24.0 + STACK * 2.0, 16.0 + STACK * 2.0),
            Shape::Delay => Size::new(44.0, 16.0),
            Shape::HorizontalCylinder => Size::new(56.0, 16.0),
            Shape::LinedCylinder => Size::new(28.0, 50.0),
            Shape::Display => Size::new(60.0, 16.0),
            Shape::DividedRect => Size::new(24.0, 30.0),
            Shape::WindowPane => Size::new(24.0 + PANE, 16.0 + PANE),
            Shape::SlopedRect => Size::new(24.0, 34.0),
            Shape::Brace | Shape::BraceRight => Size::new(24.0 + BRACE, 16.0),
            Shape::Braces => Size::new(24.0 + BRACE * 2.0, 16.0),
            Shape::Text => Size::new(8.0, 4.0),
            Shape::NotchedPentagon => Size::new(44.0, 20.0),
            Shape::BowTie => Size::new(52.0, 16.0),
            Shape::Flag => Size::new(28.0, 34.0),
            Shape::DataStore => Size::new(24.0, 20.0),
            Shape::Folder => Size::new(28.0, 20.0 + TAB),
            Shape::Bucket => Size::new(52.0, 40.0),
            Shape::Console | Shape::Browser => Size::new(28.0, 20.0 + BAR),
            Shape::Person => Size::new(28.0, 54.0),
            Shape::Triangle
            | Shape::FlippedTriangle
            | Shape::Bolt
            | Shape::FilledCircle
            | Shape::SmallCircle
            | Shape::FramedCircle
            | Shape::CrossedCircle
            | Shape::Hourglass
            | Shape::Fork => Size::new(0.0, 0.0),
        }
    }

    /// The size of a shape that is a symbol rather than a box, and so is the same size whatever its
    /// words say. Mermaid draws none of these with words in them, and neither does Unluminous.
    pub fn fixed_size(self) -> Option<Size> {
        match self {
            Shape::FilledCircle | Shape::SmallCircle => Some(Size::new(16.0, 16.0)),
            Shape::FramedCircle => Some(Size::new(20.0, 20.0)),
            Shape::CrossedCircle => Some(Size::new(34.0, 34.0)),
            Shape::Bolt => Some(Size::new(30.0, 44.0)),
            Shape::Hourglass => Some(Size::new(36.0, 44.0)),
            Shape::Fork => Some(Size::new(80.0, 10.0)),
            _ => None,
        }
    }

    /// False for a shape drawn with no words in it, which is every shape [`Shape::fixed_size`] lists.
    pub fn shows_label(self) -> bool {
        self.fixed_size().is_none()
    }

    /// The part of the shape its words are centred in.
    ///
    /// The whole rectangle for most shapes. A document's wavy edge, a cylinder's disc, a window's title
    /// bar and a triangle's narrow end are inside the rectangle too, and words drawn over them could
    /// not be read.
    pub fn text_area(self, rect: Rect) -> Rect {
        let inset = |left: f32, top: f32, right: f32, bottom: f32| {
            Rect::new(
                rect.x + left,
                rect.y + top,
                (rect.width - left - right).max(0.0),
                (rect.height - top - bottom).max(0.0),
            )
        };
        match self {
            Shape::Document | Shape::TaggedDocument => inset(0.0, 0.0, 0.0, wave(rect)),
            Shape::LinedDocument => inset(LINED, 0.0, 0.0, wave(rect)),
            Shape::Documents => inset(0.0, STACK * 2.0, STACK * 2.0, wave(rect)),
            Shape::StackedRect => inset(0.0, STACK * 2.0, STACK * 2.0, 0.0),
            Shape::LinedRect => inset(LINED, 0.0, 0.0, 0.0),
            Shape::Cylinder => inset(0.0, lip(rect) * 2.0, 0.0, lip(rect)),
            Shape::Bucket => inset(0.0, lip(rect) * 2.0, 0.0, 0.0),
            Shape::LinedCylinder => inset(0.0, lip(rect) * 3.0, 0.0, lip(rect)),
            Shape::HorizontalCylinder => inset(0.0, 0.0, side_lip(rect) * 2.0, 0.0),
            Shape::Display => inset(rect.width * 0.1, 0.0, side_lip(rect), 0.0),
            Shape::DividedRect => inset(0.0, band(rect), 0.0, 0.0),
            Shape::WindowPane => inset(PANE, PANE, 0.0, 0.0),
            Shape::SlopedRect => inset(0.0, rect.height * 0.3, 0.0, 0.0),
            Shape::Triangle => inset(rect.width * 0.25, rect.height * 0.45, rect.width * 0.25, 0.0),
            Shape::FlippedTriangle => {
                inset(rect.width * 0.25, 0.0, rect.width * 0.25, rect.height * 0.45)
            }
            Shape::Brace => inset(BRACE, 0.0, 0.0, 0.0),
            Shape::BraceRight => inset(0.0, 0.0, BRACE, 0.0),
            Shape::Braces => inset(BRACE, 0.0, BRACE, 0.0),
            Shape::Flag => inset(0.0, wave(rect), 0.0, wave(rect)),
            Shape::Folder => inset(0.0, TAB, 0.0, 0.0),
            Shape::Console | Shape::Browser => inset(0.0, BAR, 0.0, 0.0),
            Shape::Person => inset(0.0, head(rect) * 2.0, 0.0, 0.0),
            _ => rect,
        }
    }

    /// True when the shape has to be as wide as it is tall, whatever its words say.
    pub fn is_round(self) -> bool {
        matches!(self, Shape::Circle | Shape::DoubleCircle)
    }

    /// The size a node of this shape holding a label of `label` needs.
    pub fn size_for(self, label: Size) -> Size {
        // **A diamond is twice its words and a margin** (`task-2063`). A block of words `w` by `h` fits
        // inside a diamond `W` by `H` only while `w / W + h / H` stays under one, and a fixed padding
        // broke that for any label of two lines: the photographed decisions had their words running
        // out past both points. Twice the words keeps the sum near 0.85 whatever the label is.
        if self == Shape::Diamond {
            return Size::new(label.width * 2.0 + 24.0, label.height * 2.0 + 20.0);
        }
        // A triangle holds its words in its wide half, which is the diamond's arithmetic again.
        if matches!(self, Shape::Triangle | Shape::FlippedTriangle) {
            return Size::new(label.width * 2.0 + 30.0, label.height * 2.2 + 24.0);
        }
        if let Some(fixed) = self.fixed_size() {
            return fixed;
        }
        let padding = self.padding();
        let mut size = Size::new(label.width + padding.width, label.height + padding.height);
        if self.is_round() {
            let across = size.width.max(size.height);
            size = Size::new(across, across);
        }
        size
    }

    /// Where a line pointing at this node should stop.
    pub fn outline(self, rect: Rect) -> Outline {
        match self {
            Shape::Circle | Shape::DoubleCircle | Shape::Bang | Shape::Cloud => {
                Outline::Circle(rect.centre(), rect.width.min(rect.height) / 2.0)
            }
            Shape::Marker => Outline::Circle(rect.centre(), rect.width.max(rect.height) / 2.0),
            Shape::Diamond => Outline::Polygon(diamond(rect)),
            Shape::Hexagon => Outline::Polygon(hexagon(rect)),
            Shape::Parallelogram => Outline::Polygon(parallelogram(rect, false)),
            Shape::ParallelogramAlt => Outline::Polygon(parallelogram(rect, true)),
            Shape::Trapezoid => Outline::Polygon(trapezoid(rect, false)),
            Shape::TrapezoidAlt => Outline::Polygon(trapezoid(rect, true)),
            Shape::Asymmetric => Outline::Polygon(asymmetric(rect)),
            Shape::FilledCircle
            | Shape::SmallCircle
            | Shape::FramedCircle
            | Shape::CrossedCircle => {
                Outline::Circle(rect.centre(), rect.width.min(rect.height) / 2.0)
            }
            Shape::Triangle => Outline::Polygon(triangle(rect, false)),
            Shape::FlippedTriangle => Outline::Polygon(triangle(rect, true)),
            Shape::NotchedRect => Outline::Polygon(notched(rect)),
            Shape::NotchedPentagon => Outline::Polygon(pentagon(rect)),
            Shape::SlopedRect => Outline::Polygon(sloped(rect)),
            Shape::Display => Outline::Polygon(display(rect)),
            Shape::Delay => Outline::Polygon(delay(rect)),
            Shape::Hourglass => Outline::Polygon(vec![
                Point::new(rect.left(), rect.top()),
                Point::new(rect.right(), rect.top()),
                Point::new(rect.left(), rect.bottom()),
                Point::new(rect.right(), rect.bottom()),
            ]),
            _ => Outline::Rect(rect),
        }
    }

    /// Draw the shape itself, with no words in it.
    pub fn draw(self, scene: &mut Scene, rect: Rect, fill: Paint, stroke: Stroke) {
        let (fill, stroke) = (Some(fill), Some(stroke));
        match self {
            Shape::Rect => scene.add(Item::Rect { rect, radius: 0.0, fill, stroke }),
            Shape::Round => {
                scene.add(Item::Rect { rect, radius: parts::CORNER * 2.0, fill, stroke })
            }
            Shape::Stadium => {
                scene.add(Item::Rect { rect, radius: rect.height / 2.0, fill, stroke })
            }
            Shape::Subroutine => {
                scene.add(Item::Rect { rect, radius: 0.0, fill, stroke });
                // The two inner bars are what makes it read as a call rather than as a step.
                for inset in [8.0, rect.width - 8.0] {
                    scene.add(Item::Line {
                        points: vec![
                            Point::new(rect.x + inset, rect.y),
                            Point::new(rect.x + inset, rect.bottom()),
                        ],
                        stroke: stroke.expect("a stroke was just made"),
                        dash: super::scene::Dash::Solid,
                    });
                }
            }
            Shape::Cylinder => draw_cylinder(scene, rect, fill, stroke),
            Shape::Circle | Shape::Marker => scene.add(Item::Circle {
                centre: rect.centre(),
                radius: rect.width.min(rect.height) / 2.0,
                fill,
                stroke,
            }),
            Shape::DoubleCircle => {
                let radius = rect.width.min(rect.height) / 2.0;
                scene.add(Item::Circle { centre: rect.centre(), radius, fill, stroke });
                scene.add(Item::Circle {
                    centre: rect.centre(),
                    radius: radius - 5.0,
                    fill: None,
                    stroke,
                });
            }
            Shape::Bang | Shape::Cloud => {
                let points = wobbly(rect, if self == Shape::Bang { 10 } else { 12 });
                scene.add(Item::Polygon { points, fill, stroke });
            }
            Shape::Diamond => scene.add(Item::Polygon { points: diamond(rect), fill, stroke }),
            Shape::Hexagon => scene.add(Item::Polygon { points: hexagon(rect), fill, stroke }),
            Shape::Parallelogram => {
                scene.add(Item::Polygon { points: parallelogram(rect, false), fill, stroke })
            }
            Shape::ParallelogramAlt => {
                scene.add(Item::Polygon { points: parallelogram(rect, true), fill, stroke })
            }
            Shape::Trapezoid => {
                scene.add(Item::Polygon { points: trapezoid(rect, false), fill, stroke })
            }
            Shape::TrapezoidAlt => {
                scene.add(Item::Polygon { points: trapezoid(rect, true), fill, stroke })
            }
            Shape::Asymmetric => {
                scene.add(Item::Polygon { points: asymmetric(rect), fill, stroke })
            }
            _ => draw_named(self, scene, rect, fill, stroke),
        }
    }
}

/// Draw one of the shapes only `@{ shape: … }` can name.
///
/// Every one of them stays inside `rect`, which `every_shape_draws_something_and_nothing_it_draws_is_outside_its_box`
/// holds them to: the layout gave the node that rectangle and knows nothing about anything outside it.
fn draw_named(
    shape: Shape,
    scene: &mut Scene,
    rect: Rect,
    fill: Option<Paint>,
    stroke: Option<Stroke>,
) {
    let line = |scene: &mut Scene, from: Point, to: Point| {
        if let Some(stroke) = stroke {
            scene.add(Item::Line {
                points: vec![from, to],
                stroke,
                dash: super::scene::Dash::Solid,
            });
        }
    };
    match shape {
        Shape::Document => scene.add(Item::Polygon { points: document(rect), fill, stroke }),
        Shape::LinedDocument => {
            scene.add(Item::Polygon { points: document(rect), fill, stroke });
            line(
                scene,
                Point::new(rect.x + LINED, rect.y),
                Point::new(rect.x + LINED, rect.bottom() - wave(rect)),
            );
        }
        Shape::TaggedDocument => {
            scene.add(Item::Polygon { points: document(rect), fill, stroke });
            draw_tag(scene, rect.moved(0.0, -wave(rect)), stroke);
        }
        Shape::Documents => {
            for step in [2.0, 1.0, 0.0] {
                let sheet = Rect::new(
                    rect.x + STACK * step,
                    rect.y + STACK * (2.0 - step),
                    rect.width - STACK * 2.0,
                    rect.height - STACK * 2.0,
                );
                scene.add(Item::Polygon { points: document(sheet), fill, stroke });
            }
        }
        Shape::StackedRect => {
            for step in [2.0, 1.0, 0.0] {
                let sheet = Rect::new(
                    rect.x + STACK * step,
                    rect.y + STACK * (2.0 - step),
                    rect.width - STACK * 2.0,
                    rect.height - STACK * 2.0,
                );
                scene.add(Item::Rect { rect: sheet, radius: 0.0, fill, stroke });
            }
        }
        Shape::NotchedRect => scene.add(Item::Polygon { points: notched(rect), fill, stroke }),
        Shape::LinedRect => {
            scene.add(Item::Rect { rect, radius: 0.0, fill, stroke });
            line(
                scene,
                Point::new(rect.x + LINED, rect.y),
                Point::new(rect.x + LINED, rect.bottom()),
            );
        }
        Shape::TaggedRect => {
            scene.add(Item::Rect { rect, radius: 0.0, fill, stroke });
            draw_tag(scene, rect, stroke);
        }
        Shape::Bolt => scene.add(Item::Polygon { points: bolt(rect), fill, stroke }),
        Shape::Delay => scene.add(Item::Polygon { points: delay(rect), fill, stroke }),
        Shape::HorizontalCylinder => draw_side_cylinder(scene, rect, fill, stroke),
        Shape::LinedCylinder => {
            draw_cylinder(scene, rect, fill, stroke);
            let lip = lip(rect);
            if let Some(stroke) = stroke {
                let below: Vec<Point> =
                    oval(Point::new(rect.centre().x, rect.y + lip * 2.0), rect.width / 2.0, lip)
                        .into_iter()
                        .filter(|point| point.y >= rect.y + lip * 2.0 - 0.01)
                        .collect();
                let mut below = below;
                below.sort_by(|a, b| a.x.total_cmp(&b.x));
                scene.add(Item::Line { points: below, stroke, dash: super::scene::Dash::Solid });
            }
        }
        Shape::Display => scene.add(Item::Polygon { points: display(rect), fill, stroke }),
        Shape::DividedRect => {
            scene.add(Item::Rect { rect, radius: 0.0, fill, stroke });
            line(
                scene,
                Point::new(rect.x, rect.y + band(rect)),
                Point::new(rect.right(), rect.y + band(rect)),
            );
        }
        Shape::Triangle => scene.add(Item::Polygon { points: triangle(rect, false), fill, stroke }),
        Shape::FlippedTriangle => {
            scene.add(Item::Polygon { points: triangle(rect, true), fill, stroke })
        }
        Shape::WindowPane => {
            scene.add(Item::Rect { rect, radius: 0.0, fill, stroke });
            line(scene, Point::new(rect.x, rect.y + PANE), Point::new(rect.right(), rect.y + PANE));
            line(
                scene,
                Point::new(rect.x + PANE, rect.y),
                Point::new(rect.x + PANE, rect.bottom()),
            );
        }
        Shape::FilledCircle => scene.add(Item::Circle {
            centre: rect.centre(),
            radius: rect.width.min(rect.height) / 2.0,
            fill: stroke.map(|stroke| Paint::solid(stroke.color)),
            stroke,
        }),
        Shape::SmallCircle => scene.add(Item::Circle {
            centre: rect.centre(),
            radius: rect.width.min(rect.height) / 2.0,
            fill,
            stroke,
        }),
        Shape::FramedCircle => {
            let radius = rect.width.min(rect.height) / 2.0;
            scene.add(Item::Circle { centre: rect.centre(), radius, fill, stroke });
            scene.add(Item::Circle {
                centre: rect.centre(),
                radius: radius * 0.55,
                fill: stroke.map(|stroke| Paint::solid(stroke.color)),
                stroke: None,
            });
        }
        Shape::CrossedCircle => {
            let radius = rect.width.min(rect.height) / 2.0;
            let centre = rect.centre();
            scene.add(Item::Circle { centre, radius, fill, stroke });
            let arm = radius * std::f32::consts::FRAC_1_SQRT_2;
            line(
                scene,
                Point::new(centre.x - arm, centre.y - arm),
                Point::new(centre.x + arm, centre.y + arm),
            );
            line(
                scene,
                Point::new(centre.x - arm, centre.y + arm),
                Point::new(centre.x + arm, centre.y - arm),
            );
        }
        Shape::SlopedRect => scene.add(Item::Polygon { points: sloped(rect), fill, stroke }),
        Shape::Hourglass => {
            let centre = rect.centre();
            for (y, toward) in [(rect.top(), centre.y), (rect.bottom(), centre.y)] {
                scene.add(Item::Polygon {
                    points: vec![
                        Point::new(rect.left(), y),
                        Point::new(rect.right(), y),
                        Point::new(centre.x, toward),
                    ],
                    fill,
                    stroke,
                });
            }
        }
        Shape::Brace | Shape::BraceRight | Shape::Braces => {
            if let Some(stroke) = stroke {
                if shape != Shape::BraceRight {
                    scene.add(Item::Line {
                        points: brace(rect, false),
                        stroke,
                        dash: super::scene::Dash::Solid,
                    });
                }
                if shape != Shape::Brace {
                    scene.add(Item::Line {
                        points: brace(rect, true),
                        stroke,
                        dash: super::scene::Dash::Solid,
                    });
                }
            }
        }
        Shape::Text => {}
        Shape::NotchedPentagon => scene.add(Item::Polygon { points: pentagon(rect), fill, stroke }),
        Shape::Fork => scene.add(Item::Rect {
            rect,
            radius: 2.0,
            fill: stroke.map(|stroke| Paint::solid(stroke.color)),
            stroke: None,
        }),
        Shape::BowTie => scene.add(Item::Polygon { points: bow_tie(rect), fill, stroke }),
        Shape::Flag => scene.add(Item::Polygon { points: flag(rect), fill, stroke }),
        Shape::DataStore => {
            scene.add(Item::Rect { rect, radius: 0.0, fill, stroke: None });
            line(scene, Point::new(rect.x, rect.y), Point::new(rect.right(), rect.y));
            line(scene, Point::new(rect.x, rect.bottom()), Point::new(rect.right(), rect.bottom()));
        }
        Shape::Folder => scene.add(Item::Polygon { points: folder(rect), fill, stroke }),
        Shape::Bucket => {
            let lip = lip(rect);
            let narrow = rect.width * 0.12;
            scene.add(Item::Polygon {
                points: vec![
                    Point::new(rect.left(), rect.y + lip),
                    Point::new(rect.right(), rect.y + lip),
                    Point::new(rect.right() - narrow, rect.bottom()),
                    Point::new(rect.left() + narrow, rect.bottom()),
                ],
                fill,
                stroke,
            });
            scene.add(Item::Polygon {
                points: oval(Point::new(rect.centre().x, rect.y + lip), rect.width / 2.0, lip),
                fill,
                stroke,
            });
        }
        Shape::Console | Shape::Browser => {
            scene.add(Item::Rect { rect, radius: 3.0, fill, stroke });
            line(scene, Point::new(rect.x, rect.y + BAR), Point::new(rect.right(), rect.y + BAR));
            if let Some(stroke) = stroke {
                // Three dots for the window's buttons; a browser adds an address field beside them.
                for index in 0..3 {
                    scene.add(Item::Circle {
                        centre: Point::new(rect.x + 8.0 + index as f32 * 7.0, rect.y + BAR / 2.0),
                        radius: 2.0,
                        fill: Some(Paint::solid(stroke.color)),
                        stroke: None,
                    });
                }
                if shape == Shape::Browser && rect.width > 60.0 {
                    scene.add(Item::Rect {
                        rect: Rect::new(rect.x + 34.0, rect.y + 4.0, rect.width - 40.0, BAR - 8.0),
                        radius: 3.0,
                        fill: None,
                        stroke: Some(Stroke::new(stroke.color, 1.0)),
                    });
                }
            }
        }
        Shape::Person => {
            let radius = head(rect);
            scene.add(Item::Circle {
                centre: Point::new(rect.centre().x, rect.y + radius),
                radius,
                fill,
                stroke,
            });
            let body =
                Rect::new(rect.x, rect.y + radius * 2.0, rect.width, rect.height - radius * 2.0);
            scene.add(Item::Rect { rect: body, radius: parts::CORNER * 2.0, fill, stroke });
        }
        // The shapes `Shape::draw` draws itself never reach here.
        _ => scene.add(Item::Rect { rect, radius: 0.0, fill, stroke }),
    }
}

/// A page whose bottom edge is a wave.
fn document(rect: Rect) -> Vec<Point> {
    let depth = wave(rect);
    let mut points =
        vec![Point::new(rect.left(), rect.top()), Point::new(rect.right(), rect.top())];
    const STEPS: usize = 16;
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let x = rect.right() - rect.width * t;
        let y = rect.bottom() - depth / 2.0 - depth / 2.0 * (t * std::f32::consts::TAU).sin();
        points.push(Point::new(x, y));
    }
    points
}

/// Paper tape: a wave along the top and the same wave along the bottom.
fn flag(rect: Rect) -> Vec<Point> {
    let depth = wave(rect);
    const STEPS: usize = 16;
    let mut points = Vec::new();
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let y = rect.top() + depth / 2.0 + depth / 2.0 * (t * std::f32::consts::TAU).sin();
        points.push(Point::new(rect.left() + rect.width * t, y));
    }
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let y = rect.bottom() - depth / 2.0 + depth / 2.0 * (t * std::f32::consts::TAU).sin();
        points.push(Point::new(rect.right() - rect.width * t, y));
    }
    points
}

/// The small folded corner at the bottom right of a tagged process or document.
fn draw_tag(scene: &mut Scene, rect: Rect, stroke: Option<Stroke>) {
    let Some(stroke) = stroke else {
        return;
    };
    let size = (rect.height * 0.3).min(12.0);
    scene.add(Item::Polygon {
        points: vec![
            Point::new(rect.right(), rect.bottom() - size),
            Point::new(rect.right(), rect.bottom()),
            Point::new(rect.right() - size, rect.bottom()),
        ],
        fill: Some(Paint::solid(stroke.color)),
        stroke: None,
    });
}

/// A cylinder lying on its side: a disc at each end, the right one whole.
fn draw_side_cylinder(scene: &mut Scene, rect: Rect, fill: Option<Paint>, stroke: Option<Stroke>) {
    let lip = side_lip(rect);
    let middle = rect.centre().y;
    scene.add(Item::Polygon {
        points: oval(Point::new(rect.left() + lip, middle), lip, rect.height / 2.0),
        fill,
        stroke,
    });
    let body = Rect::new(rect.left() + lip, rect.top(), rect.width - lip * 2.0, rect.height);
    scene.add(Item::Rect { rect: body, radius: 0.0, fill, stroke: None });
    if let Some(stroke) = stroke {
        for y in [rect.top(), rect.bottom()] {
            scene.add(Item::Line {
                points: vec![Point::new(rect.left() + lip, y), Point::new(rect.right() - lip, y)],
                stroke,
                dash: super::scene::Dash::Solid,
            });
        }
    }
    scene.add(Item::Polygon {
        points: oval(Point::new(rect.right() - lip, middle), lip, rect.height / 2.0),
        fill,
        stroke,
    });
}

/// A card: a rectangle with its top left corner cut off.
fn notched(rect: Rect) -> Vec<Point> {
    let cut = (rect.height * 0.35).min(14.0);
    vec![
        Point::new(rect.left() + cut, rect.top()),
        Point::new(rect.right(), rect.top()),
        Point::new(rect.right(), rect.bottom()),
        Point::new(rect.left(), rect.bottom()),
        Point::new(rect.left(), rect.top() + cut),
    ]
}

/// A loop limit: a rectangle with both its top corners cut off.
fn pentagon(rect: Rect) -> Vec<Point> {
    let cut = (rect.height * 0.35).min(16.0);
    vec![
        Point::new(rect.left() + cut, rect.top()),
        Point::new(rect.right() - cut, rect.top()),
        Point::new(rect.right(), rect.top() + cut),
        Point::new(rect.right(), rect.bottom()),
        Point::new(rect.left(), rect.bottom()),
        Point::new(rect.left(), rect.top() + cut),
    ]
}

/// Manual input: the top edge slopes up from left to right.
fn sloped(rect: Rect) -> Vec<Point> {
    vec![
        Point::new(rect.left(), rect.top() + rect.height * 0.3),
        Point::new(rect.right(), rect.top()),
        Point::new(rect.right(), rect.bottom()),
        Point::new(rect.left(), rect.bottom()),
    ]
}

/// A triangle pointing up, or down when `flipped`.
fn triangle(rect: Rect, flipped: bool) -> Vec<Point> {
    let centre = rect.centre().x;
    if flipped {
        vec![
            Point::new(rect.left(), rect.top()),
            Point::new(rect.right(), rect.top()),
            Point::new(centre, rect.bottom()),
        ]
    } else {
        vec![
            Point::new(centre, rect.top()),
            Point::new(rect.right(), rect.bottom()),
            Point::new(rect.left(), rect.bottom()),
        ]
    }
}

/// A half circle on the right of a rectangle, as a list of points.
fn round_right(rect: Rect, across: f32) -> Vec<Point> {
    const STEPS: usize = 12;
    let centre = Point::new(rect.right() - across, rect.centre().y);
    (0..=STEPS)
        .map(|step| {
            let angle =
                -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * step as f32 / STEPS as f32;
            Point::new(centre.x + across * angle.cos(), centre.y + rect.height / 2.0 * angle.sin())
        })
        .collect()
}

/// A delay: square on the left, a half circle on the right.
fn delay(rect: Rect) -> Vec<Point> {
    let across = (rect.height / 2.0).min(rect.width / 2.0);
    let mut points = vec![Point::new(rect.left(), rect.top())];
    points.extend(round_right(rect, across));
    points.push(Point::new(rect.left(), rect.bottom()));
    points
}

/// A display: a point on the left and a curve on the right.
fn display(rect: Rect) -> Vec<Point> {
    let across = side_lip(rect).max(6.0);
    let point = rect.width * 0.1;
    let mut points =
        vec![Point::new(rect.left(), rect.centre().y), Point::new(rect.left() + point, rect.top())];
    points.extend(round_right(rect, across));
    points.push(Point::new(rect.left() + point, rect.bottom()));
    points
}

/// Stored data: a curve bulging out on the left and one cut in on the right.
fn bow_tie(rect: Rect) -> Vec<Point> {
    let across = (rect.width * 0.1).min(rect.height * 0.3);
    const STEPS: usize = 12;
    let mut points = Vec::new();
    // The left side, bottom to top, bulging out.
    for step in 0..=STEPS {
        let angle = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * step as f32 / STEPS as f32;
        points.push(Point::new(
            rect.left() + across + across * angle.cos(),
            rect.centre().y + rect.height / 2.0 * angle.sin(),
        ));
    }
    // The right side, top to bottom, cut in.
    for step in 0..=STEPS {
        let angle =
            -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * step as f32 / STEPS as f32;
        points.push(Point::new(
            rect.right() - across * angle.cos(),
            rect.centre().y + rect.height / 2.0 * angle.sin(),
        ));
    }
    points
}

/// A lightning bolt filling its rectangle.
fn bolt(rect: Rect) -> Vec<Point> {
    let at = |x: f32, y: f32| Point::new(rect.x + rect.width * x, rect.y + rect.height * y);
    vec![
        at(0.62, 0.0),
        at(0.15, 0.55),
        at(0.48, 0.55),
        at(0.35, 1.0),
        at(0.85, 0.42),
        at(0.52, 0.42),
        at(0.7, 0.0),
    ]
}

/// A folder: a tab along the left of its top edge.
fn folder(rect: Rect) -> Vec<Point> {
    let tab = (rect.width * 0.35).min(60.0);
    vec![
        Point::new(rect.left(), rect.top()),
        Point::new(rect.left() + tab, rect.top()),
        Point::new(rect.left() + tab + TAB, rect.top() + TAB),
        Point::new(rect.right(), rect.top() + TAB),
        Point::new(rect.right(), rect.bottom()),
        Point::new(rect.left(), rect.bottom()),
    ]
}

/// A curly brace down the left of `rect`, or down the right when `right`.
fn brace(rect: Rect, right: bool) -> Vec<Point> {
    // Measured from the edge the brace points at: its tip is on that edge, its spine half a brace in,
    // and its two ends a whole brace in.
    let (edge, inward) = if right { (rect.right(), -1.0) } else { (rect.left(), 1.0) };
    let at = |depth: f32, y: f32| Point::new(edge + inward * depth, y);
    let (top, bottom, centre) = (rect.top(), rect.bottom(), rect.centre().y);
    vec![
        at(BRACE, top),
        at(BRACE / 2.0, top + 5.0),
        at(BRACE / 2.0, centre - 5.0),
        at(0.0, centre),
        at(BRACE / 2.0, centre + 5.0),
        at(BRACE / 2.0, bottom - 5.0),
        at(BRACE, bottom),
    ]
}

/// A cylinder: a rectangle with an ellipse at each end, drawn as the top disc and the body.
fn draw_cylinder(scene: &mut Scene, rect: Rect, fill: Option<Paint>, stroke: Option<Stroke>) {
    let lip = (rect.height * 0.18).min(12.0);
    let body = Rect::new(rect.x, rect.y + lip, rect.width, rect.height - lip * 2.0);
    scene.add(Item::Rect { rect: body, radius: 0.0, fill, stroke: None });
    // The two sides, drawn as lines so the discs are not cut across.
    if let Some(stroke) = stroke {
        for x in [rect.left(), rect.right()] {
            scene.add(Item::Line {
                points: vec![Point::new(x, rect.y + lip), Point::new(x, rect.bottom() - lip)],
                stroke,
                dash: super::scene::Dash::Solid,
            });
        }
    }
    // The bottom disc is filled whole and outlined only along its front half: its back half is
    // inside the cylinder, and drawn it ran through the last line of the words (`task-2339`).
    let bottom = Point::new(rect.centre().x, rect.bottom() - lip);
    scene.add(Item::Polygon { points: oval(bottom, rect.width / 2.0, lip), fill, stroke: None });
    if let Some(stroke) = stroke {
        let front: Vec<Point> = (0..=16)
            .map(|step| {
                let angle = step as f32 / 16.0 * std::f32::consts::PI;
                Point::new(bottom.x + rect.width / 2.0 * angle.cos(), bottom.y + lip * angle.sin())
            })
            .collect();
        scene.add(Item::Line { points: front, stroke, dash: super::scene::Dash::Solid });
    }
    scene.add(Item::Polygon {
        points: oval(Point::new(rect.centre().x, rect.y + lip), rect.width / 2.0, lip),
        fill,
        stroke,
    });
}

/// A flattened ellipse, for the ends of a cylinder.
fn oval(centre: Point, across: f32, down: f32) -> Vec<Point> {
    (0..32)
        .map(|step| {
            let angle = step as f32 / 32.0 * std::f32::consts::TAU;
            Point::new(centre.x + across * angle.cos(), centre.y + down * angle.sin())
        })
        .collect()
}

/// A rough closed outline, for a mindmap's bang and cloud.
///
/// The bumps are worked out from the point number rather than drawn at random, so the same node is
/// the same shape every time it is laid out.
fn wobbly(rect: Rect, bumps: usize) -> Vec<Point> {
    let centre = rect.centre();
    let steps = bumps * 4;
    (0..steps)
        .map(|step| {
            let angle = step as f32 / steps as f32 * std::f32::consts::TAU;
            let wave = 1.0 + 0.12 * (angle * bumps as f32).cos();
            Point::new(
                centre.x + rect.width / 2.0 * wave * angle.cos(),
                centre.y + rect.height / 2.0 * wave * angle.sin(),
            )
        })
        .collect()
}

fn diamond(rect: Rect) -> Vec<Point> {
    let centre = rect.centre();
    vec![
        Point::new(centre.x, rect.top()),
        Point::new(rect.right(), centre.y),
        Point::new(centre.x, rect.bottom()),
        Point::new(rect.left(), centre.y),
    ]
}

fn hexagon(rect: Rect) -> Vec<Point> {
    let notch = (rect.width * 0.16).min(rect.height * 0.6);
    vec![
        Point::new(rect.left() + notch, rect.top()),
        Point::new(rect.right() - notch, rect.top()),
        Point::new(rect.right(), rect.centre().y),
        Point::new(rect.right() - notch, rect.bottom()),
        Point::new(rect.left() + notch, rect.bottom()),
        Point::new(rect.left(), rect.centre().y),
    ]
}

/// A parallelogram leaning right, or left when `back` is true.
fn parallelogram(rect: Rect, back: bool) -> Vec<Point> {
    let lean = (rect.width * 0.16).min(24.0);
    if back {
        vec![
            Point::new(rect.left(), rect.top()),
            Point::new(rect.right() - lean, rect.top()),
            Point::new(rect.right(), rect.bottom()),
            Point::new(rect.left() + lean, rect.bottom()),
        ]
    } else {
        vec![
            Point::new(rect.left() + lean, rect.top()),
            Point::new(rect.right(), rect.top()),
            Point::new(rect.right() - lean, rect.bottom()),
            Point::new(rect.left(), rect.bottom()),
        ]
    }
}

/// A trapezoid narrow at the top, or narrow at the bottom when `back` is true.
fn trapezoid(rect: Rect, back: bool) -> Vec<Point> {
    let lean = (rect.width * 0.18).min(28.0);
    if back {
        vec![
            Point::new(rect.left(), rect.top()),
            Point::new(rect.right(), rect.top()),
            Point::new(rect.right() - lean, rect.bottom()),
            Point::new(rect.left() + lean, rect.bottom()),
        ]
    } else {
        vec![
            Point::new(rect.left() + lean, rect.top()),
            Point::new(rect.right() - lean, rect.top()),
            Point::new(rect.right(), rect.bottom()),
            Point::new(rect.left(), rect.bottom()),
        ]
    }
}

/// A rectangle with a point on its left, which is Mermaid's asymmetric node.
fn asymmetric(rect: Rect) -> Vec<Point> {
    let point = (rect.width * 0.14).min(20.0);
    vec![
        Point::new(rect.left(), rect.top()),
        Point::new(rect.right(), rect.top()),
        Point::new(rect.right(), rect.bottom()),
        Point::new(rect.left(), rect.bottom()),
        Point::new(rect.left() + point, rect.centre().y),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The words of a decision fit inside its diamond, for one line and for several. `task-2063`.
    #[test]
    fn a_diamond_holds_its_words() {
        for words in [
            Size::new(40.0, 18.0),
            Size::new(190.0, 18.0),
            Size::new(190.0, 36.0),
            Size::new(120.0, 54.0),
        ] {
            let diamond = Shape::Diamond.size_for(words);
            let taken = words.width / diamond.width + words.height / diamond.height;
            assert!(taken < 0.95, "{words:?} in {diamond:?} takes {taken}");
        }
    }

    #[test]
    fn a_circle_is_as_wide_as_it_is_tall_whatever_its_words() {
        let size = Shape::Circle.size_for(Size::new(120.0, 20.0));
        assert_eq!(size.width, size.height, "a circle is round: {size:?}");
        assert!(size.width >= 120.0, "and still holds its words");
    }

    #[test]
    fn a_diamond_is_roomier_than_a_rectangle_for_the_same_words() {
        // Its corners are empty, so the same words need a bigger box round them.
        let words = Size::new(80.0, 20.0);
        assert!(Shape::Diamond.size_for(words).width > Shape::Rect.size_for(words).width);
        assert!(Shape::Diamond.size_for(words).height > Shape::Rect.size_for(words).height);
    }

    #[test]
    fn every_shape_gives_a_size_that_holds_its_words() {
        let words = Size::new(90.0, 40.0);
        for shape in [
            Shape::Rect,
            Shape::Round,
            Shape::Stadium,
            Shape::Subroutine,
            Shape::Cylinder,
            Shape::Circle,
            Shape::DoubleCircle,
            Shape::Asymmetric,
            Shape::Diamond,
            Shape::Hexagon,
            Shape::Parallelogram,
            Shape::ParallelogramAlt,
            Shape::Trapezoid,
            Shape::TrapezoidAlt,
            Shape::Bang,
            Shape::Cloud,
        ] {
            let size = shape.size_for(words);
            assert!(size.width >= words.width, "{shape:?} is too narrow");
            assert!(size.height >= words.height, "{shape:?} is too short");
        }
    }

    #[test]
    fn every_shape_draws_something_and_nothing_it_draws_is_outside_its_box() {
        // A shape drawn outside the rectangle the layout gave it would overlap its neighbour, which
        // the layout has no way to know about.
        let rect = Rect::new(10.0, 20.0, 120.0, 50.0);
        for shape in [
            Shape::Rect,
            Shape::Round,
            Shape::Stadium,
            Shape::Subroutine,
            Shape::Cylinder,
            Shape::Circle,
            Shape::DoubleCircle,
            Shape::Asymmetric,
            Shape::Diamond,
            Shape::Hexagon,
            Shape::Parallelogram,
            Shape::ParallelogramAlt,
            Shape::Trapezoid,
            Shape::TrapezoidAlt,
            Shape::Marker,
        ] {
            let mut scene = Scene::new();
            shape.draw(
                &mut scene,
                rect,
                Paint::solid(crate::style::Color::WHITE),
                Stroke::new(crate::style::Color::WHITE, 1.0),
            );
            assert!(!scene.is_empty(), "{shape:?} drew nothing");
            for item in &scene.items {
                let bounds = item.bounds();
                assert!(
                    bounds.left() >= rect.left() - 1.0 && bounds.right() <= rect.right() + 1.0,
                    "{shape:?} drew outside its box across: {bounds:?}"
                );
                assert!(
                    bounds.top() >= rect.top() - 1.0 && bounds.bottom() <= rect.bottom() + 1.0,
                    "{shape:?} drew outside its box down: {bounds:?}"
                );
            }
        }
    }

    #[test]
    fn a_line_to_a_diamond_stops_further_in_than_a_line_to_a_rectangle() {
        let rect = Rect::new(0.0, 0.0, 100.0, 60.0);
        let towards = Point::new(200.0, -100.0);
        let square = Shape::Rect.outline(rect).border_towards(towards);
        let diamond = Shape::Diamond.outline(rect).border_towards(towards);
        assert!(
            diamond.distance(rect.centre()) < square.distance(rect.centre()),
            "a diamond's corner is cut off, so the arrow stops sooner"
        );
    }
}
