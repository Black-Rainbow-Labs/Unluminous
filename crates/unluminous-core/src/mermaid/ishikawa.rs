//! Ishikawa diagrams: `ishikawa`, `ishikawa-beta` and `fishbone`.
//!
//! A fishbone, facing right. The first line is the problem and it goes in a box at the head, at the
//! right end of a horizontal spine. Each line indented under it is a category, drawn as a bone that
//! leaves the spine on a slant, alternating above and below, with the category's name in a box at the
//! bone's outer end. Each cause under a category is a short horizontal line off the bone with its
//! words on it, and a cause under a cause hangs from its parent's line on a short vertical joint.
//!
//! Decisions a reader might disagree with:
//!
//! - **The words sit above their line**, on the screen, for bones on both sides. Rows are packed against
//!   the spine and the joint of a cause with sub causes runs downwards from its line, so a joint never
//!   passes through the words of the cause it belongs to.
//! - **Every bone on one side has the same length**, the length the busiest bone on that side needs, so
//!   the category boxes line up.
//! - **Bones are spaced by the room their causes need.** Each bone is measured to the left and to the
//!   right of the point where it meets the spine, including its category box. Neighbours on the same
//!   side are placed so those extents do not meet, and bones on opposite sides are only kept a little
//!   apart, because they cannot touch.
//! - **A cause line sits clear of the bone beside its words.** A bone above the spine leans left as it
//!   rises, so the words, which sit higher than their line, are kept far enough from the bone to clear
//!   it.

use crate::style::CharStyle;

