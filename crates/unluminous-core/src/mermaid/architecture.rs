//! Architecture diagrams: `architecture-beta`.
//!
//! Services with icons, the groups they belong to, junctions where lines meet, and edges that leave and
//! arrive by a named side: `db:R --> L:server` leaves the database by its right and arrives at the
//! server's left. That is the whole of what the author says about where things go, so it is what the
//! layout reads.
//!
//! ## A grid from the sides, a group at a time
//!
//! Mermaid hands this to a force directed layout seeded from the sides, and its own documentation
//! records the result: services that share a shape of edges land on top of each other, and `align` was
//! added to pull them apart. Unluminous builds a grid instead. An edge `a:R -- L:b` puts `b` one cell
//! right of `a`; `a:T -- L:b` puts it up and to the right. A breadth first walk from each item places
//! its neighbours, and a cell already taken pushes the newcomer on in the same direction.
//!
//! The grid is built one container at a time, from the inside out. A group's members are laid out in
//! the group's own grid, the group becomes one block of that size, and the blocks and the loose
//! services of the level above are laid out in that level's grid by the edges between them. So a
//! group's frame can never hold a service from somewhere else, which a single flat grid with frames
//! drawn round some of its cells cannot promise.
//!
//! ## Icons
//!
//! The five built in icons are drawn: `cloud`, `database`, `disk`, `internet` and `server`. Any other
//! icon would come from an icon pack Mermaid fetches, and nothing is fetched, so its tile shows the
//! icon's name instead.

use std::collections::{HashMap, VecDeque};

