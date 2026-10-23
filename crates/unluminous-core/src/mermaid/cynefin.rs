//! Cynefin diagrams: `cynefin` and `cynefin-beta`.
//!
//! The Cynefin framework sorts situations into five domains. Four of them are quadrants, always in
//! the same places: Complex at the top left, Complicated at the top right, Chaotic at the bottom left
//! and Clear at the bottom right. Confusion is an ellipse in the middle.
//!
//! **The boundaries are waves, and the waves are a fixed function.** Each of the four boundaries is a
//! sine curve with its own phase and no randomness in it, so the same source is the same picture on
//! every run. The wave is zero at both ends, so the four boundaries meet at the centre and reach the
//! outer edge square. Mermaid's own `seed` option is not needed for the same reason. The Clear to
//! Chaotic boundary has the cliff: its lower half is drawn thick.
//!
//! **The quadrants are sized from what is in them.** Each quadrant holds a block at its outer corner:
//! the domain's name, its decision model and its kind of practice under it, and then one rounded
//! badge for each item, with the item's words wrapped inside the badge. A quadrant is made large
//! enough that
//!
//! - the block clears the boundaries, including the room a transition's label needs beside them;
//! - the block clears the ellipse, either by the quadrant being taller or by it being wider, whichever
//!   makes the smaller picture.
//!
//! So a domain's name and its decision model are never under a badge or under the ellipse.
//!
//! **A transition is a curved arrow with its label on a panel in the middle.** When the two domains
//! share a boundary the arrow crosses that boundary in a lane beside the ellipse, and the quadrants
//! leave room for the lane. A transition between opposite domains curves round the ellipse, and one
//! to or from Confusion starts on the ellipse's edge. Every label panel is put in the nearest spot
//! that covers no badge, no name and no other panel.
//!
//! The decision models are written with commas instead of arrows, because a font without an arrow
//! character draws a box.

use super::parts;
use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke};
use super::source::{self, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// Space between the outer edge of the diagram and the words in a quadrant.
const OUTER: f32 = 14.0;
/// How far a boundary wanders from its straight line, either side. Mermaid's own default.
const WAVE: f32 = 8.0;
/// Space kept between things that must not touch.
const PAD: f32 = 8.0;
/// How far a lane's arrow reaches past the ends of its label panel.
const TAIL: f32 = 28.0;
/// The gap between lanes on one boundary.
const LANE_GAP: f32 = 10.0;
/// How wide a badge's words may be before they wrap.
const BADGE_WRAP: f32 = 150.0;
/// How wide a transition's label may be before it wraps.
const PANEL_WRAP: f32 = 140.0;
/// The space between badges, and between the subtitles and the first badge.
const BADGE_GAP: f32 = 6.0;
const BADGE_PAD_X: f32 = 10.0;
const BADGE_PAD_Y: f32 = 5.0;
const PANEL_PAD_X: f32 = 8.0;
const PANEL_PAD_Y: f32 = 5.0;
/// How many items the Confusion ellipse lists before it says how many more there are.
const CONFUSION_ITEMS: usize = 3;
/// The smallest a quadrant is, which is what an empty framework is drawn at.
const SMALLEST: Size = Size::new(300.0, 220.0);
/// How many segments a boundary is drawn with.
const WAVE_STEPS: usize = 32;
/// How many points the ellipse is drawn with.
const ELLIPSE_POINTS: usize = 64;
/// How far a transition's arrow bows away from the straight line.
const BOW: f32 = 16.0;

/// One of the five domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Domain {
    Complex,
    Complicated,
    Clear,
    Chaotic,
    Confusion,
}

impl Domain {
    const ALL: [Domain; 5] =
        [Domain::Complex, Domain::Complicated, Domain::Clear, Domain::Chaotic, Domain::Confusion];

    fn keyword(self) -> &'static str {
        match self {
            Domain::Complex => "complex",
            Domain::Complicated => "complicated",
            Domain::Clear => "clear",
            Domain::Chaotic => "chaotic",
            Domain::Confusion => "confusion",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Domain::Complex => "Complex",
            Domain::Complicated => "Complicated",
            Domain::Clear => "Clear",
            Domain::Chaotic => "Chaotic",
            Domain::Confusion => "Confusion",
        }
    }

    /// The decision model and the kind of practice, which the quadrants show under their name.
    fn subtitles(self) -> [&'static str; 2] {
        match self {
            Domain::Complex => ["Probe, Sense, Respond", "Emergent practice"],
            Domain::Complicated => ["Sense, Analyse, Respond", "Good practice"],
            Domain::Clear => ["Sense, Categorise, Respond", "Best practice"],
            Domain::Chaotic => ["Act, Sense, Respond", "Novel practice"],
            Domain::Confusion => ["", ""],
        }
    }

    /// Which quadrant: the column and the row, each zero or one. Confusion has none.
    fn cell(self) -> Option<(usize, usize)> {
        match self {
            Domain::Complex => Some((0, 0)),
            Domain::Complicated => Some((1, 0)),
            Domain::Chaotic => Some((0, 1)),
            Domain::Clear => Some((1, 1)),
            Domain::Confusion => None,
        }
    }

    fn index(self) -> usize {
        Domain::ALL.iter().position(|&domain| domain == self).unwrap_or(0)
    }

    /// Which of the theme's series colours washes this domain.
    fn colour(self) -> usize {
        match self {
            Domain::Complex => 0,
            Domain::Complicated => 1,
            Domain::Clear => 2,
            Domain::Chaotic => 5,
            Domain::Confusion => 6,
        }
    }
}

/// The domain a word names, ignoring case.
fn domain_named(word: &str) -> Option<Domain> {
    let word = word.trim();
    Domain::ALL.into_iter().find(|domain| domain.keyword().eq_ignore_ascii_case(word))
}

