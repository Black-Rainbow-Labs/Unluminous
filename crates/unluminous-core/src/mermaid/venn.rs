//! Venn diagrams: `venn-beta`.
//!
//! Sets are translucent circles, and a union is the part where several of them overlap. A set has a
//! name, a size that scales its circle, and any number of `text` lines that are listed under the name.
//!
//! Where the circles go:
//!
//! - one set is a circle;
//! - two overlap by a third of the smaller one's diameter;
//! - three have their centres on a triangle whose sides are a little over half of the two radii added
//!   together, which is the classic arrangement;
//! - four or more have their centres on a ring, each overlapping the two beside it.
//!
//! A size scales a circle's area, so the radius goes with its square root, and no circle is smaller
//! than a little over half of the largest.
//!
//! **Every name sits in the part of the picture that belongs to it.** A set's name goes where no other
//! circle covers, and a union's label goes where exactly the circles of the union overlap. The block of
//! words (the name and its `text` lines) is a rectangle, so the search tries a grid of positions and
//! keeps the one whose rectangle has the most room to spare on every side. When no position holds the
//! block, the whole picture is made larger and the search is made again, because the words keep their
//! size and the circles do not. Blocks in different regions cannot touch, because the regions do not
//! share any area. A union whose circles do not overlap has no region at all, and its words are put
//! between the circles instead of growing the picture for ever.

