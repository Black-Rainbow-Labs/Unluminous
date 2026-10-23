//! Event modelling diagrams: `eventmodeling`.
//!
//! A timeline of frames. Each frame is a box in a column of its own, and the columns run left to right in
//! the order the frames were written. The rows are swimlanes, stacked top to bottom:
//!
//! - UI and automation (`ui`, `pcr`), at the top;
//! - commands and read models (`cmd`, `rmo`), in the middle;
//! - events (`evt`), at the bottom.
//!
//! A namespace written before a dot in the entity name (`Inventory.InventoryChanged`) gives that
//! combination of namespace and lane its own row, titled `Inventory: Events`.
//!
//! Decisions a reader might disagree with:
//!
//! - **Rows are put in the method's order, not in the order they first appear.** The method reads from the
//!   screen at the top to the events at the bottom, so the lanes are sorted by kind first and, inside one
//!   kind, by where they first appear in the text. A diagram that begins with an event still has its
//!   events at the bottom.
//! - **An arrow runs from the earlier thing to the later one.** A frame follows the frame written before
//!   it unless it is a reset frame. `->> n` says that frame `n` feeds this one (a read model is built from
//!   events), so the arrow runs from `n` to this frame.
//! - **Arrows never cross a box.** Each column holds exactly one frame, so the space between columns and
//!   the space between rows are always empty. Frames in neighbouring columns are joined through the space
//!   between the two columns. Any other pair is joined by leaving the top or bottom of the first box,
//!   running along the space between two rows and entering the top or bottom of the second box, whose
//!   column is empty in every other row. Several arrows in one space each get a track of their own, and
//!   several arrows on one side of a box each get a different point on it.
//! - **Colours** come from the theme's series: command 0 (blue), read model 1 (green), event 5 (orange),
//!   processor 6 (violet), UI 4 (teal). The colour says the kind and the lane says it again.
//! - **A data type in backticks is read and not used.** The data is shown as written, one line to a line of
//!   the source.

use super::parts;
use super::scene::{Anchor, Item, Paint, Point, Rect, Scene, Stroke};
use super::source::{Line, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// The widest a name or a line of data is wrapped before the box grows taller instead.
const CARD_WRAP: f32 = 240.0;
/// The narrowest a frame's box is.
const CARD_MIN_WIDTH: f32 = 100.0;
/// The space between the name and the data under it.
const DATA_GAP: f32 = 4.0;
/// The space between a lane's edge and the boxes in it.
const LANE_PAD: f32 = 10.0;
/// The space between two columns before any arrows are counted.
const COLUMN_GAP: f32 = 24.0;
/// The distance between neighbouring arrows in the same space.
const TRACK: f32 = 8.0;
/// Where the first track in a space begins.
const TRACK_START: f32 = 12.0;
/// The space left of the first column, after the lane titles.
const TITLE_GAP: f32 = 20.0;
/// The widest a lane's title is wrapped.
const TITLE_WRAP: f32 = 150.0;
/// The side of a box an arrow leaves or enters by.
const LEFT: usize = 0;
const RIGHT: usize = 1;
const TOP: usize = 2;
const BOTTOM: usize = 3;

/// What a frame is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ui,
    Processor,
    Command,
    ReadModel,
    Event,
}

impl Kind {
    /// The kind a type word names, in either spelling.
    fn from_word(word: &str) -> Option<Kind> {
        match word.to_ascii_lowercase().as_str() {
            "ui" => Some(Kind::Ui),
            "pcr" | "processor" => Some(Kind::Processor),
            "cmd" | "command" => Some(Kind::Command),
            "rmo" | "readmodel" => Some(Kind::ReadModel),
            "evt" | "event" => Some(Kind::Event),
            _ => None,
        }
    }

    /// Which of the three rows of the method the kind belongs to: 0 is the top.
    fn class(self) -> usize {
        match self {
            Kind::Ui | Kind::Processor => 0,
            Kind::Command | Kind::ReadModel => 1,
            Kind::Event => 2,
        }
    }

    /// The theme series colour the kind is drawn in.
    fn series(self) -> usize {
        match self {
            Kind::Ui => 4,
            Kind::Processor => 6,
            Kind::Command => 0,
            Kind::ReadModel => 1,
            Kind::Event => 5,
        }
    }
}

/// One frame of the timeline.
#[derive(Debug, Clone)]
struct Frame {
    line: Line,
    number: u64,
    reset: bool,
    kind: Kind,
    namespace: Option<String>,
    name: String,
    /// The name of a `[[data block]]` to show, resolved after the whole source is read.
    reference: Option<String>,
    /// The lines of data to show under the name.
    data: Vec<String>,
    /// The numbers of the frames named with `->>`.
    relations: Vec<u64>,
}

/// What follows an entity name on a frame line, or after the name of a data block.
#[derive(Debug, Default)]
struct Tail {
    inline: Option<String>,
    reference: Option<String>,
    relations: Vec<u64>,
}