use super::parts::{self, Ending};
use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke};
use super::shapes::Shape;
use super::source::{self, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// How large a service's icon tile is.
const TILE: f32 = 52.0;
/// The space between two items side by side in a grid.
const COLUMN_GAP: f32 = 90.0;
/// The space between two rows of a grid.
const ROW_GAP: f32 = 70.0;
/// The space between a group's frame and what is inside it.
const GROUP_PADDING: f32 = 22.0;
/// How far an edge runs straight out of a side before it turns.
const STUB: f32 = 16.0;
/// How wide a service's title may get before it wraps.
const TITLE_WRAP: f32 = 140.0;

/// The side of a service an edge leaves or arrives by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    fn parse(text: &str) -> Option<Side> {
        match text.trim().to_ascii_uppercase().as_str() {
            "L" => Some(Side::Left),
            "R" => Some(Side::Right),
            "T" => Some(Side::Top),
            "B" => Some(Side::Bottom),
            _ => None,
        }
    }

    /// Which way out of the side is, as a step.
    fn outward(self) -> (f32, f32) {
        match self {
            Side::Left => (-1.0, 0.0),
            Side::Right => (1.0, 0.0),
            Side::Top => (0.0, -1.0),
            Side::Bottom => (0.0, 1.0),
        }
    }

    fn is_horizontal(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Service {
    id: String,
    title: String,
    icon: String,
    junction: bool,
    group: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct Group {
    id: String,
    title: String,
    icon: String,
    parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct Edge {
    from: usize,
    from_side: Side,
    /// `{group}` after the service: the edge leaves the service's group's frame rather than the service.
    from_group: bool,
    to: usize,
    to_side: Side,
    to_group: bool,
    tail: bool,
    head: bool,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Diagram {
    services: Vec<Service>,
    groups: Vec<Group>,
    edges: Vec<Edge>,
    /// `align row` (true) and `align column` (false), with the services each names, in order.
    aligns: Vec<(bool, Vec<usize>)>,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    Ok(draw(&diagram, source, options))
}

fn read(source: &Source) -> Result<Diagram, Problem> {
    let mut diagram = Diagram::default();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut groups: HashMap<String, usize> = HashMap::new();
    let mut pending_edges = Vec::new();
    let mut pending_aligns = Vec::new();
    let mut pending_parents: Vec<(bool, usize, String)> = Vec::new();
    for line in source.statements() {
        let text = line.text.trim();
        if let Some(rest) = line.after_word("group") {
            let (id, icon, title, parent) = declaration(rest);
            groups.insert(id.clone(), diagram.groups.len());
            if let Some(parent) = parent {
                pending_parents.push((true, diagram.groups.len(), parent));
            }
            diagram.groups.push(Group { id, title, icon, parent: None });
            continue;
        }
        if let Some(rest) = line.after_word("service") {
            let (id, icon, title, parent) = declaration(rest);
            by_id.insert(id.clone(), diagram.services.len());
            if let Some(parent) = parent {
                pending_parents.push((false, diagram.services.len(), parent));
            }
            diagram.services.push(Service { id, title, icon, junction: false, group: None });
            continue;
        }
        if let Some(rest) = line.after_word("junction") {
            let (id, _, _, parent) = declaration(rest);
            by_id.insert(id.clone(), diagram.services.len());
            if let Some(parent) = parent {
                pending_parents.push((false, diagram.services.len(), parent));
            }
            diagram.services.push(Service {
                id,
                title: String::new(),
                icon: String::new(),
                junction: true,
                group: None,
            });
            continue;
        }
        if let Some(rest) = line.after_word("align") {
            pending_aligns.push((line.clone(), rest.to_owned()));
            continue;
        }
        if text.contains("--") {
            pending_edges.push(((*line).clone(), text.to_owned()));
            continue;
        }
        if line.starts_with_word("title") {
            continue;
        }
        return Err(Problem::at(
            line,
            "this is not an architecture statement. A service looks like `service db(database)[Database]` and an edge like `db:R --> L:server`.",
        ));
    }
    for (is_group, index, parent) in pending_parents {
        let Some(&group) = groups.get(&parent) else {
            continue;
        };
        if is_group {
            if group != index {
                diagram.groups[index].parent = Some(group);
            }
        } else {
            diagram.services[index].group = Some(group);
        }
    }
    for (line, text) in pending_edges {
        let edge = read_edge(&text, &by_id).ok_or_else(|| {
            Problem::at(&line, "an edge looks like `db:R --> L:server`: a service, a side, the line, a side and a service.")
        })?;
        diagram.edges.push(edge);
    }
    for (line, text) in pending_aligns {
        let mut words = text.split_whitespace();
        let row = match words.next().map(str::to_ascii_lowercase).as_deref() {
            Some("row") => true,
            Some("column") => false,
            _ => {
                return Err(Problem::at(
                    &line,
                    "`align` is followed by `row` or `column` and the services to line up.",
                ))
            }
        };
        let members: Vec<usize> = words.filter_map(|word| by_id.get(word).copied()).collect();
        if members.len() >= 2 {
            diagram.aligns.push((row, members));
        }
    }
    Ok(diagram)
}

/// `id(icon)[Title] in parent`, any of the last three left out.
fn declaration(rest: &str) -> (String, String, String, Option<String>) {
    let (head, parent) = match rest.rsplit_once(" in ") {
        Some((head, parent)) if !parent.contains([']', ')']) => {
            (head.trim(), Some(parent.trim().to_owned()))
        }
        _ => (rest.trim(), None),
    };
    let id_end = head.find(['(', '[']).unwrap_or(head.len());
    let id = head[..id_end].trim().to_owned();
    let icon = match (head.find('('), head.find(')')) {
        (Some(open), Some(close)) if close > open => head[open + 1..close].trim().to_owned(),
        _ => String::new(),
    };
    let title = match (head.find('['), head.rfind(']')) {
        (Some(open), Some(close)) if close > open => source::label(&head[open + 1..close]),
        _ => String::new(),
    };
    (id, icon, title, parent)
}

/// `a{group}:R <--> L:b{group}`, with an optional `-[label]-` in place of `--`.
fn read_edge(text: &str, by_id: &HashMap<String, usize>) -> Option<Edge> {
    let (left, middle, right) = split_line(text)?;
    let tail = left.trim_end().ends_with('<');
    let head = right.trim_start().starts_with('>');
    let left = left.trim_end().trim_end_matches('<').trim();
    let right = right.trim_start().trim_start_matches('>').trim();
    let (from_name, from_side) = left.rsplit_once(':')?;
    let (to_side, to_name) = right.split_once(':')?;
    let (from_name, from_group) = group_modifier(from_name.trim());
    let (to_name, to_group) = group_modifier(to_name.trim());
    Some(Edge {
        from: *by_id.get(from_name)?,
        from_side: Side::parse(from_side)?,
        from_group,
        to: *by_id.get(to_name)?,
        to_side: Side::parse(to_side)?,
        to_group,
        tail,
        head,
        label: middle,
    })
}

/// The text before the line, the line's label, and the text after it.
fn split_line(text: &str) -> Option<(&str, String, &str)> {
    if let Some(open) = text.find("-[") {
        let close = text[open..].find("]-")? + open;
        return Some((&text[..open], source::label(&text[open + 2..close]), &text[close + 2..]));
    }
    let at = text.find("--")?;
    let end = at + text[at..].chars().take_while(|c| *c == '-').count();
    Some((&text[..at], String::new(), &text[end..]))
}

/// `server{group}` into the name and whether the modifier was there.
fn group_modifier(name: &str) -> (&str, bool) {
    match name.strip_suffix("{group}") {
        Some(name) => (name.trim(), true),
        None => (name, false),
    }
}

/// One thing placed in a container's grid: a service or a whole group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Thing {
    Service(usize),
    Group(usize),
}

/// A neighbour and the step from one thing to it.
type Step = (Thing, (i32, i32));

/// A container laid out: where each thing in it is, from its own top left, and how large it is.
#[derive(Debug, Clone, Default)]
struct Block {
    size: Size,
    /// Every service inside, at any depth, and the rectangle it was given.
    services: Vec<(usize, Rect)>,
    /// Every group inside, at any depth, and its frame.
    groups: Vec<(usize, Rect)>,
}

/// The measurements of a service: its tile and its title.
struct Measured {
    title: Label,
    size: Size,
}

fn measure(service: &Service, options: &Options) -> Measured {
    let title =
        text::measure(&service.title, &options.style(0.85, false), options.metrics, TITLE_WRAP);
    let size = match service.junction {
        true => Size::new(12.0, 12.0),
        false => Size::new(
            TILE.max(title.width),
            TILE + 6.0 + if service.title.is_empty() { 0.0 } else { title.height },
        ),
    };
    Measured { title, size }
}

/// Which thing at this level holds service `index`: itself, or the outermost child group of
/// `container` it is inside.
fn holder(diagram: &Diagram, index: usize, container: Option<usize>) -> Option<Thing> {
    let mut group = diagram.services[index].group;
    if group == container {
        return Some(Thing::Service(index));
    }
    while let Some(at) = group {
        if diagram.groups[at].parent == container {
            return Some(Thing::Group(at));
        }
        group = diagram.groups[at].parent;
    }
    None
}

/// The step from `a` to `b` an edge between them asks for, from the sides it names.
fn step(from_side: Side, to_side: Side) -> (i32, i32) {
    let dx = match (to_side, from_side) {
        (Side::Left, _) => 1,
        (Side::Right, _) => -1,
        (_, Side::Right) => 1,
        (_, Side::Left) => -1,
        _ => 0,
    };
    let dy = match (from_side, to_side) {
        (Side::Bottom, _) => 1,
        (Side::Top, _) => -1,
        (_, Side::Top) => 1,
        (_, Side::Bottom) => -1,
        _ => 0,
    };
    if (dx, dy) == (0, 0) {
        (1, 0)
    } else {
        (dx, dy)
    }
}

/// Lay out the container `container` (the whole diagram when `None`) and everything inside it.
fn layout(
    diagram: &Diagram,
    container: Option<usize>,
    measured: &[Measured],
    options: &Options,
) -> Block {
    let mut things: Vec<Thing> = Vec::new();
    for (index, group) in diagram.groups.iter().enumerate() {
        if group.parent == container {
            things.push(Thing::Group(index));
        }
    }
    for (index, service) in diagram.services.iter().enumerate() {
        if service.group == container {
            things.push(Thing::Service(index));
        }
    }
    // Sorted into the order they were written, groups and services interleaved by first appearance.
    things.sort_by_key(|thing| match thing {
        Thing::Group(index) => (*index, 0),
        Thing::Service(index) => (*index, 1),
    });
    let blocks: HashMap<Thing, Block> = things
        .iter()
        .map(|thing| {
            let block = match thing {
                Thing::Service(index) => Block {
                    size: measured[*index].size,
                    services: vec![(
                        *index,
                        Rect::new(
                            0.0,
                            0.0,
                            measured[*index].size.width,
                            measured[*index].size.height,
                        ),
                    )],
                    groups: Vec::new(),
                },
                Thing::Group(index) => framed(diagram, *index, measured, options),
            };
            (*thing, block)
        })
        .collect();
    let cells = place(diagram, container, &things);
    arrange(&things, &cells, &blocks)
}

/// A group's own contents laid out, with its frame and title round them.
fn framed(diagram: &Diagram, index: usize, measured: &[Measured], options: &Options) -> Block {
    let inner = layout(diagram, Some(index), measured, options);
    let title = text::measure_unwrapped(
        &diagram.groups[index].title,
        &options.style(0.95, true),
        options.metrics,
    );
    let header = title.height + 14.0;
    let width = (inner.size.width + GROUP_PADDING * 2.0).max(title.width + 52.0);
    let height = inner.size.height + GROUP_PADDING * 2.0 + header;
    let dx = (width - inner.size.width) / 2.0;
    let dy = GROUP_PADDING + header;
    let mut block =
        Block { size: Size::new(width, height), services: Vec::new(), groups: Vec::new() };
    block.groups.push((index, Rect::new(0.0, 0.0, width, height)));
    for (service, rect) in inner.services {
        block.services.push((service, rect.moved(dx, dy)));
    }
    for (group, rect) in inner.groups {
        block.groups.push((group, rect.moved(dx, dy)));
    }
    block
}

/// Give every thing in a container a cell, from the edges between them and the alignments.
fn place(
    diagram: &Diagram,
    container: Option<usize>,
    things: &[Thing],
) -> HashMap<Thing, (i32, i32)> {
    // The steps between things at this level, both ways round.
    let mut neighbours: HashMap<Thing, Vec<Step>> = HashMap::new();
    for edge in &diagram.edges {
        let (Some(a), Some(b)) =
            (holder(diagram, edge.from, container), holder(diagram, edge.to, container))
        else {
            continue;
        };
        if a == b {
            continue;
        }
        let (dx, dy) = step(edge.from_side, edge.to_side);
        neighbours.entry(a).or_default().push((b, (dx, dy)));
        neighbours.entry(b).or_default().push((a, (-dx, -dy)));
    }
    let mut cells: HashMap<Thing, (i32, i32)> = HashMap::new();
    let mut taken: HashMap<(i32, i32), Thing> = HashMap::new();
    let mut next_column = 0;
    for &start in things {
        if cells.contains_key(&start) {
            continue;
        }
        // A new group of connected things starts to the right of everything placed so far.
        let origin = (next_column, 0);
        let origin = free(&taken, origin, (1, 0));
        cells.insert(start, origin);
        taken.insert(origin, start);
        let mut queue = VecDeque::from([start]);
        while let Some(thing) = queue.pop_front() {
            let here = cells[&thing];
            for &(other, (dx, dy)) in neighbours.get(&thing).map(Vec::as_slice).unwrap_or(&[]) {
                if cells.contains_key(&other) {
                    continue;
                }
                let cell = free(&taken, (here.0 + dx, here.1 + dy), (dx, dy));
                cells.insert(other, cell);
                taken.insert(cell, other);
                queue.push_back(other);
            }
        }
        next_column = taken.keys().map(|cell| cell.0).max().unwrap_or(0) + 1;
    }
    for (row, members) in &diagram.aligns {
        let members: Vec<Thing> = members
            .iter()
            .filter_map(|member| holder(diagram, *member, container))
            .filter(|thing| matches!(thing, Thing::Service(_)))
            .collect();
        let Some(first) = members.first().and_then(|thing| cells.get(thing).copied()) else {
            continue;
        };
        for (offset, thing) in members.iter().enumerate().skip(1) {
            let wanted = if *row {
                (first.0 + offset as i32, first.1)
            } else {
                (first.0, first.1 + offset as i32)
            };
            let current = cells[thing];
            if current == wanted {
                continue;
            }
            taken.remove(&current);
            let cell = free(&taken, wanted, if *row { (1, 0) } else { (0, 1) });
            cells.insert(*thing, cell);
            taken.insert(cell, *thing);
        }
    }
    cells
}

/// The first cell from `cell` onwards in the direction `step` that nothing has taken.
fn free(taken: &HashMap<(i32, i32), Thing>, mut cell: (i32, i32), step: (i32, i32)) -> (i32, i32) {
    let step = if step == (0, 0) { (1, 0) } else { step };
    while taken.contains_key(&cell) {
        cell = (cell.0 + step.0, cell.1 + step.1);
    }
    cell
}

/// Turn cells into positions: each column as wide as its widest thing, each row as tall as its tallest.
fn arrange(
    things: &[Thing],
    cells: &HashMap<Thing, (i32, i32)>,
    blocks: &HashMap<Thing, Block>,
) -> Block {
    if things.is_empty() {
        return Block::default();
    }
    let left = cells.values().map(|cell| cell.0).min().unwrap_or(0);
    let top = cells.values().map(|cell| cell.1).min().unwrap_or(0);
    let columns = (cells.values().map(|cell| cell.0).max().unwrap_or(0) - left + 1) as usize;
    let rows = (cells.values().map(|cell| cell.1).max().unwrap_or(0) - top + 1) as usize;
    let mut widths = vec![0.0_f32; columns];
    let mut heights = vec![0.0_f32; rows];
    for thing in things {
        let (column, row) = cells[thing];
        let size = blocks[thing].size;
        let (column, row) = ((column - left) as usize, (row - top) as usize);
        widths[column] = widths[column].max(size.width);
        heights[row] = heights[row].max(size.height);
    }
    let x_of: Vec<f32> = widths
        .iter()
        .scan(0.0, |at, width| {
            let here = *at;
            *at += width + COLUMN_GAP;
            Some(here)
        })
        .collect();
    let y_of: Vec<f32> = heights
        .iter()
        .scan(0.0, |at, height| {
            let here = *at;
            *at += height + ROW_GAP;
            Some(here)
        })
        .collect();
    let mut out = Block {
        size: Size::new(
            widths.iter().sum::<f32>() + COLUMN_GAP * (columns - 1) as f32,
            heights.iter().sum::<f32>() + ROW_GAP * (rows - 1) as f32,
        ),
        services: Vec::new(),
        groups: Vec::new(),
    };
    for thing in things {
        let (column, row) = cells[thing];
        let (column, row) = ((column - left) as usize, (row - top) as usize);
        let block = &blocks[thing];
        let dx = x_of[column] + (widths[column] - block.size.width) / 2.0;
        let dy = y_of[row] + (heights[row] - block.size.height) / 2.0;
        for (service, rect) in &block.services {
            out.services.push((*service, rect.moved(dx, dy)));
        }
        for (group, rect) in &block.groups {
            out.groups.push((*group, rect.moved(dx, dy)));
        }
    }
    out
}

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let measured: Vec<Measured> =
        diagram.services.iter().map(|service| measure(service, options)).collect();
    let block = layout(diagram, None, &measured, options);
    let mut scene = Scene::new();
    let top = parts::title(&mut scene, source, options, block.size.width);
    let origin = Point::new(parts::MARGIN, top + parts::MARGIN);
    let services: HashMap<usize, Rect> = block
        .services
        .iter()
        .map(|(index, rect)| (*index, rect.moved(origin.x, origin.y)))
        .collect();
    let groups: HashMap<usize, Rect> =
        block.groups.iter().map(|(index, rect)| (*index, rect.moved(origin.x, origin.y))).collect();
    let mut drawn_groups: Vec<(usize, Rect)> =
        groups.iter().map(|(index, rect)| (*index, *rect)).collect();
    drawn_groups.sort_by_key(|(index, _)| *index);
    for (_, frame) in &drawn_groups {
        draw_group_frame(&mut scene, *frame, options);
    }
    for edge in &diagram.edges {
        draw_edge(&mut scene, diagram, edge, &services, &groups, options);
    }
    // The titles go on after the lines, on a panel, because a line into a group crosses the top of
    // its frame where the title is.
    for (index, frame) in &drawn_groups {
        draw_group_title(&mut scene, &diagram.groups[*index], *frame, options);
    }
    for (index, service) in diagram.services.iter().enumerate() {
        if let Some(rect) = services.get(&index) {
            draw_service(&mut scene, service, &measured[index], *rect, options);
        }
    }
    scene.claim(Rect::new(0.0, 0.0, origin.x + block.size.width, origin.y + block.size.height));
    parts::finish(&mut scene);
    scene
}

/// A group's dashed frame.
fn draw_group_frame(scene: &mut Scene, frame: Rect, options: &Options) {
    let theme = &options.theme;
    scene.add(Item::Rect {
        rect: frame,
        radius: parts::CORNER,
        fill: Some(theme.group_fill),
        stroke: None,
    });
    let corners = vec![
        Point::new(frame.left(), frame.top()),
        Point::new(frame.right(), frame.top()),
        Point::new(frame.right(), frame.bottom()),
        Point::new(frame.left(), frame.bottom()),
        Point::new(frame.left(), frame.top()),
    ];
    scene.add(Item::Line {
        points: corners,
        stroke: Stroke::new(theme.dim, parts::LINE),
        dash: Dash::Dashed(6.0, 4.0),
    });
}

/// A group's icon and title in the top left of its frame, on a panel of their own.
fn draw_group_title(scene: &mut Scene, group: &Group, frame: Rect, options: &Options) {
    let theme = &options.theme;
    let title_style = options.style(0.95, true);
    let title = text::measure_unwrapped(&group.title, &title_style, options.metrics);
    let icon = Rect::new(frame.left() + 10.0, frame.top() + 7.0, 18.0, 18.0);
    let x = if group.icon.is_empty() { frame.left() + 10.0 } else { icon.right() + 8.0 };
    let panel = Rect::new(
        frame.left() + 6.0,
        frame.top() + 4.0,
        x + title.width + 4.0 - (frame.left() + 6.0),
        title.height.max(18.0) + 6.0,
    );
    scene.add(Item::Rect {
        rect: panel,
        radius: 3.0,
        fill: Some(Paint::solid(theme.node_fill.color)),
        stroke: None,
    });
    if !group.icon.is_empty() {
        draw_icon(scene, &group.icon, icon, options, false);
    }
    parts::one_line(
        scene,
        &group.title,
        Point::new(x, frame.top() + 7.0),
        &parts::text_style(options, 0.95, true, theme.text),
        Anchor::Start,
        title.width,
    );
}

/// A service: its icon tile with its title under it, or a dot for a junction.
fn draw_service(
    scene: &mut Scene,
    service: &Service,
    measured: &Measured,
    rect: Rect,
    options: &Options,
) {
    let theme = &options.theme;
    if service.junction {
        scene.add(Item::Circle {
            centre: rect.centre(),
            radius: 4.0,
            fill: Some(Paint::solid(theme.line)),
            stroke: None,
        });
        return;
    }
    let tile = tile_of(rect);
    scene.add(Item::Rect {
        rect: tile,
        radius: 8.0,
        fill: Some(theme.node_fill),
        stroke: Some(Stroke::new(theme.node_stroke, parts::LINE)),
    });
    draw_icon(scene, &service.icon, tile.grown(-9.0), options, true);
    if !service.title.is_empty() {
        parts::label_at(
            scene,
            &measured.title,
            Point::new(rect.centre().x, tile.bottom() + 5.0),
            &parts::text_style(options, 0.85, false, theme.text),
            Anchor::Middle,
        );
    }
}

/// Where a service's icon tile is inside the rectangle the layout gave it.
fn tile_of(rect: Rect) -> Rect {
    Rect::new(rect.centre().x - TILE / 2.0, rect.top(), TILE, TILE)
}

/// One of the built in icons, drawn into `rect`, or the icon's name for any other.
fn draw_icon(scene: &mut Scene, icon: &str, rect: Rect, options: &Options, large: bool) {
    let theme = &options.theme;
    let colour = theme.accent;
    let stroke = Stroke::new(colour, if large { 1.8 } else { 1.3 });
    let fill = Paint::faded(colour, 50);
    let name = icon.rsplit(':').next().unwrap_or(icon).to_ascii_lowercase();
    match name.as_str() {
        "" => {}
        "cloud" => Shape::Cloud.draw(
            scene,
            Rect::new(rect.x, rect.y + rect.height * 0.18, rect.width, rect.height * 0.64),
            fill,
            stroke,
        ),
        "database" => Shape::Cylinder.draw(
            scene,
            Rect::new(rect.x + rect.width * 0.14, rect.y, rect.width * 0.72, rect.height),
            fill,
            stroke,
        ),
        "disk" => Shape::Cylinder.draw(
            scene,
            Rect::new(rect.x, rect.y + rect.height * 0.25, rect.width, rect.height * 0.5),
            fill,
            stroke,
        ),
        "internet" => {
            let centre = rect.centre();
            let radius = rect.width.min(rect.height) / 2.0;
            scene.add(Item::Circle { centre, radius, fill: Some(fill), stroke: Some(stroke) });
            let meridian: Vec<Point> = (0..=24)
                .map(|step| {
                    let angle = step as f32 / 24.0 * std::f32::consts::TAU;
                    Point::new(
                        centre.x + radius * 0.45 * angle.cos(),
                        centre.y + radius * angle.sin(),
                    )
                })
                .collect();
            scene.add(Item::Line { points: meridian, stroke, dash: Dash::Solid });
            for (y, half) in [(0.0, 1.0), (-0.5, 0.86), (0.5, 0.86)] {
                scene.add(Item::Line {
                    points: vec![
                        Point::new(centre.x - radius * half, centre.y + radius * y),
                        Point::new(centre.x + radius * half, centre.y + radius * y),
                    ],
                    stroke,
                    dash: Dash::Solid,
                });
            }
        }
        "server" => {
            let unit = rect.height / 3.0;
            for index in 0..3 {
                let row =
                    Rect::new(rect.x, rect.y + unit * index as f32 + 1.0, rect.width, unit - 2.0);
                scene.add(Item::Rect {
                    rect: row,
                    radius: 2.0,
                    fill: Some(fill),
                    stroke: Some(stroke),
                });
                scene.add(Item::Circle {
                    centre: Point::new(row.right() - 6.0, row.centre().y),
                    radius: 1.8,
                    fill: Some(Paint::solid(colour)),
                    stroke: None,
                });
            }
        }
        other => {
            // An icon from a pack is shown by name, small, because nothing is fetched to draw it.
            let style = options.style(if large { 0.6 } else { 0.5 }, false);
            let label = text::measure(other, &style, options.metrics, rect.width.max(30.0));
            if large {
                parts::centred_label(
                    scene,
                    &label,
                    rect,
                    &parts::text_style(options, 0.6, false, theme.dim),
                );
            }
        }
    }
}

/// Where an edge meets a side of `rect`, part of the way along it.
fn port(rect: Rect, side: Side) -> Point {
    match side {
        Side::Left => Point::new(rect.left(), rect.centre().y),
        Side::Right => Point::new(rect.right(), rect.centre().y),
        Side::Top => Point::new(rect.centre().x, rect.top()),
        Side::Bottom => Point::new(rect.centre().x, rect.bottom()),
    }
}

/// The rectangle one end of an edge leaves: the service's tile, or with `{group}` the frame of the
/// group the service is in.
fn end_rect(
    diagram: &Diagram,
    service: usize,
    group_end: bool,
    side: Side,
    services: &HashMap<usize, Rect>,
    groups: &HashMap<usize, Rect>,
) -> Option<Rect> {
    let rect = *services.get(&service)?;
    if group_end {
        if let Some(group) = diagram.services[service].group.and_then(|group| groups.get(&group)) {
            return Some(*group);
        }
    }
    // A line leaving by the bottom leaves from under the title, or it would run through the words.
    Some(match (diagram.services[service].junction, side) {
        (true, _) => Rect::around(rect.centre(), Size::new(8.0, 8.0)),
        (false, Side::Bottom) => {
            let tile = tile_of(rect);
            Rect::new(tile.x, tile.y, tile.width, rect.height + 2.0)
        }
        (false, _) => tile_of(rect),
    })
}

fn draw_edge(
    scene: &mut Scene,
    diagram: &Diagram,
    edge: &Edge,
    services: &HashMap<usize, Rect>,
    groups: &HashMap<usize, Rect>,
    options: &Options,
) {
    let theme = &options.theme;
    let (Some(from), Some(to)) = (
        end_rect(diagram, edge.from, edge.from_group, edge.from_side, services, groups),
        end_rect(diagram, edge.to, edge.to_group, edge.to_side, services, groups),
    ) else {
        return;
    };
    let start = port(from, edge.from_side);
    let end = port(to, edge.to_side);
    let (ox, oy) = edge.from_side.outward();
    let (ix, iy) = edge.to_side.outward();
    let a = Point::new(start.x + ox * STUB, start.y + oy * STUB);
    let b = Point::new(end.x + ix * STUB, end.y + iy * STUB);
    let mut points = vec![start, a];
    match (edge.from_side.is_horizontal(), edge.to_side.is_horizontal()) {
        (true, true) => {
            let middle = (a.x + b.x) / 2.0;
            points.push(Point::new(middle, a.y));
            points.push(Point::new(middle, b.y));
        }
        (false, false) => {
            let middle = (a.y + b.y) / 2.0;
            points.push(Point::new(a.x, middle));
            points.push(Point::new(b.x, middle));
        }
        (true, false) => points.push(Point::new(b.x, a.y)),
        (false, true) => points.push(Point::new(a.x, b.y)),
    }
    points.push(b);
    points.push(end);
    let points = dedupe(points);
    let rounded = parts::rounded(&points, 8.0, parts::HEAD);
    let head = if edge.head { Ending::Arrow } else { Ending::None };
    let tail = if edge.tail { Ending::Arrow } else { Ending::None };
    let drawn = parts::trimmed(&rounded, parts::ending_inset(tail), parts::ending_inset(head));
    scene.add(Item::Line {
        points: drawn,
        stroke: Stroke::new(theme.line, parts::LINE),
        dash: Dash::Solid,
    });
    if rounded.len() >= 2 {
        parts::ending(
            scene,
            head,
            rounded[rounded.len() - 1],
            parts::heading(&rounded),
            theme.line,
            theme.node_fill,
        );
        parts::ending(
            scene,
            tail,
            rounded[0],
            parts::tail_heading(&rounded),
            theme.line,
            theme.node_fill,
        );
    }
    if !edge.label.is_empty() {
        let label = text::measure(
            &edge.label,
            &options.style(0.8, false),
            options.metrics,
            text::EDGE_WRAP,
        );
        let (p, q) = longest_segment(&points);
        let panel =
            Rect::around(p.towards(q, 0.5), Size::new(label.width + 8.0, label.height + 2.0));
        scene.add(Item::Rect {
            rect: panel,
            radius: 3.0,
            fill: Some(Paint::solid(theme.node_fill.color)),
            stroke: None,
        });
        parts::centred_label(
            scene,
            &label,
            panel,
            &parts::text_style(options, 0.8, false, theme.dim),
        );
    }
}

/// The same points with any repeated one taken out.
fn dedupe(points: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for point in points {
        if out.last().is_none_or(|last| last.distance(point) > 0.5) {
            out.push(point);
        }
    }
    out
}

/// The two ends of the longest segment of a line, which is where its label has the most room.
fn longest_segment(points: &[Point]) -> (Point, Point) {
    points
        .windows(2)
        .max_by(|a, b| a[0].distance(a[1]).total_cmp(&b[0].distance(b[1])))
        .map_or((Point::default(), Point::default()), |pair| (pair[0], pair[1]))
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

    const API: &str = "architecture-beta\n    group api(cloud)[API]\n\n    service db(database)[Database] in api\n    service disk1(disk)[Storage] in api\n    service disk2(disk)[Storage] in api\n    service server(server)[Server] in api\n\n    db:L -- R:server\n    disk1:T -- B:server\n    disk2:T -- B:db\n";

    #[test]
    fn groups_services_and_edges_are_read_with_their_sides() {
        let diagram = diagram(API);
        assert_eq!(diagram.groups.len(), 1);
        assert_eq!(diagram.services.len(), 4);
        assert_eq!(diagram.services[0].icon, "database");
        assert_eq!(diagram.services[0].group, Some(0));
        let edge = &diagram.edges[0];
        assert_eq!((edge.from_side, edge.to_side), (Side::Left, Side::Right));
        assert!(!edge.head && !edge.tail);
    }

    #[test]
    fn arrows_group_modifiers_junctions_and_aligns_are_read() {
        let diagram = diagram(
            "architecture-beta\n group one(cloud)[One]\n group two(cloud)[Two]\n service server[Server] in one\n service subnet[Subnet] in two\n junction middle\n server{group}:B --> T:subnet{group}\n middle:R <-- L:server\n align row server middle\n",
        );
        assert!(diagram.edges[0].head && diagram.edges[0].from_group && diagram.edges[0].to_group);
        assert!(diagram.edges[1].tail);
        assert!(diagram.services[2].junction);
        assert_eq!(diagram.aligns, vec![(true, vec![0, 2])]);
    }

    #[test]
    fn an_edge_places_its_target_on_the_side_it_names() {
        let diagram = diagram(API);
        let measured: Vec<Measured> =
            diagram.services.iter().map(|service| measure(service, &options())).collect();
        let block = layout(&diagram, None, &measured, &options());
        let rect = |index: usize| block.services.iter().find(|(at, _)| *at == index).unwrap().1;
        // db:L -- R:server: the server is left of the database.
        assert!(rect(3).right() < rect(0).left(), "{:?} {:?}", rect(3), rect(0));
        // disk1:T -- B:server: the first disk is below the server.
        assert!(rect(1).top() > rect(3).bottom());
        // disk2:T -- B:db: the second disk is below the database.
        assert!(rect(2).top() > rect(0).bottom());
    }

    #[test]
    fn a_group_frame_holds_only_its_own_services() {
        let text = "architecture-beta\n group a(cloud)[A]\n group b(cloud)[B]\n service a1(server)[A one] in a\n service a2(server)[A two] in a\n service b1(server)[B one] in b\n service loose(internet)[Loose]\n a1:R -- L:b1\n a2:B -- T:loose\n a1:R -- L:a2\n";
        let diagram = diagram(text);
        let measured: Vec<Measured> =
            diagram.services.iter().map(|service| measure(service, &options())).collect();
        let block = layout(&diagram, None, &measured, &options());
        for (group, frame) in &block.groups {
            for (service, rect) in &block.services {
                let inside = diagram.services[*service].group == Some(*group);
                assert_eq!(
                    frame.contains(rect.centre()),
                    inside,
                    "group {group} and service {service}"
                );
            }
        }
    }

    #[test]
    fn the_documentation_examples_draw() {
        check::drawn(API, &options(), &["API", "Database", "Storage", "Server"]);
        check::drawn(
            "architecture-beta\n    service left_disk(disk)[Disk]\n    service top_disk(disk)[Disk]\n    service bottom_disk(disk)[Disk]\n    service top_gateway(internet)[Gateway]\n    service bottom_gateway(internet)[Gateway]\n    junction junctionCenter\n    junction junctionRight\n\n    left_disk:R -- L:junctionCenter\n    top_disk:B -- T:junctionCenter\n    bottom_disk:T -- B:junctionCenter\n    junctionCenter:R -- L:junctionRight\n    top_gateway:B -- T:junctionRight\n    bottom_gateway:T -- B:junctionRight\n",
            &options(),
            &["Disk", "Gateway"],
        );
    }

    #[test]
    fn three_services_feeding_one_do_not_land_on_top_of_each_other() {
        let text = "architecture-beta\n service db1(database)[DB1]\n service db2(database)[DB2]\n service db3(database)[DB3]\n service mcp(server)[MCP]\n db1:R --> L:mcp\n db2:R --> L:mcp\n db3:R --> L:mcp\n";
        let diagram = diagram(text);
        let measured: Vec<Measured> =
            diagram.services.iter().map(|service| measure(service, &options())).collect();
        let block = layout(&diagram, None, &measured, &options());
        check::no_two_rectangles_overlap(
            &block.services.iter().map(|(_, rect)| *rect).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn a_line_that_is_not_architecture_says_what_one_looks_like() {
        let problem = check::refused("architecture-beta\n service a[A]\n banana\n", &options());
        assert_eq!(problem.line, Some(3));
        assert!(problem.reason.contains("service db(database)[Database]"));
    }
}
