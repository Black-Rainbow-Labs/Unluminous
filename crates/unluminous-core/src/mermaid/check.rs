//! The properties every diagram type is held to, written once.
//!
//! §11 of `tasks/unluminous-mermaid-plugin-tdd.md` asks that the same four things be true of every
//! diagram Unluminous draws, and that a diagram type added later inherit the list rather than have to
//! remember it. So the list is one function, and every renderer's tests run their scene through it.
//!
//! They are deliberately about the *picture* rather than about the parse. A parser test says the
//! right nodes were read; these say the drawing of them can actually be looked at.
//!
//! **[`properties`] is three of them**, and they are the three that are true of any scene whatever it
//! draws:
//!
//! 1. Nothing is placed outside the scene's own size, so nothing is clipped away.
//! 2. Every number in the scene is finite, so one piece of bad arithmetic cannot quietly poison the
//!    size and blank the whole diagram.
//! 3. Every label the source contained is somewhere in the scene.
//!
//! **Overlap is the fourth and it is per type**, which `task-1984` found this list claiming
//! otherwise. It is [`no_two_rectangles_overlap`], asked for by the types that have a notion of a
//! node box at all: a pie chart's slices overlap the circle they are in by construction, a Gantt bar
//! overlaps its row, and a sequence diagram's messages cross its lifelines, so a shared assertion
//! would have to be switched off by more types than switched it on. [`boxes_nest_or_miss`] is the
//! same question where a box may legitimately be inside another, which is a subgraph.
//!
//! The last, which is about the module rather than about one diagram, is that laying the same source
//! out twice gives exactly the same scene. Everything else here rests on it: without it the
//! screenshot tests are noise.

use super::scene::{Item, Point, Rect, Scene};
use super::{Options, Problem};

/// Run a scene through every property. Panics with which one failed and where.
pub fn properties(scene: &Scene, wanted: &[&str]) {
    inside_the_scene(scene);
    everything_is_finite(scene);
    holds_the_words(scene, wanted);
}

/// Nothing is drawn outside the size the scene reports.
///
/// Text is the exception, and it has to be: this crate cannot measure a string, so a `Text` item
/// claims only its own origin and the renderer widens the scene for the words. So the origin is
/// checked and the run of characters after it is not.
fn inside_the_scene(scene: &Scene) {
    let whole = Rect::new(0.0, 0.0, scene.size.width, scene.size.height);
    for item in &scene.items {
        let bounds = item.bounds();
        let slack = 0.5;
        assert!(
            bounds.left() >= -slack
                && bounds.top() >= -slack
                && bounds.right() <= whole.right() + slack
                && bounds.bottom() <= whole.bottom() + slack,
            "something is drawn outside the {:?} scene: {bounds:?} from {item:?}",
            scene.size
        );
    }
}

/// No two rectangles in the scene sit on top of each other.
///
/// Rectangles only, and that is the point: they are the boxes a reader has to be able to tell apart.
/// A panel drawn behind a label, a frame round a subgraph and a bar in a chart are all rectangles
/// that are *meant* to overlap something, so a renderer that draws those says so by making them
/// something else or by ordering them — which is why this is applied per diagram type, with the
/// renderer's own test choosing whether it applies.
pub fn no_two_rectangles_overlap(rects: &[Rect]) {
    for first in 0..rects.len() {
        for second in first + 1..rects.len() {
            assert!(
                !rects[first].overlaps(&rects[second]),
                "two boxes overlap: {:?} and {:?}",
                rects[first],
                rects[second]
            );
        }
    }
}

/// A weaker form for the diagrams whose boxes are meant to nest: nothing *partly* overlaps.
///
/// A subgraph's frame holds its members and a card sits inside its column, so containment is right
/// and a half-overlap is the fault. Two rectangles either miss each other entirely or one is wholly
/// inside the other.
pub fn boxes_nest_or_miss(rects: &[Rect]) {
    for first in 0..rects.len() {
        for second in first + 1..rects.len() {
            let (a, b) = (rects[first], rects[second]);
            if !a.overlaps(&b) {
                continue;
            }
            assert!(
                contains(&a, &b) || contains(&b, &a),
                "two boxes half overlap, which is neither nesting nor missing: {a:?} and {b:?}"
            );
        }
    }
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    outer.left() <= inner.left() + 0.5
        && outer.top() <= inner.top() + 0.5
        && outer.right() >= inner.right() - 0.5
        && outer.bottom() >= inner.bottom() - 0.5
}

/// Every number in the scene is a real number.
///
/// One division by zero anywhere turns a coordinate into a NaN, and a NaN compared against the
/// scene's size makes the whole diagram nothing. It is worth one loop to be told which item did it.
fn everything_is_finite(scene: &Scene) {
    assert!(
        scene.size.width.is_finite() && scene.size.height.is_finite(),
        "the scene's size is not a number: {:?}",
        scene.size
    );
    let ok = |point: &Point| point.x.is_finite() && point.y.is_finite();
    for item in &scene.items {
        let fine = match item {
            Item::Rect { rect, radius, .. } => {
                rect.x.is_finite()
                    && rect.y.is_finite()
                    && rect.width.is_finite()
                    && rect.height.is_finite()
                    && radius.is_finite()
            }
            Item::Circle { centre, radius, .. } => ok(centre) && radius.is_finite(),
            Item::Polygon { points, .. } | Item::Line { points, .. } => points.iter().all(ok),
            Item::Text { at, style, .. } => ok(at) && style.size.is_finite(),
        };
        assert!(fine, "an item has a number that is not a number: {item:?}");
    }
}