#[derive(Debug, Default)]
struct Diagram {
    frames: Vec<Frame>,
    title: Option<String>,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    Ok(draw(&diagram, source, options))
}

// ----------------------------------------------------------------------------------------------
// Reading
// ----------------------------------------------------------------------------------------------

/// Read every statement into frames, then fill in the data blocks the frames refer to.
fn read(source: &Source) -> Result<Diagram, Problem> {
    let lines = source.statements();
    let mut diagram = Diagram::default();
    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if let Some(rest) = line.after_word("title") {
            diagram.title = Some(super::source::label(rest));
        } else if let Some((reset, rest)) = frame_word(line) {
            let frame = read_frame(&lines, &mut at, reset, rest)?;
            if diagram.frames.iter().any(|earlier| earlier.number == frame.number) {
                return Err(Problem::at(
                    line,
                    format!(
                        "frame number {} is already used by an earlier frame. Give each frame its own number.",
                        frame.number
                    ),
                ));
            }
            diagram.frames.push(frame);
        } else if let Some(rest) = line.after_word("data") {
            blocks.push(read_block(&lines, &mut at, rest)?);
        } else {
            return Err(Problem::at(
                line,
                "a line here starts with `tf`, `timeframe`, `rf`, `resetframe` or `data`.",
            ));
        }
        at += 1;
    }
    resolve(&mut diagram, &blocks)?;
    Ok(diagram)
}

/// Whether a line starts a frame, and whether it is a reset frame, with what follows the word.
fn frame_word(line: &Line) -> Option<(bool, &str)> {
    for word in ["timeframe", "tf"] {
        if let Some(rest) = line.after_word(word) {
            return Some((false, rest));
        }
    }
    for word in ["resetframe", "rf"] {
        if let Some(rest) = line.after_word(word) {
            return Some((true, rest));
        }
    }
    None
}

/// Read a frame whose line starts at `lines[*at]`, moving `at` past any lines its data runs over.
fn read_frame(lines: &[&Line], at: &mut usize, reset: bool, rest: &str) -> Result<Frame, Problem> {
    let line = lines[*at];
    let shape = "a frame looks like `tf 01 cmd AddItem { price: number } ->> 02`: a number, a type and a name.";
    let (number, rest) = next_word(rest);
    let Ok(number) = number.parse::<u64>() else {
        return Err(Problem::at(line, shape));
    };
    let (type_word, rest) = next_word(rest);
    let Some(kind) = Kind::from_word(type_word) else {
        return Err(Problem::at(
            line,
            format!(
                "`{type_word}` is not a frame type. The types are ui, pcr (processor), cmd (command), rmo (readmodel) and evt (event)."
            ),
        ));
    };
    let (entity, rest) = next_word(rest);
    if entity.is_empty() {
        return Err(Problem::at(line, shape));
    }
    let (namespace, name) = split_entity(entity);
    let whole = join_open_braces(lines, at, rest)?;
    let tail = read_tail(&whole).map_err(|reason| Problem::at(line, reason))?;
    Ok(Frame {
        line: line.clone(),
        number,
        reset,
        kind,
        namespace,
        name,
        reference: tail.reference,
        data: tail.inline.map(|inner| data_lines(&inner)).unwrap_or_default(),
        relations: tail.relations,
    })
}

/// Read a `data Id { ... }` block that may run over several lines.
fn read_block(
    lines: &[&Line],
    at: &mut usize,
    rest: &str,
) -> Result<(String, Vec<String>), Problem> {
    let line = lines[*at];
    let (id, rest) = next_word(rest);
    if id.is_empty() {
        return Err(Problem::at(line, "a data block looks like `data AddItem { price: number }`."));
    }
    let whole = join_open_braces(lines, at, rest)?;
    let tail = read_tail(&whole).map_err(|reason| Problem::at(line, reason))?;
    let Some(inner) = tail.inline else {
        return Err(Problem::at(line, format!("the data block `{id}` needs its data in `{{ }}`.")));
    };
    Ok((id.to_owned(), data_lines(&inner)))
}

/// The first word of `text` and what follows it. A word ends at a space or where data begins.
fn next_word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    let end = text
        .find(|c: char| c.is_whitespace() || matches!(c, '{' | '`' | '['))
        .unwrap_or(text.len());
    (&text[..end], &text[end..])
}

/// Split `Inventory.InventoryChanged` into its namespace and its name.
fn split_entity(entity: &str) -> (Option<String>, String) {
    match entity.split_once('.') {
        Some((namespace, name)) if !namespace.is_empty() && !name.is_empty() => {
            (Some(namespace.to_owned()), name.to_owned())
        }
        _ => (None, entity.to_owned()),
    }
}