#[derive(Debug, Clone, PartialEq)]
struct Move {
    from: Domain,
    to: Domain,
    label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Diagram {
    title: Option<String>,
    /// The items of each domain, indexed by [`Domain::index`].
    items: [Vec<String>; 5],
    moves: Vec<Move>,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    Ok(draw(&diagram, source, options))
}

fn read(source: &Source) -> Result<Diagram, Problem> {
    let mut diagram = Diagram::default();
    let mut current: Option<Domain> = None;
    for line in source.statements() {
        if let Some(rest) = line.after_word("title") {
            diagram.title = Some(source::label(rest));
        } else if let Some((from, rest)) =
            line.text.split_once("-->").filter(|_| !line.text.starts_with('"'))
        {
            let (to, label) = rest.split_once(':').map_or((rest, None), |(to, label)| {
                (to, Some(source::label(label)).filter(|words| !words.is_empty()))
            });
            let named = |word: &str| {
                domain_named(&source::unquote(word)).ok_or_else(|| {
                    Problem::at(
                        line,
                        format!(
                            "`{}` is not a domain. The domains are complex, complicated, clear, chaotic and confusion.",
                            word.trim()
                        ),
                    )
                })
            };
            let (from, to) = (named(from)?, named(to)?);
            // A transition from a domain to itself says nothing, and Mermaid ignores it as well.
            if from != to {
                diagram.moves.push(Move { from, to, label });
            }
        } else if let Some(domain) = domain_named(&line.text) {
            current = Some(domain);
        } else if line.text.starts_with('"') {
            let Some(domain) = current else {
                return Err(Problem::at(
                    line,
                    "an item goes under the domain it belongs to: write `complex`, `complicated`, `clear`, `chaotic` or `confusion` first.",
                ));
            };
            let words = source::label(&line.text);
            if !words.is_empty() {
                diagram.items[domain.index()].push(words);
            }
        } else if ["style", "classDef", "class", "linkStyle"]
            .iter()
            .any(|word| line.starts_with_word(word))
        {
            continue;
        } else {
            return Err(Problem::at(
                line,
                "a line is a domain name, a quoted item such as `\"Run an experiment\"`, a transition such as `complex --> clear : \"Label\"`, or a `title`.",
            ));
        }
    }
    Ok(diagram)
}

// ---------------------------------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------------------------------

/// What a piece of drawn text is, so the drawing can choose its style.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Name,
    Subtitle,
    Badge,
}

/// One thing in a domain: a line of words, or a badge with words wrapped inside it.
#[derive(Debug, Clone)]
struct Piece {
    domain: Domain,
    kind: Kind,
    label: Label,
    /// The area the piece covers. For words this is the words, for a badge the rounded box.
    rect: Rect,
    /// For words, where they are placed and how they are anchored.
    at: Point,
    anchor: Anchor,
}

/// The measured contents of one domain, before it is placed.
struct Block {
    domain: Domain,
    name: Label,
    subtitles: Vec<Label>,
    /// Each badge's words and the size of its box.
    badges: Vec<(Label, Size)>,
    width: f32,
    height: f32,
}

fn measure_badge(words: &str, options: &Options) -> (Label, Size) {
    let label = text::measure(words, &options.style(0.9, false), options.metrics, BADGE_WRAP);
    let size = Size::new(label.width + BADGE_PAD_X * 2.0, label.height + BADGE_PAD_Y * 2.0);
    (label, size)
}

/// Measure what one quadrant holds.
fn measure_block(domain: Domain, items: &[String], options: &Options) -> Block {
    let name = text::measure_unwrapped(domain.title(), &options.style(1.15, true), options.metrics);
    let small = options.style(0.8, false);
    let subtitles: Vec<Label> = domain
        .subtitles()
        .iter()
        .filter(|words| !words.is_empty())
        .map(|words| text::measure_unwrapped(words, &small, options.metrics))
        .collect();
    let badges: Vec<(Label, Size)> =
        items.iter().map(|words| measure_badge(words, options)).collect();
    let mut width = name.width;
    let mut height = name.height;
    for subtitle in &subtitles {
        width = width.max(subtitle.width);
        height += subtitle.height;
    }
    if !badges.is_empty() {
        height += BADGE_GAP + 2.0;
    }
    for (_, size) in &badges {
        width = width.max(size.width);
        height += size.height + BADGE_GAP;
    }
    if !badges.is_empty() {
        height -= BADGE_GAP;
    }
    Block { domain, name, subtitles, badges, width, height }
}

/// Place a block with its top left (or top right, when `right` is set) at `corner`, as pieces.
fn place_block(block: &Block, corner: Point, right: bool, pieces: &mut Vec<Piece>) {
    let (edge, anchor) =
        if right { (corner.x + block.width, Anchor::End) } else { (corner.x, Anchor::Start) };
    let words_left = |width: f32| if right { edge - width } else { edge };
    let mut y = corner.y;
    let words = |label: &Label, kind: Kind, y: f32, pieces: &mut Vec<Piece>| {
        pieces.push(Piece {
            domain: block.domain,
            kind,
            label: label.clone(),
            rect: Rect::new(words_left(label.width), y, label.width, label.height),
            at: Point::new(edge, y),
            anchor,
        });
    };
    words(&block.name, Kind::Name, y, pieces);
    y += block.name.height;
    for subtitle in &block.subtitles {
        words(subtitle, Kind::Subtitle, y, pieces);
        y += subtitle.height;
    }
    if !block.badges.is_empty() {
        y += BADGE_GAP + 2.0;
    }
    for (label, size) in &block.badges {
        let left = if right { edge - size.width } else { edge };
        pieces.push(Piece {
            domain: block.domain,
            kind: Kind::Badge,
            label: label.clone(),
            rect: Rect::new(left, y, size.width, size.height),
            at: Point::new(left, y),
            anchor: Anchor::Start,
        });
        y += size.height + BADGE_GAP;
    }
}

/// The Confusion ellipse and what is written in it.
struct Ellipse {
    centre: Point,
    rx: f32,
    ry: f32,
}

impl Ellipse {
    fn bounds(&self) -> Rect {
        Rect::new(self.centre.x - self.rx, self.centre.y - self.ry, self.rx * 2.0, self.ry * 2.0)
    }
}