use super::parts;
use super::scene::{Anchor, Item, Point, Rect, Scene, Size, Stroke};
use super::source::{self, Line, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// The most sets one diagram may have. More than this is a ring too crowded to read.
const MOST_SETS: usize = 12;
/// The size of a set that was given none.
const DEFAULT_SIZE: f32 = 10.0;
/// The radius of the largest circle when nothing needs the picture to be larger.
const BASE_RADIUS: f32 = 90.0;
/// How small the smallest circle may be, as a share of the largest.
const SMALLEST_SHARE: f32 = 0.55;
/// How much larger the whole picture is made each time the words do not fit.
const GROWTH: f32 = 1.15;
/// How many times the picture may be made larger. 1.15 to this power is a little over five.
const MOST_GROWTHS: usize = 12;
/// How wide a name or a `text` line may be before it wraps.
const BLOCK_WRAP: f32 = 150.0;
/// Space round a block of words, so it does not touch the edge of its region.
const BLOCK_PAD: f32 = 4.0;
/// How far inside its region every part of a block must be.
const EDGE: f32 = 2.0;
/// The space between the name and the first `text` line, and between `text` lines.
const NAME_GAP: f32 = 6.0;
const NODE_GAP: f32 = 3.0;
/// How far apart the points tested inside a block are. Small enough that no other circle can slip
/// between two of them.
const PROBE: f32 = 8.0;

/// A set or a union, and the words in it.
#[derive(Debug, Clone, PartialEq)]
struct Area {
    /// The sets this area is the overlap of, as indexes into [`Diagram::sizes`]. A set has one.
    members: Vec<usize>,
    /// What is written in it. A set without a label is named by its identifier.
    label: Option<String>,
    /// The `text` lines under it.
    nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Diagram {
    title: Option<String>,
    /// The identifier of each set, in the order they were written.
    ids: Vec<String>,
    /// The size each set was given, if it was given one.
    sizes: Vec<Option<f32>>,
    /// Every set and union in the order written.
    areas: Vec<Area>,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    if diagram.ids.is_empty() {
        return Err(Problem::whole("a Venn diagram needs at least one `set`."));
    }
    Ok(draw(&diagram, source, options))
}

/// What a `set`, `union` or `text` line declares.
struct Declaration {
    names: Vec<String>,
    label: Option<String>,
    size: Option<f32>,
}

fn read(source: &Source) -> Result<Diagram, Problem> {
    let mut diagram = Diagram::default();
    let mut current: Option<usize> = None;
    for line in source.statements() {
        if let Some(rest) = line.after_word("title") {
            diagram.title = Some(source::label(rest));
        } else if ["style", "classDef", "class", "linkStyle"]
            .iter()
            .any(|word| line.starts_with_word(word))
        {
            continue;
        } else if let Some(rest) = line.after_word("set") {
            current = Some(read_set(&mut diagram, line, rest)?);
        } else if let Some(rest) = line.after_word("union") {
            current = Some(read_union(&mut diagram, line, rest)?);
        } else if let Some(rest) = line.after_word("text") {
            read_text(&mut diagram, current, line, rest)?;
        } else {
            return Err(Problem::at(
                line,
                "a line is `set A[\"Label\"]`, `union A,B[\"Label\"]`, an indented `text T[\"Label\"]`, or a `title`.",
            ));
        }
    }
    Ok(diagram)
}

/// A `set` line. Returns the index of the area it made.
fn read_set(diagram: &mut Diagram, line: &Line, rest: &str) -> Result<usize, Problem> {
    let declaration = declaration(rest).map_err(|reason| Problem::at(line, reason))?;
    let [id] = declaration.names.as_slice() else {
        return Err(Problem::at(line, "a set has one name. Use `union` to name an overlap."));
    };
    if diagram.ids.contains(id) {
        return Err(Problem::at(line, format!("there is already a set called `{id}`.")));
    }
    if diagram.ids.len() >= MOST_SETS {
        return Err(Problem::at(
            line,
            format!("Unluminous draws {MOST_SETS} sets at most, and this is the next one."),
        ));
    }
    let label = declaration.label.unwrap_or_else(|| id.clone());
    diagram.ids.push(id.clone());
    diagram.sizes.push(declaration.size);
    diagram.areas.push(Area {
        members: vec![diagram.ids.len() - 1],
        label: Some(label),
        nodes: Vec::new(),
    });
    Ok(diagram.areas.len() - 1)
}

/// A `union` line. Returns the index of the area it made.
fn read_union(diagram: &mut Diagram, line: &Line, rest: &str) -> Result<usize, Problem> {
    let declaration = declaration(rest).map_err(|reason| Problem::at(line, reason))?;
    let mut members = Vec::new();
    for name in &declaration.names {
        let Some(index) = diagram.ids.iter().position(|id| id == name) else {
            return Err(Problem::at(
                line,
                format!("`{name}` is not a set. A union names sets written on earlier lines."),
            ));
        };
        if !members.contains(&index) {
            members.push(index);
        }
    }
    if members.len() < 2 {
        return Err(Problem::at(line, "a union names two or more sets, such as `union A,B`."));
    }
    members.sort_unstable();
    diagram.areas.push(Area { members, label: declaration.label, nodes: Vec::new() });
    Ok(diagram.areas.len() - 1)
}

/// A `text` line, which belongs to the set or union above it.
fn read_text(
    diagram: &mut Diagram,
    current: Option<usize>,
    line: &Line,
    rest: &str,
) -> Result<(), Problem> {
    let declaration = declaration(rest).map_err(|reason| Problem::at(line, reason))?;
    let Some(area) = current.and_then(|index| diagram.areas.get_mut(index)) else {
        return Err(Problem::at(
            line,
            "a `text` line goes under the `set` or `union` it belongs to.",
        ));
    };
    let Some(name) = declaration.names.first() else {
        return Err(Problem::at(line, "a `text` line looks like `text T1[\"Words\"]`."));
    };
    area.nodes.push(declaration.label.unwrap_or_else(|| name.clone()));
    Ok(())
}

/// Read `A,B["Label"]:12`: the names, the label in brackets and the size after a colon.
fn declaration(rest: &str) -> Result<Declaration, String> {
    let (rest, size) = split_size(rest)?;
    let (names, label) = match first_bracket(rest) {
        Some(open) => {
            let inner = rest.get(open + 1..).unwrap_or("").trim_end();
            let Some(inner) = inner.strip_suffix(']') else {
                return Err(
                    "a label looks like [\"Name\"] and needs its closing bracket.".to_owned()
                );
            };
            (rest.get(..open).unwrap_or("").trim(), Some(source::label(inner)))
        }
        None => (rest.trim(), None),
    };
    let names: Vec<String> = source::split_outside_quotes(names, ',')
        .into_iter()
        .map(|name| source::unquote(&name))
        .collect();
    if names.iter().any(String::is_empty) {
        return Err("every set needs a name, and a comma needs a name on both sides.".to_owned());
    }
    Ok(Declaration { names, label, size })
}

/// Take a `:12` off the end, when what follows the last colon is a number.
fn split_size(rest: &str) -> Result<(&str, Option<f32>), String> {
    let Some(colon) = rest.rfind(':') else {
        return Ok((rest, None));
    };
    let tail = rest.get(colon + 1..).unwrap_or("").trim();
    if tail.is_empty() || !tail.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Ok((rest, None));
    }
    match tail.parse::<f32>() {
        Ok(size) if size.is_finite() && size > 0.0 => {
            Ok((rest.get(..colon).unwrap_or("").trim_end(), Some(size)))
        }
        _ => Err(format!("`{tail}` is not a size. A size is a number above zero.")),
    }
}

/// Where the first `[` outside quotes is.
fn first_bracket(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (at, character) in text.char_indices() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '[' => return Some(at),
            None => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------------------------------

/// The measured words of one area: its name and its `text` lines.
struct Block {
    label: Option<Label>,
    nodes: Vec<Label>,
    width: f32,
    height: f32,
}

fn measure_block(area: &Area, options: &Options) -> Option<Block> {
    let bold = options.style(1.0, true);
    let plain = options.style(0.85, false);
    let label = area
        .label
        .as_deref()
        .filter(|words| !words.trim().is_empty())
        .map(|words| text::measure(words, &bold, options.metrics, BLOCK_WRAP));
    let nodes: Vec<Label> = area
        .nodes
        .iter()
        .filter(|words| !words.trim().is_empty())
        .map(|words| text::measure(words, &plain, options.metrics, BLOCK_WRAP))
        .collect();
    if label.is_none() && nodes.is_empty() {
        return None;
    }
    let width = label.iter().chain(nodes.iter()).map(|label| label.width).fold(0.0_f32, f32::max);
    let mut height = label.as_ref().map_or(0.0, |label| label.height);
    if label.is_some() && !nodes.is_empty() {
        height += NAME_GAP;
    }
    height += nodes.iter().map(|node| node.height).sum::<f32>();
    height += NODE_GAP * nodes.len().saturating_sub(1) as f32;
    Some(Block { label, nodes, width, height })
}

// ---------------------------------------------------------------------------------------------------
// Circles
// ---------------------------------------------------------------------------------------------------

type Circle = (Point, f32);

/// The radius of each set at this scale.
fn radii(sizes: &[Option<f32>], scale: f32) -> Vec<f32> {
    let weights: Vec<f32> = sizes.iter().map(|size| size.unwrap_or(DEFAULT_SIZE)).collect();
    let largest = weights.iter().copied().fold(f32::MIN_POSITIVE, f32::max);
    weights
        .iter()
        .map(|weight| {
            let share = (weight / largest).sqrt().max(SMALLEST_SHARE);
            BASE_RADIUS * scale * share
        })
        .collect()
}

/// Where each circle's centre goes: the arrangement for one, two, three, and four or more.
fn centres(radii: &[f32]) -> Vec<Point> {
    match radii {
        [] => Vec::new(),
        [_] => vec![Point::new(0.0, 0.0)],
        [a, b] => {
            let overlap = a.min(*b) * 2.0 / 3.0;
            vec![Point::new(0.0, 0.0), Point::new(a + b - overlap, 0.0)]
        }
        [a, b, c] => {
            let side = |first: f32, second: f32| (first + second) * 0.62;
            let (ab, ac, bc) = (side(*a, *b), side(*a, *c), side(*b, *c));
            let across = (ab * ab + ac * ac - bc * bc) / (2.0 * ab);
            let up = (ac * ac - across * across).max(0.0).sqrt();
            vec![Point::new(0.0, 0.0), Point::new(ab, 0.0), Point::new(across, -up)]
        }
        many => {
            let count = many.len() as f32;
            let mean = many.iter().sum::<f32>() / count;
            let ring = mean * 1.25 / (2.0 * (std::f32::consts::PI / count).sin());
            (0..many.len())
                .map(|index| {
                    let angle =
                        -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * index as f32 / count;
                    Point::new(ring * angle.cos(), ring * angle.sin())
                })
                .collect()
        }
    }
}

/// How far inside the region `point` is: the smallest of how far inside each circle of the area it
/// is and, when `exclusive`, how far outside each of the other circles. Negative means outside.
fn region_margin(point: Point, members: &[usize], circles: &[Circle], exclusive: bool) -> f32 {
    let mut margin = f32::INFINITY;
    for (index, (centre, radius)) in circles.iter().enumerate() {
        let distance = point.distance(*centre);
        if members.contains(&index) {
            margin = margin.min(radius - distance);
        } else if exclusive {
            margin = margin.min(distance - radius);
        }
    }
    margin
}

/// Whether every circle of the area overlaps every other, which is when it has a region at all.
fn has_region(members: &[usize], circles: &[Circle]) -> bool {
    members.iter().all(|&first| {
        members.iter().all(|&second| {
            let (a, ra) = circles[first];
            let (b, rb) = circles[second];
            first == second || a.distance(b) < ra + rb - 2.0 * EDGE
        })
    })
}

/// Where a block goes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Placed {
    /// The words themselves, without the padding.
    rect: Rect,
    /// Whether the rectangle is wholly inside its region.
    fits: bool,
    /// Whether it is also clear of every circle that is not in the area.
    exclusive: bool,
}

/// What the search for a position needs to know.
struct Search<'a> {
    circles: &'a [Circle],
    members: &'a [usize],
    exclusive: bool,
    /// The size of the rectangle to place, padding included.
    size: Size,
    /// Rectangles already placed.
    taken: &'a [Rect],
}