/// The index just after the `}` that closes the `{` at the start of `text`, when there is one.
fn closing_brace(text: &str) -> Option<usize> {
    let mut depth = 0_i32;
    for (index, character) in text.char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth <= 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// `rest` joined with the lines after it until every `{` in it is closed. `at` moves to the last line used.
fn join_open_braces(lines: &[&Line], at: &mut usize, rest: &str) -> Result<String, Problem> {
    let start = lines[*at];
    let mut text = rest.to_owned();
    loop {
        let Some(open) = text.find('{') else {
            return Ok(text);
        };
        if closing_brace(&text[open..]).is_some() {
            return Ok(text);
        }
        if *at + 1 >= lines.len() {
            return Err(Problem::at(start, "this `{` is never closed with a `}`."));
        }
        *at += 1;
        text.push('\n');
        text.push_str(&lines[*at].text);
    }
}

/// Read what follows an entity name: a data type, data, a data block reference and relations.
fn read_tail(text: &str) -> Result<Tail, String> {
    let mut tail = Tail::default();
    let mut rest = text.trim();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("->>") {
            let (number, after) = next_number_word(after);
            let Ok(number) = number.parse::<u64>() else {
                return Err("`->>` should be followed by the number of a frame.".to_owned());
            };
            tail.relations.push(number);
            rest = after.trim_start();
        } else if let Some(after) = rest.strip_prefix('`') {
            let Some(end) = after.find('`') else {
                return Err("a data type in backticks is missing its closing backtick.".to_owned());
            };
            rest = after[end + 1..].trim_start();
        } else if rest.starts_with('{') {
            let Some(end) = closing_brace(rest) else {
                return Err("this `{` is never closed with a `}`.".to_owned());
            };
            let inner = rest[1..end - 1].trim().to_owned();
            tail.inline = Some(inner);
            rest = rest[end..].trim_start();
        } else if let Some(after) = rest.strip_prefix("[[") {
            let Some(end) = after.find("]]") else {
                return Err("a `[[data block]]` is missing its closing `]]`.".to_owned());
            };
            tail.reference = Some(after[..end].trim().to_owned());
            rest = after[end + 2..].trim_start();
        } else {
            let (word, _) = next_word(rest);
            let shown = if word.is_empty() { rest } else { word };
            return Err(format!(
                "`{shown}` does not belong after a frame's name. Data goes in `{{ }}`, a data block in `[[ ]]` and a relation is `->> 02`."
            ));
        }
    }
    Ok(tail)
}

/// The word that follows `->>`, ended by a space.
fn next_number_word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (&text[..end], &text[end..])
}

/// The lines of a piece of data, trimmed, without the empty ones.
fn data_lines(inner: &str) -> Vec<String> {
    inner.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect()
}

/// Put each data block's lines into the frames that refer to it, and check every relation is to a frame.
fn resolve(diagram: &mut Diagram, blocks: &[(String, Vec<String>)]) -> Result<(), Problem> {
    let numbers: Vec<u64> = diagram.frames.iter().map(|frame| frame.number).collect();
    for frame in &mut diagram.frames {
        if let Some(reference) = &frame.reference {
            let Some((_, lines)) = blocks.iter().rev().find(|(id, _)| id == reference) else {
                return Err(Problem::at(
                    &frame.line,
                    format!("the data block `{reference}` is never defined. It looks like `data {reference} {{ ... }}`."),
                ));
            };
            let mut shown = lines.clone();
            shown.extend(std::mem::take(&mut frame.data));
            frame.data = shown;
        }
        for relation in &frame.relations {
            if !numbers.contains(relation) {
                return Err(Problem::at(
                    &frame.line,
                    format!("`->> {relation}` names a frame that does not exist."),
                ));
            }
            if *relation == frame.number {
                return Err(Problem::at(&frame.line, "a frame cannot be related to itself."));
            }
        }
    }
    Ok(())
}

// ----------------------------------------------------------------------------------------------
// Lanes and arrows
// ----------------------------------------------------------------------------------------------

/// One row of the diagram.
#[derive(Debug, Clone, PartialEq)]
struct Lane {
    namespace: Option<String>,
    class: usize,
}

impl Lane {
    /// The title written at the left of the row.
    fn title(&self) -> String {
        let base = ["UI/Automation", "Command/Read Model", "Events"][self.class.min(2)];
        match &self.namespace {
            Some(namespace) => format!("{namespace}: {base}"),
            None => base.to_owned(),
        }
    }
}

/// The rows, top to bottom, and the row of each frame.
fn lanes_of(frames: &[Frame]) -> (Vec<Lane>, Vec<usize>) {
    let mut found: Vec<Lane> = Vec::new();
    let mut old_index = Vec::new();
    for frame in frames {
        let lane = Lane { namespace: frame.namespace.clone(), class: frame.kind.class() };
        let index = match found.iter().position(|known| *known == lane) {
            Some(index) => index,
            None => {
                found.push(lane);
                found.len() - 1
            }
        };
        old_index.push(index);
    }
    let mut order: Vec<usize> = (0..found.len()).collect();
    order.sort_by_key(|&index| found[index].class);
    let mut new_index = vec![0; found.len()];
    for (position, &old) in order.iter().enumerate() {
        new_index[old] = position;
    }
    let lanes = order.iter().map(|&old| found[old].clone()).collect();
    (lanes, old_index.iter().map(|&old| new_index[old]).collect())
}