/// The measured contents of the Confusion ellipse: its name, up to three items and a count of the rest.
fn measure_confusion(items: &[String], options: &Options) -> Block {
    let shown: Vec<String> = items.iter().take(CONFUSION_ITEMS).cloned().collect();
    let mut block = measure_block(Domain::Confusion, &shown, options);
    if items.len() > CONFUSION_ITEMS {
        let more = format!("+{} more", items.len() - CONFUSION_ITEMS);
        let (label, size) = measure_badge(&more, options);
        block.width = block.width.max(size.width);
        block.height += BADGE_GAP + size.height;
        block.badges.push((label, size));
    }
    block
}

/// Which boundary a transition crosses, if it crosses one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    /// The vertical boundary between Complex and Complicated.
    Top,
    /// The vertical boundary between Chaotic and Clear.
    Bottom,
    /// The horizontal boundary between Complex and Chaotic.
    Left,
    /// The horizontal boundary between Complicated and Clear.
    Right,
}

fn lane_of(from: Domain, to: Domain) -> Option<Lane> {
    let ((from_column, from_row), (to_column, to_row)) = (from.cell()?, to.cell()?);
    match (from_column == to_column, from_row == to_row) {
        (false, true) => Some(if from_row == 0 { Lane::Top } else { Lane::Bottom }),
        (true, false) => Some(if from_column == 0 { Lane::Left } else { Lane::Right }),
        _ => None,
    }
}

impl Lane {
    /// True for a boundary that runs up and down, which a transition crosses sideways.
    fn is_vertical(self) -> bool {
        matches!(self, Lane::Top | Lane::Bottom)
    }
}

/// A transition, measured.
struct Measured {
    mv: Move,
    label: Option<Label>,
    /// The size of the panel the label is drawn on, or of the room it would take.
    panel: Size,
    lane: Option<Lane>,
}

fn measure_moves(diagram: &Diagram, options: &Options) -> Vec<Measured> {
    let style = options.style(0.85, false);
    diagram
        .moves
        .iter()
        .map(|mv| {
            let label = mv
                .label
                .as_deref()
                .map(|words| text::measure(words, &style, options.metrics, PANEL_WRAP));
            let panel = label.as_ref().map_or(Size::new(36.0, 12.0), |label| {
                Size::new(label.width + PANEL_PAD_X * 2.0, label.height + PANEL_PAD_Y * 2.0)
            });
            Measured { mv: mv.clone(), label, panel, lane: lane_of(mv.from, mv.to) }
        })
        .collect()
}

/// An arrow, and the panel its label is on.
struct Arrow {
    curve: Vec<Point>,
    panel: Option<(Rect, Label)>,
}

/// The whole picture, worked out.
struct Plan {
    /// The size of one quadrant.
    quadrant: Size,
    pieces: Vec<Piece>,
    ellipse: Ellipse,
    arrows: Vec<Arrow>,
}

impl Plan {
    fn size(&self) -> Size {
        Size::new(self.quadrant.width * 2.0, self.quadrant.height * 2.0)
    }

    fn centre(&self) -> Point {
        Point::new(self.quadrant.width, self.quadrant.height)
    }
}

/// How big the ellipse has to be to hold a block, with room to spare.
fn ellipse_for(block: &Block) -> (f32, f32) {
    let width = block.width + 12.0;
    let height = block.height + 10.0;
    ((width / 2.0 * 1.52).max(60.0), (height / 2.0 * 1.52).max(34.0))
}

/// The largest panel in some transitions, counting only those with a lane.
fn largest_panel(moves: &[Measured], vertical: bool) -> Option<Size> {
    moves
        .iter()
        .filter(|measured| measured.lane.is_some_and(|lane| lane.is_vertical() == vertical))
        .map(|measured| measured.panel)
        .reduce(|a, b| Size::new(a.width.max(b.width), a.height.max(b.height)))
}

/// How many transitions use one boundary.
fn lane_count(moves: &[Measured], lane: Lane) -> usize {
    moves.iter().filter(|measured| measured.lane == Some(lane)).count()
}

fn plan(diagram: &Diagram, options: &Options) -> Plan {
    let quadrants: Vec<Block> =
        [Domain::Complex, Domain::Complicated, Domain::Chaotic, Domain::Clear]
            .into_iter()
            .map(|domain| measure_block(domain, &diagram.items[domain.index()], options))
            .collect();
    let confusion = measure_confusion(&diagram.items[Domain::Confusion.index()], options);
    let moves = measure_moves(diagram, options);

    let widest = quadrants.iter().map(|block| block.width).fold(0.0_f32, f32::max);
    let tallest = quadrants.iter().map(|block| block.height).fold(0.0_f32, f32::max);

    // The panels beside the boundaries decide how much room each boundary needs.
    let across = largest_panel(&moves, true);
    let down = largest_panel(&moves, false);
    let (mut rx, mut ry) = ellipse_for(&confusion);
    if let Some(panel) = across {
        rx = rx.max(panel.width / 2.0 + TAIL);
    }
    if let Some(panel) = down {
        ry = ry.max(panel.height / 2.0 + TAIL);
    }
    let side_gap = across.map_or(WAVE + PAD, |panel| panel.width / 2.0 + TAIL + PAD);
    let level_gap = down.map_or(WAVE + PAD, |panel| panel.height / 2.0 + TAIL + PAD);

    let mut width = OUTER + widest + side_gap;
    let mut height = OUTER + tallest + level_gap;
    // The lanes stacked along the centre lines, outside the ellipse.
    let top_or_bottom = lane_count(&moves, Lane::Top).max(lane_count(&moves, Lane::Bottom));
    let left_or_right = lane_count(&moves, Lane::Left).max(lane_count(&moves, Lane::Right));
    if let Some(panel) = across {
        height = height.max(OUTER + ry + PAD + top_or_bottom as f32 * (panel.height + LANE_GAP));
    }
    if let Some(panel) = down {
        width = width.max(OUTER + rx + PAD + left_or_right as f32 * (panel.width + LANE_GAP));
    }
    width = width.max(SMALLEST.width).max(rx + OUTER + 10.0);
    height = height.max(SMALLEST.height).max(ry + OUTER + 10.0);

    // Clearing the ellipse: taller, or wider, whichever gives the smaller picture.
    let gap = PAD + WAVE + 4.0;
    let taller = height.max(OUTER + tallest + ry + gap);
    let wider = width.max(OUTER + widest + rx + gap);
    let (width, height) =
        if width * taller <= wider * height { (width, taller) } else { (wider, height) };
    let quadrant = Size::new(width, height);

    let centre = Point::new(width, height);
    let mut pieces = Vec::new();
    for block in &quadrants {
        let (column, row) = block.domain.cell().unwrap_or((0, 0));
        let left = if column == 0 { OUTER } else { width * 2.0 - OUTER - block.width };
        let top = if row == 0 { OUTER } else { height * 2.0 - OUTER - block.height };
        place_block(block, Point::new(left, top), column == 1, &mut pieces);
    }
    let ellipse = Ellipse { centre, rx, ry };
    place_confusion(&confusion, &ellipse, &mut pieces);

    let mut plan = Plan { quadrant, pieces, ellipse, arrows: Vec::new() };
    plan.arrows = route_arrows(&plan, &moves, across, down);
    plan
}