use super::parts;
use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Stroke};
use super::source::{self, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// How far a bone moves to the left for each point it rises away from the spine.
const SLOPE: f32 = 0.45;
/// The shortest a bone is, from the spine to its outer end.
const MIN_HEIGHT: f32 = 90.0;
/// The least distance between the points where two bones meet the spine.
const MIN_STEP: f32 = 36.0;
/// Space kept between the extents of two neighbouring bones.
const GAP: f32 = 18.0;
/// Space between the last bone and the head.
const HEAD_GAP: f32 = 28.0;
/// How far the spine runs on to the left of the first bone.
const TAIL: f32 = 34.0;
/// How far a joint is from the left end of the line it hangs from.
const JOINT_INSET: f32 = 12.0;

/// One line under a category: a cause, or a cause of a cause.
#[derive(Debug, Clone, PartialEq)]
struct Entry {
    /// Zero for a cause on the bone itself, one for a cause of that cause, and so on.
    depth: usize,
    text: String,
}

#[derive(Debug, Clone, PartialEq)]
struct Category {
    name: String,
    /// Every cause under it, in the order written, each with its depth.
    causes: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Diagram {
    problem: String,
    categories: Vec<Category>,
}

/// A cause's horizontal line and its words, with `x` relative to where the bone meets the spine and
/// `y` relative to the spine.
#[derive(Debug, Clone, PartialEq)]
struct Tick {
    y: f32,
    left: f32,
    right: f32,
    text: String,
    width: f32,
    /// The index of the cause this one hangs from.
    parent: Option<usize>,
}

/// One bone, laid out around its own origin, which is the point where it meets the spine.
#[derive(Debug, Clone)]
struct Bone {
    index: usize,
    above: bool,
    ticks: Vec<Tick>,
    /// Vertical joints from a cause to the last of its sub causes: x, top y, bottom y.
    joints: Vec<(f32, f32, f32)>,
    outer: Point,
    name: Label,
    name_rect: Rect,
    /// How far left of its origin anything on the bone reaches, as a positive number.
    reach_left: f32,
    /// How far right of its origin anything on the bone reaches.
    reach_right: f32,
}

/// The measurements every bone shares.
struct Fonts {
    cause: CharStyle,
    name: CharStyle,
    /// One line of a cause's text.
    line: f32,
    /// The vertical room one cause takes.
    row: f32,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    Ok(draw(&diagram, source, options))
}

/// Read the problem and the tree of causes under it.
fn read(source: &Source) -> Result<Diagram, Problem> {
    let lines = source.statements();
    let Some((first, rest)) = lines.split_first() else {
        return Err(Problem::whole(
            "an Ishikawa diagram begins with the problem on the line after its keyword.",
        ));
    };
    let mut diagram = Diagram { problem: source::label(&first.text), categories: Vec::new() };
    let mut indents: Vec<usize> = Vec::new();
    for line in rest {
        while indents.last().is_some_and(|last| *last >= line.indent) {
            indents.pop();
        }
        let depth = indents.len();
        indents.push(line.indent);
        let text = source::label(&line.text);
        if text.is_empty() {
            return Err(Problem::at(line, "a cause needs some words."));
        }
        if depth == 0 {
            diagram.categories.push(Category { name: text, causes: Vec::new() });
        } else if let Some(category) = diagram.categories.last_mut() {
            category.causes.push(Entry { depth: depth - 1, text });
        }
    }
    Ok(diagram)
}

/// The fonts and row height every bone is measured with.
fn fonts(options: &Options) -> Fonts {
    let cause = options.style(0.9, false);
    let name = options.style(1.0, true);
    let line = text::measure_unwrapped("X", &cause, options.metrics).line_height;
    Fonts { cause, name, line, row: line + 8.0 }
}

/// Lay out one category's bone, `height` long, above or below the spine.
fn place_bone(
    category: &Category,
    index: usize,
    above: bool,
    height: f32,
    fonts: &Fonts,
    options: &Options,
) -> Bone {
    let count = category.causes.len();
    let mut ticks: Vec<Tick> = Vec::with_capacity(count);
    let mut latest: Vec<usize> = Vec::new();
    for (position, entry) in category.causes.iter().enumerate() {
        let y = if above {
            -((count - position) as f32) * fonts.row
        } else {
            (position + 1) as f32 * fonts.row
        };
        let width = text::width_of(&entry.text, &fonts.cause, options.metrics);
        let depth = entry.depth.min(latest.len());
        let parent = depth.checked_sub(1).map(|up| latest[up]);
        let (right, gap) = match parent {
            None => (-SLOPE * y.abs(), if above { SLOPE * (fonts.line + 2.0) + 4.0 } else { 6.0 }),
            Some(up) => (joint_x(&ticks[up]), 7.0),
        };
        let left = right - (width + gap + 3.0);
        latest.truncate(depth);
        latest.push(position);
        ticks.push(Tick { y, left, right, text: entry.text.clone(), width, parent });
    }
    let joints = joints_of(&ticks);
    let sign = if above { -1.0 } else { 1.0 };
    let outer = Point::new(-SLOPE * height, sign * height);
    let name = text::measure_unwrapped(&category.name, &fonts.name, options.metrics);
    let (box_width, box_height) =
        (name.width + parts::PADDING_X * 2.0, name.height + parts::PADDING_Y * 2.0);
    let box_top = if above { outer.y - box_height } else { outer.y };
    let name_rect = Rect::new(outer.x - box_width / 2.0, box_top, box_width, box_height);
    let reach_left = ticks.iter().map(|tick| -tick.left).fold(-name_rect.left(), f32::max).max(0.0);
    let reach_right = name_rect.right().max(0.0);
    Bone { index, above, ticks, joints, outer, name, name_rect, reach_left, reach_right }
}

/// The x of the joint that sub causes of `tick` hang from.
fn joint_x(tick: &Tick) -> f32 {
    tick.left + JOINT_INSET.min((tick.right - tick.left) / 2.0)
}

/// A vertical joint for every cause that has sub causes, running to its last descendant.
fn joints_of(ticks: &[Tick]) -> Vec<(f32, f32, f32)> {
    let mut last: Vec<f32> = ticks.iter().map(|tick| tick.y).collect();
    for (position, tick) in ticks.iter().enumerate() {
        let mut ancestor = tick.parent;
        while let Some(up) = ancestor {
            last[up] = last[up].max(ticks[position].y);
            ancestor = ticks[up].parent;
        }
    }
    ticks
        .iter()
        .enumerate()
        .filter(|(position, tick)| last[*position] > tick.y)
        .map(|(position, tick)| (joint_x(tick), tick.y, last[position]))
        .collect()
}

/// Where each bone meets the spine, left to right, so that no two bones' contents meet.
fn spread(bones: &[Bone]) -> Vec<f32> {
    let mut positions: Vec<f32> = Vec::with_capacity(bones.len());
    let mut last_on_side: [Option<usize>; 2] = [None, None];
    for (index, bone) in bones.iter().enumerate() {
        let side = usize::from(!bone.above);
        let mut at = positions.last().map_or(0.0, |previous| previous + MIN_STEP);
        if let Some(before) = last_on_side[side] {
            let needed = positions[before] + bones[before].reach_right + bone.reach_left + GAP;
            at = at.max(needed);
        }
        positions.push(at);
        last_on_side[side] = Some(index);
    }
    positions
}

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let mut scene = Scene::new();
    let fonts = fonts(options);
    let heights = side_heights(diagram, &fonts);
    let bones: Vec<Bone> = diagram
        .categories
        .iter()
        .enumerate()
        .map(|(index, category)| {
            let above = index % 2 == 0;
            let height = heights[usize::from(!above)];
            place_bone(category, index, above, height, &fonts, options)
        })
        .collect();
    let positions = spread(&bones);
    let head_style = options.style(1.1, true);
    let head = text::measure(&diagram.problem, &head_style, options.metrics, text::WRAP);
    let head_size = (head.width + parts::PADDING_X * 2.5, head.height + parts::PADDING_Y * 2.5);

    let start = positions.first().map_or(0.0, |first| first - TAIL);
    let head_left = positions
        .iter()
        .zip(&bones)
        .map(|(at, bone)| at + bone.reach_right)
        .fold(start + TAIL, f32::max)
        + HEAD_GAP;
    let leftmost =
        positions.iter().zip(&bones).map(|(at, bone)| at - bone.reach_left).fold(start, f32::min);
    let shift_x = parts::MARGIN - leftmost;
    let total_width = head_left + head_size.0 + shift_x + parts::MARGIN;

    let top = parts::title(&mut scene, source, options, total_width);
    let above_room = bones
        .iter()
        .filter(|bone| bone.above)
        .map(|bone| -bone.name_rect.top())
        .fold(head_size.1 / 2.0, f32::max);
    let spine_y = top + parts::MARGIN + above_room;
    let origin = Point::new(shift_x, spine_y);

    let spine = Stroke::new(options.theme.line, parts::THICK);
    scene.add(Item::Line {
        points: vec![
            Point::new(origin.x + start, origin.y),
            Point::new(origin.x + head_left, origin.y),
        ],
        stroke: spine,
        dash: Dash::Solid,
    });
    for (bone, at) in bones.iter().zip(&positions) {
        draw_bone(&mut scene, bone, Point::new(origin.x + at, origin.y), &fonts, options);
    }
    let head_rect =
        Rect::new(origin.x + head_left, origin.y - head_size.1 / 2.0, head_size.0, head_size.1);
    scene.add(Item::Rect {
        rect: head_rect,
        radius: 8.0,
        fill: Some(options.theme.node_fill),
        stroke: Some(Stroke::new(options.theme.accent, parts::LINE * 1.5)),
    });
    parts::centred_label(
        &mut scene,
        &head,
        head_rect,
        &parts::text_style(options, 1.1, true, options.theme.text),
    );
    parts::finish(&mut scene);
    scene
}

/// The length of the bones above the spine and of those below it: the most any one of them needs.
fn side_heights(diagram: &Diagram, fonts: &Fonts) -> [f32; 2] {
    let mut heights = [MIN_HEIGHT; 2];
    for (index, category) in diagram.categories.iter().enumerate() {
        let side = index % 2;
        let needed = (category.causes.len() + 1) as f32 * fonts.row;
        heights[side] = heights[side].max(needed);
    }
    heights
}

/// Draw one bone, its category box and its causes, with `at` as the point where it meets the spine.
fn draw_bone(scene: &mut Scene, bone: &Bone, at: Point, fonts: &Fonts, options: &Options) {
    let colour = options.theme.series(bone.index);
    let stroke = Stroke::new(colour, parts::LINE);
    let moved = |point: Point| Point::new(at.x + point.x, at.y + point.y);
    scene.add(Item::Line {
        points: vec![moved(bone.outer), at],
        stroke: Stroke::new(colour, parts::LINE * 1.5),
        dash: Dash::Solid,
    });
    let name_rect = bone.name_rect.moved(at.x, at.y);
    scene.add(Item::Rect {
        rect: name_rect,
        radius: parts::CORNER,
        fill: Some(Paint::faded(colour, 60)),
        stroke: Some(Stroke::new(colour, parts::LINE)),
    });
    parts::centred_label(
        scene,
        &bone.name,
        name_rect,
        &parts::text_style(options, 1.0, true, options.theme.text),
    );
    for &(x, from, to) in &bone.joints {
        scene.add(Item::Line {
            points: vec![Point::new(at.x + x, at.y + from), Point::new(at.x + x, at.y + to)],
            stroke,
            dash: Dash::Solid,
        });
    }
    let style = parts::text_style(options, 0.9, false, options.theme.text);
    for tick in &bone.ticks {
        let y = at.y + tick.y;
        scene.add(Item::Line {
            points: vec![Point::new(at.x + tick.left, y), Point::new(at.x + tick.right, y)],
            stroke,
            dash: Dash::Solid,
        });
        parts::one_line(
            scene,
            &tick.text,
            Point::new(at.x + tick.left + 3.0, y - fonts.line - 2.0),
            &style,
            Anchor::Start,
            tick.width,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::{check, Options};
    use super::*;
    use crate::metrics::FixedMetrics;

    fn options() -> Options<'static> {
        Options::new(Box::leak(Box::new(FixedMetrics::default())))
    }

    fn diagram(text: &str) -> Diagram {
        read(&Source::read(text).expect("a diagram")).expect("it should read")
    }

    const PHOTO: &str = "ishikawa-beta\n    Blurry Photo\n    Process\n        Out of focus\n\
        \x20       Shutter speed too slow\n        Protective film not removed\n\
        \x20       Beautification filter applied\n    User\n        Shaky hands\n    Equipment\n\
        \x20       LENS\n            Inappropriate lens\n            Damaged lens\n\
        \x20           Dirty lens\n        SENSOR\n            Damaged sensor\n\
        \x20           Dirty sensor\n    Environment\n        Subject moved too quickly\n\
        \x20       Too dark\n";

    #[test]
    fn the_first_line_is_the_problem_and_indentation_nests_the_causes() {
        let diagram = diagram(PHOTO);
        assert_eq!(diagram.problem, "Blurry Photo");
        let names: Vec<&str> = diagram.categories.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Process", "User", "Equipment", "Environment"]);
        let equipment = &diagram.categories[2];
        let depths: Vec<usize> = equipment.causes.iter().map(|cause| cause.depth).collect();
        assert_eq!(depths, vec![0, 1, 1, 1, 0, 1, 1], "LENS and SENSOR hold sub causes");
    }

    #[test]
    fn the_fishbone_keyword_is_drawn_like_the_others() {
        check::drawn("fishbone\n  Late\n  People\n    Tired\n", &options(), &["Late", "Tired"]);
        check::drawn("ishikawa\n  Late\n  People\n    Tired\n", &options(), &["People"]);
    }

    #[test]
    fn a_diagram_with_nothing_in_it_is_refused_with_a_plain_sentence() {
        let problem = check::refused("ishikawa-beta\n", &options());
        assert!(problem.reason.contains("problem"), "{}", problem.reason);
    }

    #[test]
    fn a_problem_with_no_causes_is_just_the_head() {
        check::drawn("ishikawa-beta\n  Only a problem\n", &options(), &["Only a problem"]);
    }

    #[test]
    fn the_whole_example_is_drawn_and_keeps_every_property() {
        let scene = check::drawn(
            PHOTO,
            &options(),
            &["Blurry Photo", "Process", "Dirty sensor", "Too dark", "SENSOR"],
        );
        check::no_two_rectangles_overlap(&scene.rects());
    }

    /// The rectangle a text item covers, measured with the same stub the renderer used.
    fn text_rect(item: &Item, options: &Options) -> Option<(String, Rect)> {
        let Item::Text { at, text, style, anchor } = item else {
            return None;
        };
        let measure = CharStyle { size: style.size, bold: style.bold, ..options.base.clone() };
        let width = text::width_of(text, &measure, options.metrics);
        let height = text::measure_unwrapped("X", &measure, options.metrics).line_height;
        let left = match anchor {
            Anchor::Start => at.x,
            Anchor::Middle => at.x - width / 2.0,
            Anchor::End => at.x - width,
        };
        Some((text.clone(), Rect::new(left, at.y, width, height)))
    }

    fn crosses(points: &[Point], rect: &Rect) -> bool {
        let inner = Rect::new(rect.x + 1.0, rect.y + 1.0, rect.width - 2.0, rect.height - 2.0);
        points.windows(2).any(|pair| {
            let steps = (pair[0].distance(pair[1]) as usize).max(1);
            (0..=steps)
                .any(|step| inner.contains(pair[0].towards(pair[1], step as f32 / steps as f32)))
        })
    }

    #[test]
    fn no_words_touch_other_words_and_no_line_crosses_any_words() {
        let options = options();
        let scene = check::drawn(PHOTO, &options, &[]);
        let words: Vec<(String, Rect)> =
            scene.items.iter().filter_map(|item| text_rect(item, &options)).collect();
        for first in 0..words.len() {
            for second in first + 1..words.len() {
                assert!(
                    !words[first].1.overlaps(&words[second].1),
                    "`{}` overlaps `{}`",
                    words[first].0,
                    words[second].0
                );
            }
        }
        for item in &scene.items {
            if let Item::Line { points, .. } = item {
                for (text, rect) in &words {
                    assert!(!crosses(points, rect), "a line crosses `{text}`");
                }
            }
        }
    }

    #[test]
    fn a_crowded_bone_pushes_its_neighbour_along_the_spine() {
        let text =
            "ishikawa-beta\n  P\n  A\n    a very long cause that needs a great deal of room\n\
            \x20 B\n    b\n  C\n    another very long cause that also needs a great deal of room\n";
        let options = options();
        let scene =
            check::drawn(text, &options, &["a very long cause that needs a great deal of room"]);
        let words: Vec<(String, Rect)> =
            scene.items.iter().filter_map(|item| text_rect(item, &options)).collect();
        for first in 0..words.len() {
            for second in first + 1..words.len() {
                assert!(!words[first].1.overlaps(&words[second].1));
            }
        }
    }

    #[test]
    fn a_cause_hangs_from_its_parent_on_a_joint() {
        let scene = check::drawn(
            "ishikawa-beta\n  P\n  A\n    one\n      two\n      three\n    four\n",
            &options(),
            &["two"],
        );
        let verticals = scene
            .items
            .iter()
            .filter(|item| {
                matches!(item, Item::Line { points, .. }
                if points.len() == 2 && (points[0].x - points[1].x).abs() < 0.01)
            })
            .count();
        assert_eq!(verticals, 1, "one joint, from `one` down to `three`");
    }

    #[test]
    fn a_source_cut_off_anywhere_never_panics() {
        let options = options();
        for end in 0..=PHOTO.len() {
            if let Some(part) = PHOTO.get(..end) {
                let _ = super::super::render(part, &options);
            }
        }
    }
}