/// Every arrow as a pair of frame positions, from the one that comes first to the one that follows.
fn links_of(frames: &[Frame]) -> Vec<(usize, usize)> {
    let mut links: Vec<(usize, usize)> = Vec::new();
    let mut add = |from: usize, to: usize| {
        if !links.contains(&(from, to)) {
            links.push((from, to));
        }
    };
    for (position, frame) in frames.iter().enumerate() {
        if position > 0 && !frame.reset {
            add(position - 1, position);
        }
        for relation in &frame.relations {
            if let Some(from) = frames.iter().position(|other| other.number == *relation) {
                add(from, position);
            }
        }
    }
    links
}

/// Where an arrow runs between its two ends.
#[derive(Debug, Clone, Copy)]
enum Route {
    /// Through the space between column `gap` and the next, on track number `track`.
    Column { gap: usize, track: usize },
    /// Through the space above row `gap`, on track number `track`.
    Lane { gap: usize, track: usize },
}

#[derive(Debug, Clone, Copy)]
struct Arrow {
    from: usize,
    to: usize,
    from_side: usize,
    to_side: usize,
    from_port: usize,
    to_port: usize,
    route: Route,
}

/// Every arrow, and how many tracks and points each space and each box side is asked for.
#[derive(Debug, Default)]
struct Plan {
    arrows: Vec<Arrow>,
    column_tracks: Vec<usize>,
    lane_tracks: Vec<usize>,
    /// For each frame, how many arrows use its left, right, top and bottom side.
    ports: Vec<[usize; 4]>,
}

/// Decide the route of every arrow.
fn plan(links: &[(usize, usize)], lane_of: &[usize], lane_count: usize) -> Plan {
    let mut plan = Plan {
        arrows: Vec::new(),
        column_tracks: vec![0; lane_of.len()],
        lane_tracks: vec![0; lane_count + 1],
        ports: vec![[0; 4]; lane_of.len()],
    };
    for &(from, to) in links {
        let (from_side, to_side, route) = route_of(&mut plan, from, to, lane_of);
        let from_port = claim_port(&mut plan, from, from_side);
        let to_port = claim_port(&mut plan, to, to_side);
        plan.arrows.push(Arrow { from, to, from_side, to_side, from_port, to_port, route });
    }
    plan
}

/// Which sides an arrow uses and which space it runs in, taking the next track there.
fn route_of(plan: &mut Plan, from: usize, to: usize, lane_of: &[usize]) -> (usize, usize, Route) {
    if from.abs_diff(to) == 1 {
        let gap = from.min(to);
        let track = plan.column_tracks[gap];
        plan.column_tracks[gap] += 1;
        let (from_side, to_side) = if to > from { (RIGHT, LEFT) } else { (LEFT, RIGHT) };
        return (from_side, to_side, Route::Column { gap, track });
    }
    let (from_lane, to_lane) = (lane_of[from], lane_of[to]);
    let (from_side, to_side, gap) = if to_lane > from_lane {
        (BOTTOM, TOP, from_lane + 1)
    } else if to_lane < from_lane {
        (TOP, BOTTOM, from_lane)
    } else {
        (TOP, TOP, from_lane)
    };
    let track = plan.lane_tracks[gap];
    plan.lane_tracks[gap] += 1;
    (from_side, to_side, Route::Lane { gap, track })
}

/// Take the next point on a side of a frame's box.
fn claim_port(plan: &mut Plan, frame: usize, side: usize) -> usize {
    let used = plan.ports[frame][side];
    plan.ports[frame][side] += 1;
    used
}

// ----------------------------------------------------------------------------------------------
// Measuring
// ----------------------------------------------------------------------------------------------

/// A frame's box, measured.
struct Card {
    name: Label,
    data: Option<Label>,
    width: f32,
    height: f32,
}

/// Measure a frame's name and data and the box that holds them.
fn card_of(frame: &Frame, options: &Options) -> Card {
    let name = text::measure(&frame.name, &options.style(1.0, true), options.metrics, CARD_WRAP);
    let data = (!frame.data.is_empty()).then(|| {
        text::measure(
            &frame.data.join("\n"),
            &options.style(0.8, false),
            options.metrics,
            CARD_WRAP,
        )
    });
    let widest = data.as_ref().map_or(name.width, |data| name.width.max(data.width));
    let mut height = parts::PADDING_Y * 2.0 + name.height;
    if let Some(data) = &data {
        height += DATA_GAP + data.height;
    }
    Card { width: (widest + parts::PADDING_X * 2.0).max(CARD_MIN_WIDTH), height, name, data }
}