/// Centre the Confusion block in its ellipse.
fn place_confusion(block: &Block, ellipse: &Ellipse, pieces: &mut Vec<Piece>) {
    let mut y = ellipse.centre.y - block.height / 2.0;
    let middle = ellipse.centre.x;
    pieces.push(Piece {
        domain: Domain::Confusion,
        kind: Kind::Name,
        label: block.name.clone(),
        rect: Rect::new(middle - block.name.width / 2.0, y, block.name.width, block.name.height),
        at: Point::new(middle, y),
        anchor: Anchor::Middle,
    });
    y += block.name.height;
    if !block.badges.is_empty() {
        y += BADGE_GAP + 2.0;
    }
    for (label, size) in &block.badges {
        pieces.push(Piece {
            domain: Domain::Confusion,
            kind: Kind::Badge,
            label: label.clone(),
            rect: Rect::new(middle - size.width / 2.0, y, size.width, size.height),
            at: Point::new(middle - size.width / 2.0, y),
            anchor: Anchor::Start,
        });
        y += size.height + BADGE_GAP;
    }
}

// ---------------------------------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------------------------------

/// A quadratic curve from `start` to `end` pulled towards `control`.
fn curve(start: Point, control: Point, end: Point) -> Vec<Point> {
    const STEPS: usize = 20;
    (0..=STEPS)
        .map(|step| {
            let t = step as f32 / STEPS as f32;
            start.towards(control, t).towards(control.towards(end, t), t)
        })
        .collect()
}

/// The unit vector at a right angle to the way from `from` to `to`, turned clockwise on the screen.
fn right_angle(from: Point, to: Point) -> Point {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let length = (dx * dx + dy * dy).sqrt();
    if length < f32::EPSILON {
        return Point::new(0.0, -1.0);
    }
    Point::new(-dy / length, dx / length)
}

/// The sign of a quadrant's side of the centre, along one axis: -1 for the left or top.
fn side(index: usize) -> f32 {
    if index == 0 {
        -1.0
    } else {
        1.0
    }
}

/// Work out every transition's arrow, then put each label panel in the nearest free spot.
fn route_arrows(
    plan: &Plan,
    moves: &[Measured],
    across: Option<Size>,
    down: Option<Size>,
) -> Vec<Arrow> {
    let mut used = [0_usize; 4];
    let mut curves = Vec::new();
    for measured in moves {
        let path = match measured.lane {
            Some(lane) => {
                let slot = match lane {
                    Lane::Top => 0,
                    Lane::Bottom => 1,
                    Lane::Left => 2,
                    Lane::Right => 3,
                };
                let order = used[slot];
                used[slot] += 1;
                lane_curve(plan, &measured.mv, lane, order, across, down)
            }
            None => free_curve(plan, &measured.mv),
        };
        curves.push(path);
    }

    let mut obstacles: Vec<Rect> = plan.pieces.iter().map(|piece| piece.rect.grown(2.0)).collect();
    obstacles.push(plan.ellipse.bounds());
    let frame = Rect::new(2.0, 2.0, plan.size().width - 4.0, plan.size().height - 4.0);
    let mut arrows = Vec::new();
    for (measured, path) in moves.iter().zip(curves) {
        let panel = measured.label.as_ref().map(|label| {
            let middle = path[path.len() / 2];
            let rect = free_spot(middle, measured.panel, &obstacles, frame);
            obstacles.push(rect.grown(2.0));
            (rect, label.clone())
        });
        arrows.push(Arrow { curve: path, panel });
    }
    arrows
}

/// The arrow for a transition across a boundary, in its lane beside the ellipse.
fn lane_curve(
    plan: &Plan,
    mv: &Move,
    lane: Lane,
    order: usize,
    across: Option<Size>,
    down: Option<Size>,
) -> Vec<Point> {
    let centre = plan.centre();
    let Ellipse { rx, ry, .. } = plan.ellipse;
    let (from_cell, to_cell) = (mv.from.cell().unwrap_or((0, 0)), mv.to.cell().unwrap_or((0, 0)));
    if lane.is_vertical() {
        let panel = across.unwrap_or_default();
        let outward = if lane == Lane::Top { -1.0 } else { 1.0 };
        let y = centre.y
            + outward * (ry + PAD + panel.height / 2.0 + order as f32 * (panel.height + LANE_GAP));
        let direction = side(to_cell.0) - side(from_cell.0);
        let reach = panel.width / 2.0 + TAIL;
        let start = Point::new(centre.x - direction.signum() * reach, y);
        let end = Point::new(centre.x + direction.signum() * reach, y);
        curve(start, Point::new(centre.x, y + outward * BOW), end)
    } else {
        let panel = down.unwrap_or_default();
        let outward = if lane == Lane::Left { -1.0 } else { 1.0 };
        let x = centre.x
            + outward * (rx + PAD + panel.width / 2.0 + order as f32 * (panel.width + LANE_GAP));
        let direction = side(to_cell.1) - side(from_cell.1);
        let reach = panel.height / 2.0 + TAIL;
        let start = Point::new(x, centre.y - direction.signum() * reach);
        let end = Point::new(x, centre.y + direction.signum() * reach);
        curve(start, Point::new(x + outward * BOW, centre.y), end)
    }
}