/// The position whose rectangle has the most room to spare, and how much room that is.
fn best_position(search: &Search) -> Option<(Point, f32)> {
    let smallest = search
        .members
        .iter()
        .copied()
        .min_by(|&a, &b| search.circles[a].1.total_cmp(&search.circles[b].1))?;
    let (centre, radius) = search.circles[smallest];
    let step = (radius / 30.0).max(3.0);
    let mut best: Option<(Point, f32)> = None;
    let mut y = centre.y - radius;
    while y <= centre.y + radius {
        let mut x = centre.x - radius;
        while x <= centre.x + radius {
            let at = Point::new(x, y);
            let rect = Rect::around(at, search.size);
            let clear = !search.taken.iter().any(|taken| taken.grown(EDGE).overlaps(&rect));
            if clear {
                let floor = best.map_or(f32::NEG_INFINITY, |(_, score)| score);
                if let Some(score) = rectangle_margin(search, &rect, floor) {
                    best = Some((at, score));
                }
            }
            x += step;
        }
        y += step;
    }
    best
}

/// The room to spare of the worst point of a rectangle, or `None` once it is known not to beat `floor`.
fn rectangle_margin(search: &Search, rect: &Rect, floor: f32) -> Option<f32> {
    let across = (rect.width / PROBE).ceil().max(1.0) as usize;
    let down = (rect.height / PROBE).ceil().max(1.0) as usize;
    let mut worst = f32::INFINITY;
    for row in 0..=down {
        for column in 0..=across {
            let point = Point::new(
                rect.x + rect.width * column as f32 / across as f32,
                rect.y + rect.height * row as f32 / down as f32,
            );
            worst =
                worst.min(region_margin(point, search.members, search.circles, search.exclusive));
            if worst <= floor {
                return None;
            }
        }
    }
    Some(worst)
}