/// Where everything is.
struct Geometry {
    column_x: Vec<f32>,
    lane_y: Vec<f32>,
    lane_height: Vec<f32>,
    gap_top: Vec<f32>,
    cards: Vec<Rect>,
    /// The right edge of the lane bands.
    right: f32,
    bottom: f32,
}

/// The left edge of every column, and the right edge of the last.
fn column_positions(cards: &[Card], plan: &Plan, left: f32) -> (Vec<f32>, f32) {
    let mut xs = Vec::new();
    let mut x = left;
    for (index, card) in cards.iter().enumerate() {
        xs.push(x);
        x += card.width;
        if index + 1 < cards.len() {
            x += COLUMN_GAP + TRACK * plan.column_tracks[index] as f32;
        }
    }
    (xs, x)
}

/// The top of every lane, the top of every space between lanes, and the bottom of the last space.
fn lane_positions(heights: &[f32], plan: &Plan, top: f32) -> (Vec<f32>, Vec<f32>, f32) {
    let mut lane_y = Vec::new();
    let mut gap_top = Vec::new();
    let mut y = top;
    for (index, height) in heights.iter().enumerate() {
        gap_top.push(y);
        y += gap_height(plan.lane_tracks[index], index > 0);
        lane_y.push(y);
        y += height;
    }
    gap_top.push(y);
    y += gap_height(plan.lane_tracks[heights.len()], true);
    (lane_y, gap_top, y)
}

/// How tall the space above or below a lane is, for the tracks that run in it.
fn gap_height(tracks: usize, between: bool) -> f32 {
    let tracks = tracks as f32;
    if tracks == 0.0 {
        return if between { 20.0 } else { 4.0 };
    }
    TRACK_START * 2.0 + TRACK * (tracks - 1.0)
}

// ----------------------------------------------------------------------------------------------
// Drawing
// ----------------------------------------------------------------------------------------------

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let mut scene = Scene::new();
    let mut titled = source.clone();
    if titled.title.is_none() {
        titled.title.clone_from(&diagram.title);
    }
    if diagram.frames.is_empty() {
        parts::title(&mut scene, &titled, options, 200.0);
        parts::finish(&mut scene);
        return scene;
    }
    let (lanes, lane_of) = lanes_of(&diagram.frames);
    let plan = plan(&links_of(&diagram.frames), &lane_of, lanes.len());
    let cards: Vec<Card> = diagram.frames.iter().map(|frame| card_of(frame, options)).collect();
    let titles: Vec<Label> = lanes
        .iter()
        .map(|lane| {
            text::measure(&lane.title(), &options.style(0.9, true), options.metrics, TITLE_WRAP)
        })
        .collect();
    let title_width = titles.iter().map(|label| label.width).fold(0.0_f32, f32::max);
    let left = parts::MARGIN + parts::PADDING_X + title_width + TITLE_GAP;
    let (column_x, last_right) = column_positions(&cards, &plan, left);
    let right = last_right + parts::PADDING_X;
    let top = parts::title(&mut scene, &titled, options, right + parts::MARGIN) + parts::MARGIN;
    let geometry = place(&cards, &lane_of, &plan, column_x, top, right);
    draw_lanes(&mut scene, &lanes, &titles, &geometry, options);
    draw_cards(&mut scene, diagram, &cards, &geometry, options);
    draw_arrows(&mut scene, &plan, &geometry, options);
    scene.claim(Rect::new(0.0, 0.0, geometry.right + parts::MARGIN, geometry.bottom));
    parts::finish(&mut scene);
    scene
}

/// Work out the rows and the box of every frame.
fn place(
    cards: &[Card],
    lane_of: &[usize],
    plan: &Plan,
    column_x: Vec<f32>,
    top: f32,
    right: f32,
) -> Geometry {
    let lane_count = plan.lane_tracks.len() - 1;
    let mut lane_height = vec![0.0_f32; lane_count];
    for (card, &lane) in cards.iter().zip(lane_of) {
        lane_height[lane] = lane_height[lane].max(card.height + LANE_PAD * 2.0);
    }
    let (lane_y, gap_top, bottom) = lane_positions(&lane_height, plan, top);
    let rects = cards
        .iter()
        .enumerate()
        .map(|(index, card)| {
            let lane = lane_of[index];
            let y = lane_y[lane] + (lane_height[lane] - card.height) / 2.0;
            Rect::new(column_x[index], y, card.width, card.height)
        })
        .collect();
    Geometry { column_x, lane_y, lane_height, gap_top, cards: rects, right, bottom }
}