/// The arrow between opposite domains, or to or from Confusion.
fn free_curve(plan: &Plan, mv: &Move) -> Vec<Point> {
    let centre = plan.centre();
    let Ellipse { rx, ry, .. } = plan.ellipse;
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    let corner = |domain: Domain, reach: f32| {
        let (column, row) = domain.cell().unwrap_or((0, 0));
        Point::new(
            centre.x + side(column) * (diagonal * rx + reach),
            centre.y + side(row) * (diagonal * ry + reach * 0.8),
        )
    };
    if mv.from == Domain::Confusion || mv.to == Domain::Confusion {
        let outer = if mv.from == Domain::Confusion { mv.to } else { mv.from };
        let edge = corner(outer, 0.0);
        let far = corner(outer, 48.0);
        let (start, end) = if mv.from == Domain::Confusion { (edge, far) } else { (far, edge) };
        let bow = right_angle(start, end);
        let middle = start.towards(end, 0.5);
        return curve(start, Point::new(middle.x + bow.x * BOW, middle.y + bow.y * BOW), end);
    }
    let start = corner(mv.from, 10.0);
    let end = corner(mv.to, 10.0);
    let bow = right_angle(start, end);
    // The curve must bulge past the ellipse, whichever way it is turned.
    let reach = ((bow.x * rx).powi(2) + (bow.y * ry).powi(2)).sqrt() + 24.0;
    let control = Point::new(centre.x + bow.x * reach * 2.0, centre.y + bow.y * reach * 2.0);
    curve(start, control, end)
}

/// The spot nearest `wanted` where a panel of `size` covers nothing in `obstacles` and stays in `frame`.
/// When there is none, the panel goes at `wanted`.
fn free_spot(wanted: Point, size: Size, obstacles: &[Rect], frame: Rect) -> Rect {
    let preferred = Rect::around(wanted, size);
    let free = |rect: &Rect| {
        rect.left() >= frame.left()
            && rect.top() >= frame.top()
            && rect.right() <= frame.right()
            && rect.bottom() <= frame.bottom()
            && !obstacles.iter().any(|obstacle| obstacle.overlaps(rect))
    };
    if free(&preferred) {
        return preferred;
    }
    const STEP: f32 = 6.0;
    let mut best: Option<(f32, Rect)> = None;
    let mut y = frame.top();
    while y + size.height <= frame.bottom() {
        let mut x = frame.left();
        while x + size.width <= frame.right() {
            let rect = Rect::new(x, y, size.width, size.height);
            let distance = rect.centre().distance(wanted);
            if best.is_none_or(|(known, _)| distance < known) && free(&rect) {
                best = Some((distance, rect));
            }
            x += STEP;
        }
        y += STEP;
    }
    best.map_or(preferred, |(_, rect)| rect)
}

// ---------------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------------

/// A boundary: a straight line with a sine wave added at right angles, zero at both ends.
///
/// The wave goes round one and a half times. Its phase is the only thing that tells one boundary from
/// another, and each boundary has its own fixed phase.
fn wave(from: Point, to: Point, phase: f32) -> Vec<Point> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let length = (dx * dx + dy * dy).sqrt();
    if length < f32::EPSILON {
        return vec![from, to];
    }
    let (nx, ny) = (-dy / length, dx / length);
    (0..=WAVE_STEPS)
        .map(|step| {
            let t = step as f32 / WAVE_STEPS as f32;
            let base = from.towards(to, t);
            let offset = WAVE
                * (3.0 * std::f32::consts::PI * t + phase).sin()
                * (std::f32::consts::PI * t).sin();
            Point::new(base.x + nx * offset, base.y + ny * offset)
        })
        .collect()
}

/// Join paths end to end, leaving out a point that repeats the one before it.
fn joined(paths: &[Vec<Point>]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for path in paths {
        for point in path {
            if out.last() != Some(point) {
                out.push(*point);
            }
        }
    }
    out
}

fn reversed(path: &[Point]) -> Vec<Point> {
    path.iter().rev().copied().collect()
}

/// The four boundaries, each running away from the centre: up, down, left and right.
struct Boundaries {
    top: Vec<Point>,
    bottom: Vec<Point>,
    left: Vec<Point>,
    right: Vec<Point>,
}

fn boundaries(quadrant: Size) -> Boundaries {
    let (width, height) = (quadrant.width, quadrant.height);
    let centre = Point::new(width, height);
    Boundaries {
        top: wave(centre, Point::new(width, 0.0), 0.4),
        bottom: wave(centre, Point::new(width, height * 2.0), 2.1),
        left: wave(centre, Point::new(0.0, height), 4.0),
        right: wave(centre, Point::new(width * 2.0, height), 1.2),
    }
}

/// The outline of one quadrant, following its two boundaries.
fn quadrant_outline(domain: Domain, quadrant: Size, edges: &Boundaries) -> Vec<Point> {
    let (width, height) = (quadrant.width, quadrant.height);
    let corner = |x: f32, y: f32| vec![Point::new(x, y)];
    match domain {
        Domain::Complex => joined(&[
            corner(0.0, 0.0),
            corner(width, 0.0),
            reversed(&edges.top),
            edges.left.clone(),
        ]),
        Domain::Complicated => joined(&[
            corner(width, 0.0),
            corner(width * 2.0, 0.0),
            corner(width * 2.0, height),
            reversed(&edges.right),
            edges.top.clone(),
        ]),
        Domain::Chaotic => joined(&[
            corner(0.0, height),
            reversed(&edges.left),
            edges.bottom.clone(),
            corner(0.0, height * 2.0),
        ]),
        _ => joined(&[
            edges.right.clone(),
            corner(width * 2.0, height * 2.0),
            corner(width, height * 2.0),
            reversed(&edges.bottom),
        ]),
    }
}