/// Find a position for one block: in the part of the region no other circle covers when there is
/// room, in the whole overlap when there is not, and as near as possible when there is no room.
fn place_block(area: &Area, block: &Block, circles: &[Circle], taken: &[Rect]) -> Placed {
    let size = Size::new(block.width + BLOCK_PAD * 2.0, block.height + BLOCK_PAD * 2.0);
    let words = |at: Point| Rect::around(at, Size::new(block.width, block.height));
    if !has_region(&area.members, circles) {
        let centre = mean_centre(&area.members, circles);
        return Placed { rect: words(centre), fits: false, exclusive: false };
    }
    let mut closest: Option<(Point, f32, bool)> = None;
    for exclusive in [true, false] {
        let search = Search { circles, members: &area.members, exclusive, size, taken };
        let Some((at, score)) = best_position(&search) else {
            continue;
        };
        if score >= EDGE {
            return Placed { rect: words(at), fits: true, exclusive };
        }
        if closest.is_none_or(|(_, known, _)| score > known) {
            closest = Some((at, score, exclusive));
        }
    }
    let at = closest.map_or_else(|| mean_centre(&area.members, circles), |(at, _, _)| at);
    Placed { rect: words(at), fits: false, exclusive: false }
}

/// The middle of the centres of some circles.
fn mean_centre(members: &[usize], circles: &[Circle]) -> Point {
    let count = members.len().max(1) as f32;
    Point::new(
        members.iter().map(|&index| circles[index].0.x).sum::<f32>() / count,
        members.iter().map(|&index| circles[index].0.y).sum::<f32>() / count,
    )
}