/// The band behind each row, with its title at the left.
fn draw_lanes(
    scene: &mut Scene,
    lanes: &[Lane],
    titles: &[Label],
    geometry: &Geometry,
    options: &Options,
) {
    let style = parts::text_style(options, 0.9, true, options.theme.dim);
    for (index, title) in titles.iter().enumerate().take(lanes.len()) {
        let band = Rect::new(
            parts::MARGIN,
            geometry.lane_y[index],
            geometry.right - parts::MARGIN,
            geometry.lane_height[index],
        );
        scene.add(Item::Rect {
            rect: band,
            radius: parts::CORNER,
            fill: Some(Paint::faded(options.theme.grid, 70)),
            stroke: None,
        });
        let y = band.top() + (band.height - title.height) / 2.0;
        let at = Point::new(band.left() + parts::PADDING_X, y);
        parts::label_at(scene, title, at, &style, Anchor::Start);
    }
}

/// Each frame's box, with its name in bold and its data under it in a smaller face.
fn draw_cards(
    scene: &mut Scene,
    diagram: &Diagram,
    cards: &[Card],
    geometry: &Geometry,
    options: &Options,
) {
    let name_style = parts::text_style(options, 1.0, true, options.theme.text);
    let data_style = parts::text_style(options, 0.8, false, options.theme.text);
    for (index, frame) in diagram.frames.iter().enumerate() {
        let rect = geometry.cards[index];
        let series = frame.kind.series();
        scene.add(Item::Rect {
            rect,
            radius: parts::CORNER,
            fill: Some(options.theme.wash(series, 90)),
            stroke: Some(Stroke::new(options.theme.series(series), parts::LINE)),
        });
        let at = Point::new(rect.left() + parts::PADDING_X, rect.top() + parts::PADDING_Y);
        parts::label_at(scene, &cards[index].name, at, &name_style, Anchor::Start);
        if let Some(data) = &cards[index].data {
            let below = at.y + cards[index].name.height + DATA_GAP;
            let at = Point::new(at.x, below);
            parts::label_at(scene, data, at, &data_style, Anchor::Start);
        }
    }
}

/// The point on a side of a box where the arrow numbered `port` of `total` meets it.
fn side_point(rect: Rect, side: usize, port: usize, total: usize) -> Point {
    let spread = (port as f32 - (total.max(1) as f32 - 1.0) / 2.0) * TRACK;
    let along = if side == LEFT || side == RIGHT { rect.height } else { rect.width };
    let limit = (along / 2.0 - 6.0).max(0.0);
    let offset = spread.clamp(-limit, limit);
    let centre = rect.centre();
    match side {
        LEFT => Point::new(rect.left(), centre.y + offset),
        RIGHT => Point::new(rect.right(), centre.y + offset),
        TOP => Point::new(centre.x + offset, rect.top()),
        _ => Point::new(centre.x + offset, rect.bottom()),
    }
}

/// The corners of one arrow's path, from the first box to the second.
fn path_of(arrow: &Arrow, plan: &Plan, geometry: &Geometry, widths: &[f32]) -> Vec<Point> {
    let start = side_point(
        geometry.cards[arrow.from],
        arrow.from_side,
        arrow.from_port,
        plan.ports[arrow.from][arrow.from_side],
    );
    let end = side_point(
        geometry.cards[arrow.to],
        arrow.to_side,
        arrow.to_port,
        plan.ports[arrow.to][arrow.to_side],
    );
    match arrow.route {
        Route::Column { gap, track } => {
            let x = geometry.column_x[gap] + widths[gap] + TRACK_START + TRACK * track as f32;
            vec![start, Point::new(x, start.y), Point::new(x, end.y), end]
        }
        Route::Lane { gap, track } => {
            let y = geometry.gap_top[gap] + TRACK_START + TRACK * track as f32;
            vec![start, Point::new(start.x, y), Point::new(end.x, y), end]
        }
    }
}