fn ellipse_outline(ellipse: &Ellipse) -> Vec<Point> {
    (0..ELLIPSE_POINTS)
        .map(|step| {
            let angle = std::f32::consts::TAU * step as f32 / ELLIPSE_POINTS as f32;
            Point::new(
                ellipse.centre.x + ellipse.rx * angle.cos(),
                ellipse.centre.y + ellipse.ry * angle.sin(),
            )
        })
        .collect()
}

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let plan = plan(diagram, options);
    let size = plan.size();
    let mut scene = Scene::new();
    let mut titled = source.clone();
    if titled.title.is_none() {
        titled.title.clone_from(&diagram.title);
    }
    let head = parts::title(&mut scene, &titled, options, size.width + parts::MARGIN * 2.0);
    let mut inner = Scene::new();
    draw_regions(&mut inner, &plan, options);
    draw_arrows(&mut inner, &plan, options);
    draw_pieces(&mut inner, &plan, options);
    draw_panels(&mut inner, &plan, options);
    scene.add_scene(inner, parts::MARGIN, head + parts::MARGIN);
    scene.claim(Rect::new(
        0.0,
        0.0,
        size.width + parts::MARGIN * 2.0,
        size.height + head + parts::MARGIN * 2.0,
    ));
    parts::finish(&mut scene);
    scene
}

/// The quadrants, their boundaries, the cliff and the ellipse.
fn draw_regions(scene: &mut Scene, plan: &Plan, options: &Options) {
    let size = plan.size();
    let edges = boundaries(plan.quadrant);
    for domain in [Domain::Complex, Domain::Complicated, Domain::Chaotic, Domain::Clear] {
        scene.add(Item::Polygon {
            points: quadrant_outline(domain, plan.quadrant, &edges),
            fill: Some(options.theme.wash(domain.colour(), 38)),
            stroke: None,
        });
    }
    scene.add(Item::Rect {
        rect: Rect::new(0.0, 0.0, size.width, size.height),
        radius: 0.0,
        fill: None,
        stroke: Some(Stroke::new(options.theme.node_stroke, parts::LINE)),
    });
    for edge in [&edges.top, &edges.bottom, &edges.left, &edges.right] {
        scene.add(Item::Line {
            points: edge.clone(),
            stroke: Stroke::new(options.theme.node_stroke, parts::LINE),
            dash: Dash::Solid,
        });
    }
    // The cliff is the lower half of the boundary between Chaotic and Clear.
    let cliff = edges.bottom.get(edges.bottom.len() / 2..).unwrap_or(&[]).to_vec();
    scene.add(Item::Line {
        points: cliff,
        stroke: Stroke::new(options.theme.series(5), parts::THICK * 1.5),
        dash: Dash::Solid,
    });
    scene.add(Item::Polygon {
        points: ellipse_outline(&plan.ellipse),
        fill: Some(Paint::faded(options.theme.series(Domain::Confusion.colour()), 80)),
        stroke: Some(Stroke::new(options.theme.series(Domain::Confusion.colour()), parts::LINE)),
    });
}

fn draw_arrows(scene: &mut Scene, plan: &Plan, options: &Options) {
    let colour = options.theme.line;
    for arrow in &plan.arrows {
        let heading = parts::heading(&arrow.curve);
        let Some(tip) = arrow.curve.last().copied() else { continue };
        scene.add(Item::Line {
            points: parts::trimmed(&arrow.curve, 0.0, parts::HEAD * 0.9),
            stroke: Stroke::new(colour, parts::LINE + 0.5),
            dash: Dash::Solid,
        });
        scene.add(Item::Polygon {
            points: parts::arrow_head(tip, heading, parts::HEAD),
            fill: Some(Paint::solid(colour)),
            stroke: None,
        });
    }
}

fn draw_pieces(scene: &mut Scene, plan: &Plan, options: &Options) {
    for piece in &plan.pieces {
        match piece.kind {
            Kind::Name => {
                let style = parts::text_style(options, 1.15, true, options.theme.text);
                parts::label_at(scene, &piece.label, piece.at, &style, piece.anchor);
            }
            Kind::Subtitle => {
                let style = parts::text_style(options, 0.8, false, options.theme.dim);
                parts::label_at(scene, &piece.label, piece.at, &style, piece.anchor);
            }
            Kind::Badge => {
                scene.add(Item::Rect {
                    rect: piece.rect,
                    radius: 8.0,
                    fill: Some(options.theme.node_fill),
                    stroke: Some(Stroke::new(
                        options.theme.series(piece.domain.colour()),
                        parts::LINE,
                    )),
                });
                let style = parts::text_style(options, 0.9, false, options.theme.text);
                parts::centred_label(scene, &piece.label, piece.rect, &style);
            }
        }
    }
}