/// The picture worked out: the circles, and where each block of words goes.
struct Laid {
    circles: Vec<Circle>,
    /// One for each area, `None` where the area has no words.
    placed: Vec<Option<Placed>>,
    /// How many blocks could not be put inside the region they belong to.
    misplaced: usize,
}

/// Place everything at one scale.
fn place_all(diagram: &Diagram, blocks: &[Option<Block>], scale: f32) -> Laid {
    let radii = radii(&diagram.sizes, scale);
    let circles: Vec<Circle> = centres(&radii).into_iter().zip(radii.iter().copied()).collect();
    // The smaller regions first, because a region with many circles in it has the least room and a
    // block placed earlier is a block the others must avoid.
    let mut order: Vec<usize> = (0..diagram.areas.len()).collect();
    order.sort_by_key(|&index| (std::cmp::Reverse(diagram.areas[index].members.len()), index));
    let mut placed: Vec<Option<Placed>> = vec![None; diagram.areas.len()];
    let mut taken: Vec<Rect> = Vec::new();
    let mut misplaced = 0;
    for index in order {
        let (Some(area), Some(Some(block))) = (diagram.areas.get(index), blocks.get(index)) else {
            continue;
        };
        let spot = place_block(area, block, &circles, &taken);
        taken.push(spot.rect.grown(BLOCK_PAD));
        // An area whose circles do not meet has no region to fit in, and a larger picture would not
        // give it one, so it is not counted.
        if !spot.fits && has_region(&area.members, &circles) {
            misplaced += 1;
        }
        placed[index] = Some(spot);
    }
    Laid { circles, placed, misplaced }
}

/// Work out the picture, growing it until every block of words fits or growing stops helping.
fn lay_out(diagram: &Diagram, blocks: &[Option<Block>]) -> Laid {
    let mut best: Option<Laid> = None;
    let mut scale = 1.0;
    for _ in 0..=MOST_GROWTHS {
        let laid = place_all(diagram, blocks, scale);
        if laid.misplaced == 0 {
            return laid;
        }
        if best.as_ref().is_none_or(|known| laid.misplaced < known.misplaced) {
            best = Some(laid);
        }
        scale *= GROWTH;
    }
    best.unwrap_or_else(|| place_all(diagram, blocks, 1.0))
}

// ---------------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------------

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let blocks: Vec<Option<Block>> =
        diagram.areas.iter().map(|area| measure_block(area, options)).collect();
    let laid = lay_out(diagram, &blocks);
    let (left, top, right, bottom) = bounds(&laid);
    let width = right - left;

    let mut scene = Scene::new();
    let mut titled = source.clone();
    if titled.title.is_none() {
        titled.title.clone_from(&diagram.title);
    }
    let head = parts::title(&mut scene, &titled, options, width + parts::MARGIN * 2.0);
    let (dx, dy) = (parts::MARGIN - left, head + parts::MARGIN - top);

    for (index, (centre, radius)) in laid.circles.iter().enumerate() {
        scene.add(Item::Circle {
            centre: Point::new(centre.x + dx, centre.y + dy),
            radius: *radius,
            fill: Some(options.theme.wash(index, 70)),
            stroke: Some(Stroke::new(options.theme.series(index), parts::LINE)),
        });
    }
    for (placed, block) in laid.placed.iter().zip(&blocks) {
        if let (Some(placed), Some(block)) = (placed, block) {
            draw_block(&mut scene, block, placed.rect.moved(dx, dy), options);
        }
    }
    scene.claim(Rect::new(
        0.0,
        0.0,
        width + parts::MARGIN * 2.0,
        head + parts::MARGIN * 2.0 + (bottom - top),
    ));
    parts::finish(&mut scene);
    scene
}