/// Draw every arrow, with its head.
fn draw_arrows(scene: &mut Scene, plan: &Plan, geometry: &Geometry, options: &Options) {
    let widths: Vec<f32> = geometry.cards.iter().map(|rect| rect.width).collect();
    let colour = options.theme.line;
    for arrow in &plan.arrows {
        let path = path_of(arrow, plan, geometry, &widths);
        let tip = path[path.len() - 1];
        let direction = parts::heading(&path);
        let curved = parts::rounded(&path, 8.0, parts::HEAD);
        let drawn = parts::trimmed(&curved, 0.0, parts::ending_inset(parts::Ending::Arrow));
        scene.add(Item::Line {
            points: drawn,
            stroke: Stroke::new(colour, parts::LINE),
            dash: super::scene::Dash::Solid,
        });
        parts::ending(scene, parts::Ending::Arrow, tip, direction, colour, options.theme.node_fill);
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

    #[test]
    fn both_spellings_of_every_word_are_read() {
        let compact = diagram(
            "eventmodeling\ntf 01 ui A\ntf 02 pcr B\ntf 03 cmd C\ntf 04 rmo D\nrf 05 evt E\n",
        );
        let relaxed = diagram(
            "eventmodeling\ntimeframe 01 ui A\ntimeframe 02 processor B\ntimeframe 03 command C\n\
             timeframe 04 readmodel D\nresetframe 05 event E\n",
        );
        let kinds = |d: &Diagram| d.frames.iter().map(|f| (f.kind, f.reset)).collect::<Vec<_>>();
        assert_eq!(kinds(&compact), kinds(&relaxed));
        assert_eq!(compact.frames.len(), 5);
        assert!(compact.frames[4].reset);
    }

    #[test]
    fn a_namespace_is_split_off_the_name() {
        let diagram = diagram("eventmodeling\nrf 01 evt Inventory.InventoryChanged\n");
        assert_eq!(diagram.frames[0].namespace.as_deref(), Some("Inventory"));
        assert_eq!(diagram.frames[0].name, "InventoryChanged");
    }

    #[test]
    fn inline_data_with_and_without_a_type_is_read() {
        let diagram = diagram(
            "eventmodeling\ntf 01 cmd Add { description: string }\ntf 02 rmo User `json`{ \"name\": \"foo\" }\n",
        );
        assert_eq!(diagram.frames[0].data, vec!["description: string".to_owned()]);
        assert_eq!(diagram.frames[1].data, vec!["\"name\": \"foo\"".to_owned()]);
    }

    #[test]
    fn data_blocks_over_several_lines_fill_the_frames_that_name_them() {
        let diagram = diagram(
            "eventmodeling\ntf 01 cmd AddItem [[Item]]\ntf 02 evt ItemAdded [[Item]]\n\n\
             data Item {\n  description: string\n  price: number\n}\n",
        );
        let wanted = vec!["description: string".to_owned(), "price: number".to_owned()];
        assert_eq!(diagram.frames[0].data, wanted);
        assert_eq!(diagram.frames[1].data, wanted);
    }

    #[test]
    fn several_relations_on_one_line_are_all_read() {
        let diagram = diagram(
            "eventmodeling\nrf 02 evt A\nrf 03 evt B\nrf 04 evt C\ntf 01 rmo R ->> 02 ->> 03 ->> 04\n",
        );
        assert_eq!(diagram.frames[3].relations, vec![2, 3, 4]);
    }

    #[test]
    fn a_reused_frame_number_is_refused_with_its_line() {
        let problem = check::refused("eventmodeling\ntf 01 ui A\ntf 01 cmd B\n", &options());
        assert_eq!(problem.line, Some(3));
        assert!(problem.reason.contains("already used"));
    }

    #[test]
    fn a_bad_type_or_a_bad_line_is_refused_with_a_sentence() {
        let problem = check::refused("eventmodeling\ntf 01 widget A\n", &options());
        assert_eq!(problem.line, Some(2));
        assert!(problem.reason.contains("not a frame type"));
        let problem = check::refused("eventmodeling\ntf one ui A\n", &options());
        assert!(problem.reason.contains("a number"));
        let problem = check::refused("eventmodeling\nhello there\n", &options());
        assert_eq!(problem.line, Some(2));
    }

    #[test]
    fn an_unclosed_brace_a_missing_block_and_a_missing_frame_are_refused() {
        let problem = check::refused("eventmodeling\ntf 01 ui A { a: b\n", &options());
        assert!(problem.reason.contains("never closed"));
        let problem = check::refused("eventmodeling\ntf 01 ui A [[Nothing]]\n", &options());
        assert!(problem.reason.contains("never defined"));
        let problem = check::refused("eventmodeling\ntf 01 ui A ->> 09\n", &options());
        assert!(problem.reason.contains("does not exist"));
    }

    #[test]
    fn lanes_are_in_the_methods_order_with_namespaces_in_order_of_appearance() {
        let diagram = diagram(
            "eventmodeling\nrf 01 evt Inventory.Changed\nrf 02 evt External.Changed\n\
             tf 03 ui Screen\ntf 04 cmd Add\n",
        );
        let (lanes, lane_of) = lanes_of(&diagram.frames);
        let titles: Vec<String> = lanes.iter().map(Lane::title).collect();
        assert_eq!(
            titles,
            vec!["UI/Automation", "Command/Read Model", "Inventory: Events", "External: Events"]
        );
        assert_eq!(lane_of, vec![2, 3, 0, 1]);
    }

    #[test]
    fn a_frame_follows_the_one_before_it_unless_it_is_a_reset_frame() {
        let diagram = diagram(
            "eventmodeling\ntf 01 ui A\ntf 02 cmd B\nrf 03 evt C\ntf 04 evt D\nrf 05 rmo E ->> 01\n",
        );
        assert_eq!(links_of(&diagram.frames), vec![(0, 1), (2, 3), (0, 4)]);
    }

    #[test]
    fn the_basic_flow_is_drawn_in_three_rows_and_three_columns() {
        let scene = check::drawn(
            "eventmodeling\ntf 01 ui CartUI\ntf 02 cmd AddItem\ntf 03 evt ItemAdded\n",
            &options(),
            &["CartUI", "AddItem", "ItemAdded", "UI/Automation", "Events"],
        );
        let boxes = &scene.rects()[3..];
        assert_eq!(boxes.len(), 3);
        assert!(boxes[0].right() < boxes[1].left() && boxes[1].right() < boxes[2].left());
        assert!(boxes[0].bottom() < boxes[1].top() && boxes[1].bottom() < boxes[2].top());
    }

    #[test]
    fn data_is_inside_its_box_and_makes_the_box_taller() {
        let plain = check::drawn("eventmodeling\ntf 01 cmd Add\n", &options(), &["Add"]);
        let with = check::drawn(
            "eventmodeling\ntf 01 cmd Add [[A]]\n\ndata A {\n x: 1\n y: 2\n z: 3\n}\n",
            &options(),
            &["Add", "x: 1", "z: 3"],
        );
        let height = |scene: &Scene| scene.rects()[1].height;
        assert!(height(&with) > height(&plain) + 50.0);
        let card = with.rects()[1];
        for item in &with.items {
            if let Item::Text { at, text, .. } = item {
                if ["Add", "x: 1", "y: 2", "z: 3"].contains(&text.as_str()) {
                    assert!(at.y >= card.top() && at.y + 20.0 <= card.bottom() + 0.01, "{text}");
                    assert!(at.x >= card.left() && at.x <= card.right(), "{text}");
                }
            }
        }
    }

    #[test]
    fn boxes_keep_apart_and_lane_bands_hold_their_boxes() {
        let scene = check::drawn(
            "eventmodeling\ntf 01 ui CartUI\ntf 02 cmd AddItem { description: string }\n\
             tf 03 evt ItemAdded { description: string }\nrf 04 evt External.InventoryChanged\n\
             tf 05 pcr InventoryProcessor\ntf 06 cmd ChangeInventory\n\
             tf 07 evt Cart.InventoryChanged\ntf 08 rmo Items ->> 03 ->> 07\n",
            &options(),
            &["CartUI", "InventoryProcessor", "Items", "Cart: Events"],
        );
        check::boxes_nest_or_miss(&scene.rects());
    }

    #[test]
    fn an_arrow_between_distant_columns_misses_every_box() {
        let scene = check::drawn(
            "eventmodeling\nrf 02 evt A\nrf 03 evt B\nrf 04 evt C\nrf 05 evt D\n\
             tf 01 rmo R ->> 02 ->> 05\n",
            &options(),
            &["R", "A", "D"],
        );
        let boxes: Vec<Rect> = scene.rects().into_iter().skip(2).collect();
        for item in &scene.items {
            let Item::Line { points, .. } = item else { continue };
            for pair in points.windows(2) {
                for rect in &boxes {
                    assert!(!crosses(pair[0], pair[1], rect.grown(-1.0)), "{pair:?} {rect:?}");
                }
            }
        }
    }

    /// Whether the straight segment from `a` to `b` passes through the inside of `rect`.
    fn crosses(a: Point, b: Point, rect: Rect) -> bool {
        (0..=40).any(|step| {
            let point = a.towards(b, step as f32 / 40.0);
            rect.contains(point)
        })
    }

    #[test]
    fn an_empty_diagram_and_a_title_are_drawn() {
        let scene = check::drawn("eventmodeling\n title Shop\ntf 01 ui A\n", &options(), &["Shop"]);
        assert!(!scene.is_empty());
        assert!(render_empty().is_empty());
    }

    fn render_empty() -> Scene {
        let source = Source::read("eventmodeling\n").expect("a source");
        render(&source, &options()).expect("an empty diagram draws")
    }

    #[test]
    fn a_source_cut_off_anywhere_never_panics() {
        let text = "eventmodeling\ntf 01 ui CartUI `json`{ \"a\": 1 }\ntf 02 cmd X [[B]] ->> 01\ndata B {\n a: b\n}\n";
        for end in 0..=text.len() {
            if let Some(part) = text.get(..end) {
                let _ = super::super::render(part, &options());
            }
        }
    }

    #[test]
    fn the_documentation_examples_draw() {
        let examples = [
            "eventmodeling\n\ntf 01 ui CartUI\ntf 02 cmd AddItem { description: string }\ntf 03 evt ItemAdded { description: string }\n",
            "eventmodeling\n\nrf 02 evt CartCreated\nrf 03 evt ItemAdded\nrf 04 evt ItemRemoved\nrf 05 evt CartCleared\ntf 01 rmo CartUI ->> 02 ->> 03 ->> 04 ->> 05\n",
            "eventmodeling\n\ntf 03 evt External.InventoryChanged\ntf 02 pcr InventoryProcessor\ntf 04 cmd ChangeInventory\ntf 05 evt Cart.InventoryChanged\n",
            "eventmodeling\n\ntf 01 evt ItemAdded\ntf 02 ui OtherUI\nrf 03 rmo CartItems ->> 01\n",
        ];
        for example in examples {
            check::drawn(example, &options(), &[]);
        }
    }
}