fn draw_panels(scene: &mut Scene, plan: &Plan, options: &Options) {
    for arrow in &plan.arrows {
        let Some((rect, label)) = &arrow.panel else { continue };
        scene.add(Item::Rect {
            rect: *rect,
            radius: 4.0,
            fill: Some(options.theme.node_fill),
            stroke: Some(Stroke::new(options.theme.node_stroke, parts::LINE)),
        });
        let style = parts::text_style(options, 0.85, false, options.theme.text);
        parts::centred_label(scene, label, *rect, &style);
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

    const FULL: &str = "cynefin-beta\n  title Incident Response\n  complex\n    \"Investigate root cause\"\n    \"Run chaos experiment\"\n  complicated\n    \"Analyze performance data\"\n    \"Expert review needed\"\n  clear\n    \"Restart service\"\n    \"Apply known fix\"\n  chaotic\n    \"Page on-call immediately\"\n  confusion\n    \"Unknown failure mode\"\n  complex --> complicated : \"Pattern identified\"\n  complicated --> clear : \"Best practice codified\"\n  clear --> chaotic : \"Complacency\"\n  chaotic --> complex : \"Stabilized\"\n";

    /// Whether a point is inside the ellipse.
    fn in_ellipse(ellipse: &Ellipse, point: Point) -> bool {
        let dx = (point.x - ellipse.centre.x) / ellipse.rx;
        let dy = (point.y - ellipse.centre.y) / ellipse.ry;
        dx * dx + dy * dy <= 1.0
    }

    /// Whether any part of a rectangle is inside the ellipse, tested along a grid.
    fn touches_ellipse(ellipse: &Ellipse, rect: &Rect) -> bool {
        (0..=10).any(|row| {
            (0..=20).any(|column| {
                let point = Point::new(
                    rect.x + rect.width * column as f32 / 20.0,
                    rect.y + rect.height * row as f32 / 10.0,
                );
                in_ellipse(ellipse, point)
            })
        })
    }

    #[test]
    fn domains_items_and_transitions_are_read() {
        let diagram = diagram(FULL);
        assert_eq!(diagram.title.as_deref(), Some("Incident Response"));
        assert_eq!(
            diagram.items[Domain::Complex.index()],
            vec!["Investigate root cause", "Run chaos experiment"]
        );
        assert_eq!(diagram.items[Domain::Confusion.index()], vec!["Unknown failure mode"]);
        assert_eq!(diagram.moves.len(), 4);
        assert_eq!(
            diagram.moves[2],
            Move {
                from: Domain::Clear,
                to: Domain::Chaotic,
                label: Some("Complacency".to_owned())
            }
        );
    }

    #[test]
    fn a_transition_needs_no_label_and_a_self_loop_is_dropped() {
        let diagram =
            diagram("cynefin-beta\n complex --> clear\n complex --> complex : \"nothing\"\n");
        assert_eq!(
            diagram.moves,
            vec![Move { from: Domain::Complex, to: Domain::Clear, label: None }]
        );
    }

    #[test]
    fn an_item_before_any_domain_is_refused() {
        let problem = check::refused("cynefin-beta\n \"Lost\"\n complex\n", &options());
        assert_eq!(problem.line, Some(2));
    }

    #[test]
    fn a_transition_to_something_that_is_not_a_domain_names_it() {
        let problem = check::refused("cynefin-beta\n complex --> simple : \"x\"\n", &options());
        assert_eq!(problem.line, Some(2));
        assert!(problem.reason.contains("`simple`"));
    }

    #[test]
    fn a_stray_line_says_what_a_line_looks_like() {
        let problem = check::refused("cynefin-beta\n complex\n Run an experiment\n", &options());
        assert_eq!(problem.line, Some(3));
        assert!(problem.reason.contains("quoted item"));
    }

    #[test]
    fn the_four_domains_are_in_their_places_whatever_order_they_are_written_in() {
        let text = "cynefin-beta\n clear\n   \"a\"\n chaotic\n   \"b\"\n complicated\n   \"c\"\n complex\n   \"d\"\n";
        let plan = plan(&diagram(text), &options());
        let centre = plan.centre();
        for piece in plan.pieces.iter().filter(|piece| piece.domain != Domain::Confusion) {
            let (column, row) = piece.domain.cell().expect("a quadrant");
            let rect = piece.rect;
            let (left, right) =
                if column == 0 { (0.0, centre.x) } else { (centre.x, plan.size().width) };
            let (top, bottom) =
                if row == 0 { (0.0, centre.y) } else { (centre.y, plan.size().height) };
            assert!(
                rect.left() >= left
                    && rect.right() <= right
                    && rect.top() >= top
                    && rect.bottom() <= bottom,
                "{:?} is outside its quadrant: {rect:?}",
                piece.domain
            );
        }
    }

    #[test]
    fn nothing_in_a_quadrant_is_under_the_ellipse_or_another_piece() {
        let texts = [
            FULL,
            "cynefin-beta\n complex\n \"One\"\n confusion\n \"a\"\n \"b\"\n \"c\"\n \"d\"\n \"e\"\n",
            "cynefin-beta\n complex\n \"A very long item that has to wrap onto several lines to fit its badge nicely\"\n \"Another quite long item that wraps too\"\n clear\n \"Short\"\n complex --> complicated : \"A long label that wraps over two lines\"\n complicated --> complex : \"Back again\"\n",
        ];
        for text in texts {
            let plan = plan(&diagram(text), &options());
            let quadrant_pieces: Vec<&Piece> =
                plan.pieces.iter().filter(|piece| piece.domain != Domain::Confusion).collect();
            for piece in &quadrant_pieces {
                assert!(
                    !touches_ellipse(&plan.ellipse, &piece.rect),
                    "the ellipse is over {:?} {:?}",
                    piece.kind,
                    piece.rect
                );
            }
            for (index, first) in quadrant_pieces.iter().enumerate() {
                for second in &quadrant_pieces[index + 1..] {
                    assert!(
                        !first.rect.overlaps(&second.rect),
                        "two pieces overlap: {:?} and {:?}",
                        first.rect,
                        second.rect
                    );
                }
            }
        }
    }

    #[test]
    fn a_badge_is_big_enough_for_its_wrapped_words() {
        let plan = plan(
            &diagram("cynefin-beta\n complex\n \"aaaa bbbb cccc dddd eeee ffff gggg\"\n"),
            &options(),
        );
        let badge = plan.pieces.iter().find(|piece| piece.kind == Kind::Badge).expect("a badge");
        assert!(badge.label.lines.len() > 1, "the words wrapped");
        assert!(badge.rect.width >= badge.label.width + BADGE_PAD_X * 2.0 - 0.01);
        assert!(badge.rect.height >= badge.label.height + BADGE_PAD_Y * 2.0 - 0.01);
    }

    #[test]
    fn confusion_lists_three_items_and_counts_the_rest() {
        let scene = check::drawn(
            "cynefin-beta\n confusion\n \"a1\"\n \"a2\"\n \"a3\"\n \"a4\"\n \"a5\"\n",
            &options(),
            &["a1", "a2", "a3", "+2 more"],
        );
        assert!(!scene.texts().contains(&"a4"), "the fourth is not listed");
    }

    #[test]
    fn the_confusion_items_are_inside_the_ellipse() {
        let plan = plan(
            &diagram("cynefin-beta\n confusion\n \"one\"\n \"two\"\n \"three\"\n \"four\"\n"),
            &options(),
        );
        for piece in plan.pieces.iter().filter(|piece| piece.domain == Domain::Confusion) {
            let rect = piece.rect;
            for point in [
                Point::new(rect.left(), rect.top()),
                Point::new(rect.right(), rect.top()),
                Point::new(rect.left(), rect.bottom()),
                Point::new(rect.right(), rect.bottom()),
            ] {
                assert!(
                    in_ellipse(&plan.ellipse, point),
                    "{point:?} of {:?} is outside the ellipse",
                    piece.kind
                );
            }
        }
    }

    #[test]
    fn label_panels_cover_no_badge_name_or_other_panel() {
        let plan = plan(&diagram(FULL), &options());
        let panels: Vec<Rect> = plan
            .arrows
            .iter()
            .filter_map(|arrow| arrow.panel.as_ref().map(|(rect, _)| *rect))
            .collect();
        assert_eq!(panels.len(), 4);
        for panel in &panels {
            for piece in &plan.pieces {
                assert!(!panel.overlaps(&piece.rect), "a panel covers {:?}", piece.rect);
            }
            assert!(!touches_ellipse(&plan.ellipse, panel), "a panel covers the ellipse");
        }
        check::no_two_rectangles_overlap(&panels);
    }

    #[test]
    fn transitions_between_opposite_domains_and_with_confusion_are_drawn() {
        let text = "cynefin-beta\n complex\n \"a\"\n clear\n \"b\"\n complex --> clear : \"Diagonal\"\n chaotic --> complicated : \"Other way\"\n confusion --> complex : \"Sorted\"\n clear --> confusion : \"Lost\"\n";
        let scene = check::drawn(text, &options(), &["Diagonal", "Other way", "Sorted", "Lost"]);
        check::boxes_nest_or_miss(&scene.rects());
    }

    #[test]
    fn two_transitions_over_one_boundary_have_lanes_of_their_own() {
        let text = "cynefin-beta\n complex --> complicated : \"Forward\"\n complicated --> complex : \"Back\"\n";
        let plan = plan(&diagram(text), &options());
        let first = plan.arrows[0].panel.as_ref().expect("a panel").0;
        let second = plan.arrows[1].panel.as_ref().expect("a panel").0;
        assert!(!first.overlaps(&second));
    }

    #[test]
    fn the_cliff_and_the_four_boundaries_are_drawn_and_the_boundaries_wave() {
        let scene = check::drawn(
            FULL,
            &options(),
            &["Complex", "Chaotic", "Probe, Sense, Respond", "Novel practice"],
        );
        let lines: Vec<&Item> = scene
            .items
            .iter()
            .filter(
                |item| matches!(item, Item::Line { points, .. } if points.len() == WAVE_STEPS + 1),
            )
            .collect();
        assert_eq!(lines.len(), 4, "four boundaries");
        let thick = scene
            .items
            .iter()
            .filter(|item| matches!(item, Item::Line { stroke, .. } if stroke.width > parts::THICK))
            .count();
        assert_eq!(thick, 1, "one cliff");
        let Item::Line { points, .. } = lines[0] else { panic!("a line") };
        let (start, end) = (points[0], points[points.len() - 1]);
        let straight = |point: &Point| {
            let t = (point.y - start.y) / (end.y - start.y);
            (point.x - (start.x + (end.x - start.x) * t)).abs()
        };
        let wander = points.iter().map(straight).fold(0.0_f32, f32::max);
        assert!(
            wander > 1.0 && wander <= WAVE + 0.01,
            "the boundary waves by up to {WAVE}: {wander}"
        );
    }

    #[test]
    fn the_documentation_examples_draw() {
        let basic = "cynefin-beta\n  title Incident Response\n\n  complex\n    \"Investigate root cause\"\n    \"Run chaos experiment\"\n\n  complicated\n    \"Analyze performance data\"\n    \"Expert review needed\"\n\n  clear\n    \"Restart service\"\n    \"Apply known fix\"\n\n  chaotic\n    \"Page on-call immediately\"\n\n  confusion\n    \"Unknown failure mode\"\n";
        check::drawn(
            basic,
            &options(),
            &["Incident Response", "Restart service", "Unknown failure mode"],
        );
        check::drawn(
            FULL,
            &options(),
            &["Pattern identified", "Best practice codified", "Complacency", "Stabilized"],
        );
        let empty = "cynefin-beta\n  title Cynefin Framework\n\n  complex\n  complicated\n  clear\n  chaotic\n";
        let scene = check::drawn(
            empty,
            &options(),
            &["Cynefin Framework", "Complex", "Complicated", "Clear", "Chaotic", "Confusion"],
        );
        check::boxes_nest_or_miss(&scene.rects());
        let accessible = "cynefin-beta\n  accTitle: Cynefin framework for software delivery decisions\n  accDescr: A map\n\n  complex\n    \"New feature discovery\"\n";
        check::drawn(accessible, &options(), &["New feature discovery"]);
    }

    #[test]
    fn every_prefix_of_a_diagram_draws_or_is_refused_without_panicking() {
        for (end, _) in FULL.char_indices().chain(std::iter::once((FULL.len(), ' '))) {
            let _ = super::super::render(&FULL[..end], &options());
        }
    }
}