/// The box round every circle and every block of words.
fn bounds(laid: &Laid) -> (f32, f32, f32, f32) {
    let mut all = Rect::new(0.0, 0.0, 0.0, 0.0);
    let mut first = true;
    let mut take = |rect: Rect| {
        all = if first { rect } else { all.union(&rect) };
        first = false;
    };
    for (centre, radius) in &laid.circles {
        take(Rect::new(centre.x - radius, centre.y - radius, radius * 2.0, radius * 2.0));
    }
    for placed in laid.placed.iter().flatten() {
        take(placed.rect);
    }
    (all.left(), all.top(), all.right(), all.bottom())
}

/// Write a block of words centred in `rect`: the name, then the `text` lines under it.
fn draw_block(scene: &mut Scene, block: &Block, rect: Rect, options: &Options) {
    let name = parts::text_style(options, 1.0, true, options.theme.text);
    let plain = parts::text_style(options, 0.85, false, options.theme.text);
    let middle = rect.centre().x;
    let mut y = rect.top();
    if let Some(label) = &block.label {
        parts::label_at(scene, label, Point::new(middle, y), &name, Anchor::Middle);
        y += label.height;
        if !block.nodes.is_empty() {
            y += NAME_GAP;
        }
    }
    for node in &block.nodes {
        parts::label_at(scene, node, Point::new(middle, y), &plain, Anchor::Middle);
        y += node.height + NODE_GAP;
    }
}

#[cfg(test)]
mod tests {
    use super::super::check;
    use super::*;
    use crate::metrics::FixedMetrics;