/// Every word the source asked for is somewhere in the scene.
///
/// Two ways of matching, and the second is what makes this usable. A long label is **wrapped** into
/// several lines, each drawn as its own piece of text, so `somebody should look` may well be
/// `somebody should` and `look`. Requiring one piece to hold the whole phrase would make the test
/// fail on correct behaviour and would push every fixture towards short labels, which is exactly the
/// wrong pressure — long labels are where a layout goes wrong.
///
/// So: a single piece containing it, or the pieces run together in the order they were drawn.
fn holds_the_words(scene: &Scene, wanted: &[&str]) {
    let texts = scene.texts();
    let run_together = texts.join(" ");
    for words in wanted {
        let found =
            texts.iter().any(|drawn| drawn.contains(words)) || run_together.contains(words.trim());
        assert!(found, "the scene does not say `{words}`. It says: {texts:?}");
    }
}

/// Lay `text` out twice and assert the two scenes are identical.
///
/// The whole of the screenshot testing rests on this one, for every diagram type.
pub fn is_repeatable(text: &str, options: &Options) {
    let first = super::render(text, options);
    let second = super::render(text, options);
    assert_eq!(first, second, "laying the same source out twice gave two different pictures");
}

/// Lay `text` out, asserting that it does lay out, and check every property.
pub fn drawn(text: &str, options: &Options, wanted: &[&str]) -> Scene {
    let scene = match super::render(text, options) {
        Ok(scene) => scene,
        Err(problem) => panic!("it should have drawn: {}", problem.message()),
    };
    properties(&scene, wanted);
    is_repeatable(text, options);
    assert!(scene.size.width > 0.0 && scene.size.height > 0.0, "the scene has no size");
    scene
}

/// Lay `text` out, expecting it not to.
pub fn refused(text: &str, options: &Options) -> Problem {
    match super::render(text, options) {
        Ok(scene) => panic!("it should have been refused, and drew {} items", scene.items.len()),
        Err(problem) => problem,
    }
}

#[cfg(test)]
mod tests {
    use super::super::scene::{Anchor, TextStyle};
    use super::*;
    use crate::style::Color;

    fn plain_rect(rect: Rect) -> Item {
        Item::Rect { rect, radius: 0.0, fill: None, stroke: None }
    }

    fn plain_text(at: Point, text: &str, size: f32) -> Item {
        Item::Text {
            at,
            text: text.to_owned(),
            style: TextStyle {
                family: "Helvetica".to_owned(),
                size,
                bold: false,
                italic: false,
                color: Color::WHITE,
            },
            anchor: Anchor::Start,
        }
    }

    // Every one of the twenty diagram types is held to this checker, so if it stopped checking
    // nothing would say so. These four are built by hand rather than laid out, so each fails for
    // exactly the reason it names rather than for whatever a renderer happened to produce.

    #[test]
    #[should_panic(expected = "two boxes overlap")]
    fn two_overlapping_node_rectangles_fail() {
        let rects = vec![
            Rect::new(0.0, 0.0, 50.0, 20.0),
            Rect::new(25.0, 0.0, 50.0, 20.0), // twenty five points into the first box
        ];
        no_two_rectangles_overlap(&rects);
    }

    #[test]
    #[should_panic(expected = "not a number")]
    fn a_nan_anywhere_fails() {
        // A `Text` item's bounds are its own origin alone, because this crate cannot measure a
        // string, so a NaN hiding in its *style* passes the "nothing is outside the scene" check
        // and is caught only by the finite-numbers one. Building it this way, rather than putting
        // the NaN in a rectangle, is what makes this a test of that check and not of the other one.
        let mut scene = Scene::new();
        scene.add(plain_rect(Rect::new(0.0, 0.0, 100.0, 40.0)));
        scene.add(plain_text(Point::new(10.0, 10.0), "caption", f32::NAN));
        properties(&scene, &["caption"]);
    }

    #[test]
    #[should_panic(expected = "does not say")]
    fn a_missing_source_label_fails() {
        let mut scene = Scene::new();
        scene.add(plain_rect(Rect::new(0.0, 0.0, 100.0, 40.0)));
        scene.add(plain_text(Point::new(10.0, 10.0), "Start", 12.0));
        properties(&scene, &["Finish"]);
    }

    #[test]
    fn a_good_scene_passes() {
        let mut scene = Scene::new();
        scene.add(plain_rect(Rect::new(0.0, 0.0, 100.0, 40.0)));
        scene.add(plain_rect(Rect::new(120.0, 0.0, 100.0, 40.0)));
        scene.add(plain_text(Point::new(10.0, 10.0), "Start", 12.0));
        scene.add(plain_text(Point::new(130.0, 10.0), "Finish", 12.0));

        properties(&scene, &["Start", "Finish"]);
        no_two_rectangles_overlap(&scene.rects());
    }
}