    fn options() -> Options<'static> {
        Options::new(Box::leak(Box::new(FixedMetrics::default())))
    }

    fn diagram(text: &str) -> Diagram {
        read(&Source::read(text).expect("a diagram")).expect("it should read")
    }

    /// Read and lay out `text`, giving back everything a test wants to look at.
    fn analyse(text: &str) -> (Diagram, Vec<Option<Block>>, Laid) {
        let diagram = diagram(text);
        let options = options();
        let blocks: Vec<Option<Block>> =
            diagram.areas.iter().map(|area| measure_block(area, &options)).collect();
        let laid = lay_out(&diagram, &blocks);
        (diagram, blocks, laid)
    }

    /// Every block is inside its region, and no two blocks touch.
    fn assert_words_are_in_their_regions(text: &str) {
        let (diagram, blocks, laid) = analyse(text);
        assert_eq!(laid.misplaced, 0, "some words do not fit their region in:\n{text}");
        let mut rects = Vec::new();
        for (index, placed) in laid.placed.iter().enumerate() {
            let Some(placed) = placed else { continue };
            let area = &diagram.areas[index];
            assert!(placed.fits, "area {index} did not fit");
            let rect = placed.rect;
            let corners = [
                Point::new(rect.left(), rect.top()),
                Point::new(rect.right(), rect.top()),
                Point::new(rect.left(), rect.bottom()),
                Point::new(rect.right(), rect.bottom()),
                rect.centre(),
            ];
            for corner in corners {
                let margin = region_margin(corner, &area.members, &laid.circles, placed.exclusive);
                assert!(margin >= 0.0, "area {index} has words outside its region at {corner:?}");
            }
            assert!(blocks[index].is_some());
            rects.push(rect);
        }
        check::no_two_rectangles_overlap(&rects);
    }

    #[test]
    fn sets_unions_sizes_and_text_nodes_are_all_read() {
        let diagram = diagram(
            "venn-beta\n title \"Team\"\n set A[\"Alpha\"]:20\n   text A1[\"React\"]\n set \"B B\"\n union A,\"B B\"[\"Shared\"]:3\n   text X\n",
        );
        assert_eq!(diagram.title.as_deref(), Some("Team"));
        assert_eq!(diagram.ids, vec!["A", "B B"]);
        assert_eq!(diagram.sizes, vec![Some(20.0), None]);
        assert_eq!(diagram.areas[0].label.as_deref(), Some("Alpha"));
        assert_eq!(diagram.areas[0].nodes, vec!["React"]);
        assert_eq!(
            diagram.areas[1].label.as_deref(),
            Some("B B"),
            "a set without a label is its name"
        );
        assert_eq!(diagram.areas[2].members, vec![0, 1]);
        assert_eq!(
            diagram.areas[2].nodes,
            vec!["X"],
            "a text node without a label is its identifier"
        );
    }

    #[test]
    fn style_lines_are_skipped() {
        let diagram = diagram("venn-beta\n set A\n style A fill:#ff6b6b\n style A,B color:#333\n");
        assert_eq!(diagram.areas.len(), 1);
    }

    #[test]
    fn a_union_of_an_unknown_set_is_refused_with_its_line() {
        let problem = check::refused("venn-beta\n set A\n union A,Z[\"x\"]\n", &options());
        assert_eq!(problem.line, Some(3));
        assert!(problem.reason.contains("`Z`"));
    }

    #[test]
    fn a_union_of_one_set_is_refused() {
        let problem = check::refused("venn-beta\n set A\n union A\n", &options());
        assert!(problem.reason.contains("two or more"));
    }

    #[test]
    fn a_text_line_before_any_set_is_refused() {
        let problem = check::refused("venn-beta\n text T[\"x\"]\n set A\n", &options());
        assert_eq!(problem.line, Some(2));
    }

    #[test]
    fn a_repeated_set_a_bad_size_and_a_stray_line_are_refused() {
        assert!(check::refused("venn-beta\n set A\n set A\n", &options())
            .reason
            .contains("already"));
        assert!(check::refused("venn-beta\n set A:0\n", &options()).reason.contains("size"));
        assert!(check::refused("venn-beta\n set A[\"x\n", &options()).reason.contains("bracket"));
        assert_eq!(check::refused("venn-beta\n circle A\n", &options()).line, Some(2));
        assert!(check::refused("venn-beta\n title Nothing\n", &options())
            .reason
            .contains("at least one"));
    }

    #[test]
    fn too_many_sets_are_refused() {
        let mut text = String::from("venn-beta\n");
        for index in 0..=MOST_SETS {
            text.push_str(&format!(" set S{index}\n"));
        }
        assert!(check::refused(&text, &options()).reason.contains("at most"));
    }

    #[test]
    fn a_size_scales_the_area_so_the_radius_goes_with_its_square_root() {
        let radii = radii(&[Some(40.0), Some(25.0), Some(1.0)], 1.0);
        assert!((radii[0] - BASE_RADIUS).abs() < 0.01);
        assert!((radii[1] - BASE_RADIUS * (25.0_f32 / 40.0).sqrt()).abs() < 0.01);
        assert!((radii[2] - BASE_RADIUS * SMALLEST_SHARE).abs() < 0.01, "a tiny set is not tiny");
    }

    #[test]
    fn two_circles_overlap_by_a_third_of_a_diameter() {
        let radii = [90.0, 90.0];
        let centres = centres(&radii);
        let overlap = radii[0] + radii[1] - centres[0].distance(centres[1]);
        assert!((overlap - 60.0).abs() < 0.01, "a third of 180 is 60, found {overlap}");
    }

    #[test]
    fn three_circles_meet_in_the_middle_and_four_overlap_their_neighbours() {
        let three = centres(&[90.0, 90.0, 90.0]);
        let middle = Point::new(
            (three[0].x + three[1].x + three[2].x) / 3.0,
            (three[0].y + three[1].y + three[2].y) / 3.0,
        );
        for centre in &three {
            assert!(centre.distance(middle) < 90.0, "the middle is inside every circle");
        }
        let four = centres(&[90.0; 4]);
        for index in 0..4 {
            let next = four[(index + 1) % 4];
            assert!(four[index].distance(next) < 180.0, "neighbours overlap");
        }
    }

    #[test]
    fn the_documentation_examples_draw() {
        let examples = [
            "venn-beta\n  title What makes a good feature\n  set Desirable\n  set Feasible\n  set Viable\n  union Desirable,Feasible[\"Buildable\"]\n  union Feasible,Viable[\"Sustainable\"]\n  union Desirable,Viable[\"Marketable\"]\n  union Desirable,Feasible,Viable[\"Ship it\"]\n",
            "venn-beta\n  title \"Team overlap\"\n  set Frontend\n  set Backend\n  union Frontend,Backend[\"APIs\"]\n",
            "venn-beta\n  set A[\"Alpha\"]:20\n  set B[\"Beta\"]:12\n  union A,B[\"AB\"]:3\n",
            "venn-beta\n  set A[\"Frontend\"]\n    text A1[\"React\"]\n    text A2[\"Design Systems\"]\n  set B[\"Backend\"]\n    text B1[\"API\"]\n  union A,B[\"Shared\"]\n    text AB1[\"OpenAPI\"]\n",
            "venn-beta\n  set A[\"Alpha\"]:20\n    text A1[\"React\"]\n    text A2[\"Design Systems\"]\n  set B[\"Beta\"]:12\n  union A,B[\"AB\"]:3\n  style A fill:#ff6b6b\n  style A,B color:#333\n  style A1 color:red\n",
        ];
        for text in examples {
            check::drawn(text, &options(), &[]);
        }
        check::drawn(
            examples[0],
            &options(),
            &["Buildable", "Sustainable", "Marketable", "Ship it"],
        );
        check::drawn(examples[3], &options(), &["React", "Design Systems", "OpenAPI", "Shared"]);
    }

    #[test]
    fn one_two_three_and_four_sets_keep_their_words_in_their_regions() {
        assert_words_are_in_their_regions("venn-beta\n set Only\n   text T[\"a thing\"]\n");
        assert_words_are_in_their_regions(
            "venn-beta\n set A[\"Frontend\"]\n   text A1[\"React\"]\n   text A2[\"Design Systems\"]\n set B[\"Backend\"]\n   text B1[\"API\"]\n union A,B[\"Shared\"]\n   text AB1[\"OpenAPI\"]\n",
        );
        assert_words_are_in_their_regions(
            "venn-beta\n set Desirable\n set Feasible\n set Viable\n union Desirable,Feasible[\"Buildable\"]\n union Feasible,Viable[\"Sustainable\"]\n union Desirable,Viable[\"Marketable\"]\n union Desirable,Feasible,Viable[\"Ship it\"]\n",
        );
        assert_words_are_in_their_regions(
            "venn-beta\n set A[\"Alpha\"]:20\n set B[\"Beta\"]:12\n set C[\"Gamma\"]:8\n set D[\"Delta\"]:15\n union A,B[\"AB\"]\n union B,C[\"BC\"]\n union C,D[\"CD\"]\n union D,A[\"DA\"]\n",
        );
    }

    #[test]
    fn long_names_and_many_text_lines_make_the_picture_larger_instead_of_overflowing() {
        let text = "venn-beta\n set A[\"Platform engineering\"]\n   text A1[\"Developer portals and golden paths\"]\n   text A2[\"Paved roads\"]\n set B[\"Security\"]\n   text B1[\"Threat modelling for every service\"]\n union A,B[\"Secure by default platforms\"]\n   text AB1[\"Policy as code\"]\n   text AB2[\"Signed builds\"]\n";
        assert_words_are_in_their_regions(text);
        let (_, _, laid) = analyse(text);
        assert!(laid.circles[0].1 > BASE_RADIUS, "the circles grew to hold the words");
    }

    #[test]
    fn a_set_name_is_not_inside_the_circle_next_to_it() {
        let (diagram, _, laid) = analyse("venn-beta\n set A\n set B\n union A,B[\"AB\"]\n");
        let name = laid.placed[0].expect("A has a name");
        assert!(name.exclusive, "A's name is where B does not cover");
        let margin =
            region_margin(name.rect.centre(), &diagram.areas[0].members, &laid.circles, true);
        assert!(margin > 0.0);
        let (b, rb) = laid.circles[1];
        for corner in [name.rect.left(), name.rect.right()] {
            assert!(Point::new(corner, name.rect.centre().y).distance(b) > rb);
        }
    }

    #[test]
    fn a_union_of_circles_that_do_not_meet_still_draws() {
        // A and C are opposite on a ring of four, so they barely meet. The words go somewhere.
        let text = "venn-beta\n set A\n set B\n set C\n set D\n union A,C[\"Far apart\"]\n";
        let scene = check::drawn(text, &options(), &["Far apart", "A", "D"]);
        assert!(scene.size.width < 5000.0, "growing stopped");
    }

    #[test]
    fn every_prefix_of_a_diagram_draws_or_is_refused_without_panicking() {
        let text = "venn-beta\n title \"Team\"\n set A[\"Frontend ❤\"]:20\n   text A1[\"React\"]\n set B\n union A,B[\"Shared\"]:3\n   text X[\"Y\"]\n";
        for (end, _) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
            let _ = super::super::render(&text[..end], &options());
        }
    }
}
