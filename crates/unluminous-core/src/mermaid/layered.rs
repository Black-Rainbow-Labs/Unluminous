//! The layered graph layout, shared by five diagram types.
//!
//! A flowchart, a class diagram, a state diagram, an ER diagram and a requirement diagram are the
//! same problem underneath: boxes joined by labelled arrows, drawn in a direction. So the layout is
//! written once, here, and each of those five is left with only its own grammar and its own shapes
//! to worry about.
//!
//! The method is Sugiyama's, which is what `dagre` does and so what Mermaid's own pictures look
//! like:
//!
//! 1. **Break the cycles.** A depth-first walk; an edge pointing back at a node already on the stack
//!    is reversed for the purpose of layering and remembered, so it is still *drawn* the way it was
//!    written.
//! 2. **Rank.** Longest path down the resulting acyclic graph. A node sits below the lowest of its
//!    parents.
//! 3. **Fill in the gaps.** An edge spanning more than one rank gets a chain of dummy nodes, one a
//!    rank. Those are what it is later routed through, which is what stops a long edge cutting
//!    across a box that happens to be in the way.
//! 4. **Order within a rank**, by repeatedly moving each node to the median position of its
//!    neighbours and keeping the result only when it crosses fewer edges.
//! 5. **Place**, then **route**.
//!
//! ## An edge's label takes room of its own
//!
//! `task-2063` photographed the Mermaid in the task documents and counted five hundred pairs of words
//! drawn over each other, nearly all of them edge labels: two edges between neighbouring nodes each put
//! their label at the middle of their own line, and the two middles were the same place. `dagre` answers
//! this, and so Mermaid's own pictures do not have it: when a diagram has labelled edges, every edge
//! spans twice as many ranks, and a labelled edge's dummy on the middle rank is **the size of its
//! label**. The ordering and the placement then keep labels apart from each other and from every node
//! exactly as they keep nodes apart, because to them a label is a node. The gap between ranks is halved
//! at the same time, so a diagram whose labels are small is no taller than it was.
//!
//! ## Two rules this keeps, and why they matter more here than usual
//!
//! **No randomness, and a fixed number of passes.** Every sweep count in this file is a constant. A
//! layout that improved itself until it stopped improving would give a different picture for the
//! same source depending on how the floating point rounded, and every screenshot test of a diagram
//! would be noise. It also makes the cost O(n) passes whatever the input, which is what lets a
//! preview lay a diagram out on a keystroke.
//!
//! **A subgraph is laid out on its own and placed as one box.** Its contents cannot then overlap
//! anything outside it, which is the failure a single flat layout with a frame drawn round some of
//! the nodes always ends in. An edge that crosses the frame is laid out in two halves that meet on
//! it: inside, from the node to a port on the side of the frame facing the other end, and outside,
//! from that port onwards. See [`place_container`].
//!
//! ## Edges run straight through the ranks and turn between them
//!
//! An edge leaves a box straight out of its side, passes down through each rank in a lane of its
//! own, and changes lane only in the gap between two ranks, where there is nothing to cut across.
//! The corners are rounded by the caller, which is `parts::edge_path`, once the ends have been cut
//! back to the shapes.

use std::collections::HashMap;

use super::scene::{Point, Rect, Size};

/// How far apart two nodes in the same rank are.
const NODE_GAP: f32 = 34.0;
/// How far apart one rank is from the next, before any edge label is allowed for.
const RANK_GAP: f32 = 54.0;
/// How wide a dummy node is: the lane an edge passing a rank takes for itself.
const LANE: f32 = 14.0;
/// Space between a subgraph's frame and what is inside it.
const GROUP_PADDING: f32 = 20.0;
/// How many times the ordering is swept up and down. Four is where the improvement stops being
/// visible on the diagrams in `sample/mermaid`, measured by counting crossings.
const ORDER_SWEEPS: usize = 4;
/// How many times the positions are relaxed towards the median of each node's neighbours.
const POSITION_SWEEPS: usize = 6;
/// The gap between two ranks when the ranks have been doubled to give labels room.
///
/// A little over half the usual gap, so a diagram with labels is no taller than it needs to be, and
/// still enough for an edge to run straight out of a box for an arrowhead's length before it turns
/// (`task-2194`). At exactly half, the turn began under the arrowhead and the head leant.
const LABELLED_GAP: f32 = RANK_GAP * 0.62;
/// The longest an edge runs straight out of a rank before it turns.
const STUB: f32 = 14.0;
/// How much of a box's side the edges leaving it by that side are spread across.
const SIDE_SHARE: f32 = 0.6;
/// The furthest apart two edges leaving one side of a box start.
const SIDE_STEP: f32 = 14.0;

/// Which way the diagram flows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Down,
    Up,
    Right,
    Left,
}

impl Direction {
    /// Read Mermaid's own spelling: `TB`, `TD`, `BT`, `LR`, `RL`.
    pub fn parse(text: &str) -> Option<Direction> {
        match text.trim().to_ascii_uppercase().as_str() {
            "TB" | "TD" | "V" => Some(Direction::Down),
            "BT" => Some(Direction::Up),
            "LR" => Some(Direction::Right),
            "RL" => Some(Direction::Left),
            _ => None,
        }
    }

    /// True when ranks run across the page rather than down it.
    fn is_horizontal(self) -> bool {
        matches!(self, Direction::Right | Direction::Left)
    }
}

/// A node to place. Its identity is its position in [`Graph::nodes`].
#[derive(Debug, Clone, PartialEq)]
pub struct NodeSpec {
    pub size: Size,
    /// The group it is directly inside, if any.
    pub group: Option<usize>,
}

/// An edge to route.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeSpec {
    pub from: usize,
    pub to: usize,
    /// How much room its label needs, so the rank gap can be widened to hold it.
    pub label: Size,
    /// The fewest ranks it must span. Mermaid's extra dashes ask for more than one.
    pub span: usize,
    /// True when `from` is a subgraph rather than a node, for Mermaid's `A --> someSubgraph`.
    pub from_group: bool,
    /// True when `to` is a subgraph rather than a node.
    pub to_group: bool,
}

impl EdgeSpec {
    pub fn new(from: usize, to: usize) -> Self {
        Self { from, to, label: Size::default(), span: 1, from_group: false, to_group: false }
    }
}

/// A subgraph: a frame drawn round some nodes, with a title.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupSpec {
    /// How much room the title takes, which is added to the top of the frame.
    pub title: Size,
    /// The group this one is directly inside, if any.
    pub parent: Option<usize>,
}

/// Everything to be laid out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Graph {
    pub nodes: Vec<NodeSpec>,
    pub edges: Vec<EdgeSpec>,
    pub groups: Vec<GroupSpec>,
    pub direction: Direction,
}

impl Graph {
    pub fn add_node(&mut self, size: Size, group: Option<usize>) -> usize {
        self.nodes.push(NodeSpec { size, group });
        self.nodes.len() - 1
    }
}

/// Where everything went.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Placed {
    /// One rectangle per node, in the order they were given.
    pub nodes: Vec<Rect>,
    /// One frame per group, in the order they were given.
    pub groups: Vec<Rect>,
    /// One polyline per edge, from a point inside its source to a point inside its target, through
    /// whatever bends the layout gave it. Each end is at the middle height of a node, or on the frame
    /// when the end is a subgraph. Cutting each end back to the shape's own border is the caller's,
    /// because only the caller knows what shape it drew, and so is rounding the corners.
    pub edges: Vec<Vec<Point>>,
    /// Where each edge's label goes: the middle of its middle segment.
    pub labels: Vec<Point>,
    pub size: Size,
}

/// Lay `graph` out.
pub fn layout(graph: &Graph) -> Placed {
    if graph.nodes.is_empty() {
        return Placed::default();
    }
    // Everything is worked out as though the diagram ran downwards, and turned at the end. For a
    // left-to-right diagram the node sizes go in turned as well, because in that layout a node's
    // height is what takes room along a rank; turning the result back restores both.
    let turned = graph.direction.is_horizontal();
    let reach = Reach::of(graph, turned);
    let mut placed = place_container(graph, None, turned, &reach).placed;
    if turned {
        transpose(&mut placed);
    }
    match graph.direction {
        Direction::Up => flip_vertically(&mut placed),
        Direction::Left => flip_horizontally(&mut placed),
        _ => {}
    }
    placed
}

/// Which container each edge's middle is laid out in, and which way it runs there.
///
/// Worked out before anything is placed, because a subgraph is laid out before the container round it
/// and has to know already which of its edges leave by the top of its frame and which by the bottom.
/// Both questions are about the shape of the graph rather than about sizes, so they can be answered
/// first: the container is the deepest one in which the two ends are in different entities, and the
/// direction is whether the cycle removal in that container turned the edge round.
struct Reach {
    /// The container an edge is laid out in, or nothing for an edge with no route at all: a node
    /// pointing at itself, or a subgraph pointing at something inside itself.
    home: Vec<Option<Option<usize>>>,
    /// True for an edge drawn against the way the diagram runs: up the page in a downward diagram.
    upward: Vec<bool>,
}

impl Reach {
    fn of(graph: &Graph, turned: bool) -> Reach {
        let mut reach =
            Reach { home: vec![None; graph.edges.len()], upward: vec![false; graph.edges.len()] };
        let containers = std::iter::once(None).chain((0..graph.groups.len()).map(Some));
        for container in containers {
            let members = Members::of(graph, container);
            let lifted = lift_edges(graph, &members, turned);
            let reversed = back_edges(members.count(), &lifted);
            for (index, edge) in lifted.iter().enumerate() {
                reach.home[edge.edge] = Some(container);
                reach.upward[edge.edge] = reversed[index];
            }
        }
        reach
    }
}

/// The things one container places, and which of them each node and each subgraph is inside.
struct Members {
    /// The nodes directly inside it, which are its first entities.
    nodes: Vec<usize>,
    /// The subgraphs directly inside it, which are its next entities, one box each.
    children: Vec<usize>,
    /// For every node, the entity it is or is inside, or nothing when it is elsewhere.
    node_owner: Vec<Option<usize>>,
    /// For every subgraph, the entity it is or is inside, or nothing when it is elsewhere.
    group_owner: Vec<Option<usize>>,
}

impl Members {
    fn of(graph: &Graph, group: Option<usize>) -> Members {
        let nodes: Vec<usize> =
            (0..graph.nodes.len()).filter(|&index| graph.nodes[index].group == group).collect();
        let children: Vec<usize> =
            (0..graph.groups.len()).filter(|&index| graph.groups[index].parent == group).collect();
        let node_owner = ownership(graph, &nodes, &children);
        let mut group_owner = vec![None; graph.groups.len()];
        for (position, &child) in children.iter().enumerate() {
            for (index, owner) in group_owner.iter_mut().enumerate() {
                if inside(graph, Some(index), child) {
                    *owner = Some(nodes.len() + position);
                }
            }
        }
        Members { nodes, children, node_owner, group_owner }
    }

    fn count(&self) -> usize {
        self.nodes.len() + self.children.len()
    }

    /// The entity one end of an edge is, or is inside.
    fn entity_of(&self, end: usize, is_group: bool) -> Option<usize> {
        match is_group {
            true => self.group_owner.get(end).copied().flatten(),
            false => self.node_owner.get(end).copied().flatten(),
        }
    }
}

/// What one container hands the container round it.
struct Laid {
    /// Its nodes, its subgraphs and every edge that starts and ends inside it, in its own coordinates.
    placed: Placed,
    /// The part inside it of every edge that leaves it, from the node to where the edge crosses the
    /// frame, keyed by the edge.
    partials: HashMap<usize, Partial>,
}

/// The part of an edge inside one subgraph.
struct Partial {
    /// In the direction the edge is drawn, so it is joined to the rest of the edge as it stands.
    points: Vec<Point>,
    /// True when the edge's source is inside and the crossing is the last point; false when the
    /// target is inside and the crossing is the first.
    from_inside: bool,
}

impl Partial {
    /// Where the edge crosses the frame.
    fn crossing(&self) -> Point {
        let point = if self.from_inside { self.points.last() } else { self.points.first() };
        point.copied().unwrap_or_default()
    }
}

/// One box of the layout: the top level, or the inside of one subgraph.
///
/// Positions are relative to the box's own top left corner. A child group is laid out by calling
/// this again and is then placed inside as a single node, which is what keeps its contents from ever
/// overlapping anything outside it.
///
/// **An edge that leaves a subgraph leaves it through a crossing of its own** (`task-2194`). Before,
/// a subgraph's layout knew nothing about the edges reaching out of it, and the line was joined up
/// afterwards by running round the outside of the frame, which put long lines along the frames and
/// through their titles. Now each such edge is given a **port**: an entity of no height in a rank of
/// its own along the top or the bottom of the subgraph, joined to the node the edge belongs to. The
/// ordering and the placement keep it out of the way of everything else, exactly as they do a dummy,
/// so inside the frame the edge has a lane of its own to the node. Where the port ends up is where the
/// container round it picks the edge up.
fn place_container(graph: &Graph, group: Option<usize>, turned: bool, reach: &Reach) -> Laid {
    let members = Members::of(graph, group);
    let inner: Vec<Laid> = members
        .children
        .iter()
        .map(|&child| place_container(graph, Some(child), turned, reach))
        .collect();
    let titles: Vec<Size> =
        members.children.iter().map(|&child| turn(graph.groups[child].title, turned)).collect();

    let mut sizes: Vec<Size> =
        members.nodes.iter().map(|&index| turn(graph.nodes[index].size, turned)).collect();
    let mut kinds: Vec<Kind> = vec![Kind::Node; members.nodes.len()];
    for (position, laid) in inner.iter().enumerate() {
        sizes.push(frame_size(laid.placed.size, titles[position]));
        kinds.push(Kind::Group);
    }

    let mut lifted = lift_edges(graph, &members, turned);
    let mut reversed: Vec<bool> = lifted.iter().map(|edge| reach.upward[edge.edge]).collect();
    add_ports(graph, group, &members, reach, &mut sizes, &mut kinds, &mut lifted, &mut reversed);

    // Where every edge already laid out inside a child crosses that child's frame, measured from the
    // frame's left edge, so the route out here starts exactly where the one in there stopped.
    let mut anchors: Anchors = HashMap::new();
    for (position, laid) in inner.iter().enumerate() {
        for (&edge, partial) in &laid.partials {
            anchors.insert(
                (edge, members.nodes.len() + position),
                partial.crossing().x + GROUP_PADDING,
            );
        }
    }
    let title = group.map_or(Size::default(), |group| turn(graph.groups[group].title, turned));
    let boundary = Boundary { top: -(GROUP_PADDING + title.height), bottom_margin: GROUP_PADDING };
    let arranged = arrange(&sizes, &kinds, &lifted, &reversed, &anchors, boundary);

    let mut laid = Laid {
        placed: Placed {
            nodes: vec![Rect::default(); graph.nodes.len()],
            groups: vec![Rect::default(); graph.groups.len()],
            edges: vec![Vec::new(); graph.edges.len()],
            labels: vec![Point::default(); graph.edges.len()],
            size: arranged.size,
        },
        partials: HashMap::new(),
    };
    for (position, &index) in members.nodes.iter().enumerate() {
        laid.placed.nodes[index] = arranged.entities[position];
    }
    let mut moved_partials: HashMap<(usize, usize), Vec<Point>> = HashMap::new();
    for (position, &child) in members.children.iter().enumerate() {
        let frame = arranged.entities[members.nodes.len() + position];
        laid.placed.groups[child] = frame;
        let (dx, dy) = (frame.x + GROUP_PADDING, frame.y + GROUP_PADDING + titles[position].height);
        merge(&mut laid.placed, &inner[position].placed, dx, dy);
        for (&edge, partial) in &inner[position].partials {
            let points = partial.points.iter().map(|p| Point::new(p.x + dx, p.y + dy)).collect();
            moved_partials.insert((edge, members.nodes.len() + position), points);
        }
    }
    for (index, path) in arranged.paths.into_iter().enumerate() {
        let edge = &lifted[index];
        let before = moved_partials.remove(&(edge.edge, edge.from)).unwrap_or_default();
        let after = moved_partials.remove(&(edge.edge, edge.to)).unwrap_or_default();
        let points = join(&join(&before, &path.points), &after);
        match edge.port {
            None => {
                laid.placed.edges[edge.edge] = points;
                laid.placed.labels[edge.edge] = path.label;
            }
            Some(from_inside) => {
                laid.partials.insert(edge.edge, Partial { points, from_inside });
            }
        }
    }
    laid
}

/// Give every edge that leaves this container a port, on the side of the frame facing its other end.
#[allow(clippy::too_many_arguments)]
fn add_ports(
    graph: &Graph,
    group: Option<usize>,
    members: &Members,
    reach: &Reach,
    sizes: &mut Vec<Size>,
    kinds: &mut Vec<Kind>,
    lifted: &mut Vec<Lifted>,
    reversed: &mut Vec<bool>,
) {
    for (edge, spec) in graph.edges.iter().enumerate() {
        let Some(home) = reach.home[edge] else { continue };
        if home == group {
            continue;
        }
        let from = members.entity_of(spec.from, spec.from_group);
        let to = members.entity_of(spec.to, spec.to_group);
        let (inside, from_inside) = match (from, to) {
            (Some(entity), None) => (entity, true),
            (None, Some(entity)) => (entity, false),
            _ => continue,
        };
        // The edge runs down the page in the container it is laid out in unless that container
        // turned it round, so its other end is below this subgraph exactly when it leaves from here
        // going down or arrives here from above.
        let down = !reach.upward[edge];
        let top = if from_inside { !down } else { down };
        let port = sizes.len();
        sizes.push(Size::new(LANE, 0.0));
        kinds.push(Kind::Port { top });
        // In the direction it is drawn, and turned round for the layout when that runs upwards.
        let (from, to) = if from_inside { (inside, port) } else { (port, inside) };
        lifted.push(Lifted {
            edge,
            from,
            to,
            label: Size::default(),
            span: 1,
            port: Some(from_inside),
        });
        reversed.push(if from_inside { top } else { !top });
    }
}

/// Join two runs of points end to end, dropping the point where they meet from the second.
fn join(first: &[Point], second: &[Point]) -> Vec<Point> {
    let mut out = first.to_vec();
    let skip = match (first.last(), second.first()) {
        (Some(end), Some(start)) => usize::from(end.distance(*start) < 0.01),
        _ => 0,
    };
    out.extend_from_slice(&second[skip.min(second.len())..]);
    out
}

/// A size with its two numbers swapped when the diagram has been turned on its side.
fn turn(size: Size, turned: bool) -> Size {
    if turned {
        Size::new(size.height, size.width)
    } else {
        size
    }
}

/// How big a subgraph's frame is round contents of `inner`, with a title of `title`.
fn frame_size(inner: Size, title: Size) -> Size {
    Size::new(
        (inner.width + GROUP_PADDING * 2.0).max(title.width + GROUP_PADDING * 2.0),
        inner.height + GROUP_PADDING * 2.0 + title.height,
    )
}

/// Copy everything `child` placed into `into`, moved by `dx` and `dy`.
fn merge(into: &mut Placed, child: &Placed, dx: f32, dy: f32) {
    for (index, rect) in child.nodes.iter().enumerate() {
        if rect.width > 0.0 || rect.height > 0.0 {
            into.nodes[index] = rect.moved(dx, dy);
        }
    }
    for (index, rect) in child.groups.iter().enumerate() {
        if rect.width > 0.0 || rect.height > 0.0 {
            into.groups[index] = rect.moved(dx, dy);
        }
    }
    for (index, path) in child.edges.iter().enumerate() {
        if !path.is_empty() {
            into.edges[index] =
                path.iter().map(|point| Point::new(point.x + dx, point.y + dy)).collect();
            into.labels[index] = Point::new(child.labels[index].x + dx, child.labels[index].y + dy);
        }
    }
}

/// For every node, which entity of this container it belongs to, or nothing when it is elsewhere.
///
/// A node directly in this container is its own entity. A node inside one of this container's child
/// groups — however deeply — is that child group. That is what lets an edge reaching into a subgraph
/// be laid out here as an edge to the subgraph, and routed later to the real node.
fn ownership(graph: &Graph, nodes: &[usize], children: &[usize]) -> Vec<Option<usize>> {
    let mut owner = vec![None; graph.nodes.len()];
    for (position, &index) in nodes.iter().enumerate() {
        owner[index] = Some(position);
    }
    for (position, &child) in children.iter().enumerate() {
        for (index, node) in graph.nodes.iter().enumerate() {
            if inside(graph, node.group, child) {
                owner[index] = Some(nodes.len() + position);
            }
        }
    }
    owner
}

/// True when `start` is `wanted` or is nested inside it.
fn inside(graph: &Graph, start: Option<usize>, wanted: usize) -> bool {
    let mut at = start;
    // Bounded by the number of groups, so a manifest with a cycle in its nesting cannot loop here.
    for _ in 0..=graph.groups.len() {
        match at {
            Some(index) if index == wanted => return true,
            Some(index) => at = graph.groups[index].parent,
            None => return false,
        }
    }
    false
}

/// What an entity of one container is, which decides where an edge touching it starts and stops.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// A node: an edge starts at a point inside it and the caller cuts it back to the shape.
    Node,
    /// A subgraph's frame: an edge starts on the frame, where the layout inside it left off.
    Group,
    /// Where an edge leaves this container, on its top or its bottom.
    Port { top: bool },
}

/// Where an edge laid out inside a child crosses the child's frame, by the edge and the child's
/// entity, measured from the frame's left edge.
type Anchors = HashMap<(usize, usize), f32>;

/// Where this container's own frame is, in its own coordinates, which is where its ports go.
#[derive(Debug, Clone, Copy)]
struct Boundary {
    /// The top of the frame, above the contents by the padding and the title.
    top: f32,
    /// How far below the contents the bottom of the frame is.
    bottom_margin: f32,
}

/// One edge as this container sees it: between two of its entities, remembering which edge it was.
#[derive(Debug, Clone, Copy)]
struct Lifted {
    edge: usize,
    from: usize,
    to: usize,
    label: Size,
    span: usize,
    /// For the part of an edge between a node and the port it leaves this container by: whether the
    /// edge's source is the end inside. Nothing for an edge laid out here whole.
    port: Option<bool>,
}

/// Every edge whose two ends are different entities of this container.
///
/// An edge inside one child group is left for that group's own layout; an edge to somewhere outside
/// this container is given a port by [`add_ports`] instead.
fn lift_edges(graph: &Graph, members: &Members, turned: bool) -> Vec<Lifted> {
    graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(edge, spec)| {
            let from = members.entity_of(spec.from, spec.from_group)?;
            let to = members.entity_of(spec.to, spec.to_group)?;
            (from != to).then_some(Lifted {
                edge,
                from,
                to,
                // Turned with the node sizes, so the gap between two ranks holds the label the way
                // round it is really drawn. A left-to-right diagram's labels are laid out along the
                // gap between two columns, and it is their **width** that has to fit there; leaving
                // this untouched left every label on a left-to-right diagram sitting under the box
                // beside it, which is what the state diagram's picture showed.
                label: turn(spec.label, turned),
                span: spec.span.max(1),
                port: None,
            })
        })
        .collect()
}

/// An edge's route, in the container's own coordinates.
struct Path {
    points: Vec<Point>,
    label: Point,
}

/// What one container's own arrangement came to.
struct Arranged {
    entities: Vec<Rect>,
    /// One route for every lifted edge, in the same order.
    paths: Vec<Path>,
    size: Size,
}

/// Rank, order, place and route one container's entities. Everything below here is plain Sugiyama.
fn arrange(
    sizes: &[Size],
    kinds: &[Kind],
    edges: &[Lifted],
    reversed: &[bool],
    anchors: &Anchors,
    boundary: Boundary,
) -> Arranged {
    if sizes.is_empty() {
        return Arranged { entities: Vec::new(), paths: Vec::new(), size: Size::default() };
    }
    // Twice the ranks when anything carries a label, so each label has a rank of its own to sit in.
    // See the module comment.
    let labelled = edges.iter().any(has_a_label);
    let doubled: Vec<Lifted>;
    let edges: &[Lifted] = match labelled {
        true => {
            doubled = edges.iter().map(|edge| Lifted { span: edge.span * 2, ..*edge }).collect();
            &doubled
        }
        false => edges,
    };
    let ranks = rank_with_ports(kinds, edges, reversed);
    let (layers, chains) = insert_dummies(sizes, edges, reversed, &ranks);
    let labels = label_slots(edges, &chains);
    let joins = build_joins(&chains, edges, reversed);
    let order = order_layers(&layers, &joins);
    let placed = position(sizes, kinds, &order, &joins, edges, &labels, labelled);
    let gap = if labelled { LABELLED_GAP } else { rank_gap(edges) };
    let routing = Routing { kinds, anchors, boundary, stub: (gap * 0.35).min(STUB) };
    route(edges, reversed, &chains, &placed, &labels, &routing)
}

/// Rank every entity, with the ports in ranks of their own along the top and the bottom.
///
/// The ports are left out of the ranking itself: one joined to a node would otherwise push that node
/// a rank further down than its neighbours, and the picture inside the frame would change because of
/// where its edges go next.
fn rank_with_ports(kinds: &[Kind], edges: &[Lifted], reversed: &[bool]) -> Vec<usize> {
    let own: Vec<(Lifted, bool)> = edges
        .iter()
        .zip(reversed)
        .filter(|(edge, _)| edge.port.is_none())
        .map(|(edge, reversed)| (*edge, *reversed))
        .collect();
    let (own_edges, own_reversed): (Vec<Lifted>, Vec<bool>) = own.into_iter().unzip();
    let mut ranks = rank(kinds.len(), &own_edges, &own_reversed);
    let is_port = |kind: &Kind| matches!(kind, Kind::Port { .. });
    let shift = usize::from(kinds.iter().any(|kind| matches!(kind, Kind::Port { top: true })));
    let deepest = (0..kinds.len()).filter(|&at| !is_port(&kinds[at])).map(|at| ranks[at]).max();
    let deepest = deepest.unwrap_or(0) + shift;
    for (at, kind) in kinds.iter().enumerate() {
        ranks[at] = match kind {
            Kind::Port { top: true } => 0,
            Kind::Port { top: false } => deepest + 1,
            _ => ranks[at] + shift,
        };
    }
    ranks
}

/// Whether an edge has a label that takes any room.
fn has_a_label(edge: &Lifted) -> bool {
    edge.label.width > 0.0 && edge.label.height > 0.0
}

/// Where each labelled edge's label sits: the dummy on the middle rank of its chain, and how much room
/// that dummy takes.
///
/// Keyed like a slot, so the placement asks one map. The room is the label's panel, which
/// `flowchart::draw_link_label` draws eight points wider and two taller than the words, and a little
/// more so two panels in one rank do not touch.
type LabelSlots = HashMap<SlotKey, Size>;

fn label_slots(edges: &[Lifted], chains: &Chains) -> LabelSlots {
    edges
        .iter()
        .enumerate()
        .filter(|(index, edge)| has_a_label(edge) && !chains[*index].is_empty())
        .map(|(index, edge)| {
            let chain = &chains[index];
            let rank = chain[chain.len() / 2];
            (
                key(Slot::Dummy(index), rank),
                Size::new(edge.label.width + 12.0, edge.label.height + 4.0),
            )
        })
        .collect()
}

/// Which edges point backwards, found by a depth-first walk.
///
/// An edge to a node already on the stack closes a cycle. It is reversed for ranking so the graph
/// becomes acyclic, and remembered so that it is still drawn pointing the way it was written.
fn back_edges(count: usize, edges: &[Lifted]) -> Vec<bool> {
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (index, edge) in edges.iter().enumerate() {
        out[edge.from].push(index);
    }
    let mut reversed = vec![false; edges.len()];
    // 0 not seen, 1 on the stack, 2 finished.
    let mut state = vec![0_u8; count];
    for start in 0..count {
        if state[start] != 0 {
            continue;
        }
        // An explicit stack rather than recursion: a chain of ten thousand nodes is a legal
        // flowchart and would otherwise be ten thousand stack frames.
        let mut stack = vec![(start, 0_usize)];
        state[start] = 1;
        while let Some((node, position)) = stack.pop() {
            if position < out[node].len() {
                stack.push((node, position + 1));
                let index = out[node][position];
                let next = edges[index].to;
                match state[next] {
                    0 => {
                        state[next] = 1;
                        stack.push((next, 0));
                    }
                    1 => reversed[index] = true,
                    _ => {}
                }
            } else {
                state[node] = 2;
            }
        }
    }
    reversed
}

/// The two ends of an edge, the way the layout should read them.
fn ends(edge: &Lifted, reversed: bool) -> (usize, usize) {
    if reversed {
        (edge.to, edge.from)
    } else {
        (edge.from, edge.to)
    }
}

/// How far down each entity sits, as a whole number of ranks.
///
/// Longest path: a node goes one rank below the lowest thing pointing at it. Walked in topological
/// order, which the cycle removal has made possible.
fn rank(count: usize, edges: &[Lifted], reversed: &[bool]) -> Vec<usize> {
    let mut incoming = vec![0_usize; count];
    let mut out: Vec<Vec<(usize, usize)>> = vec![Vec::new(); count];
    for (index, edge) in edges.iter().enumerate() {
        let (from, to) = ends(edge, reversed[index]);
        incoming[to] += 1;
        out[from].push((to, edge.span.max(1)));
    }
    let mut ranks = vec![0_usize; count];
    let mut ready: Vec<usize> = (0..count).filter(|&node| incoming[node] == 0).collect();
    let mut done = 0;
    while let Some(node) = ready.pop() {
        done += 1;
        for &(next, span) in &out[node] {
            ranks[next] = ranks[next].max(ranks[node] + span);
            incoming[next] -= 1;
            if incoming[next] == 0 {
                ready.push(next);
            }
        }
    }
    // Every node should have come off the queue. If one has not, the cycle removal missed something
    // and the ranks it has are still usable, so the diagram is drawn rather than refused.
    debug_assert!(done == count || count == 0, "the graph should be acyclic by now");
    ranks
}

/// A node in the layered graph: either one of the container's entities, or a dummy on an edge.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Slot {
    Entity(usize),
    /// A point an edge passes through, and which edge it belongs to.
    Dummy(usize),
}

/// The chain of dummies belonging to one edge, from its source's rank downwards.
type Chains = Vec<Vec<usize>>;

/// Split every long edge into a chain of dummies, and hand back the nodes rank by rank.
///
/// The returned `layers[r]` holds every slot at rank `r`. `chains[e]` holds the dummy slot numbers
/// of edge `e`, in rank order, which is what it is routed through afterwards.
fn insert_dummies(
    sizes: &[Size],
    edges: &[Lifted],
    reversed: &[bool],
    ranks: &[usize],
) -> (Vec<Vec<Slot>>, Chains) {
    let depth = ranks.iter().copied().max().unwrap_or(0) + 1;
    let mut layers: Vec<Vec<Slot>> = vec![Vec::new(); depth];
    for entity in 0..sizes.len() {
        layers[ranks[entity]].push(Slot::Entity(entity));
    }
    let mut chains: Chains = vec![Vec::new(); edges.len()];
    for (index, edge) in edges.iter().enumerate() {
        let (from, to) = ends(edge, reversed[index]);
        let (top, bottom) = (ranks[from], ranks[to]);
        for (level, layer) in layers.iter_mut().enumerate().take(bottom).skip(top + 1) {
            layer.push(Slot::Dummy(index));
            chains[index].push(level);
        }
    }
    (layers, chains)
}

/// The order of the slots within each rank, after the crossing reduction.
///
/// The starting order is the order things were declared in, which is deterministic and is usually
/// close to what the author meant. It is then swept down and up a fixed number of times, moving each
/// slot to the median position of what it is joined to in the neighbouring rank, and each sweep is
/// kept only if it crosses fewer edges than the best so far.
fn order_layers(layers: &[Vec<Slot>], joins: &Joins) -> Vec<Vec<Slot>> {
    let mut best = layers.to_vec();
    let mut best_crossings = crossings(&best, joins);
    let mut current = best.clone();
    for _ in 0..ORDER_SWEEPS {
        for pass in 0..2 {
            median_pass(&mut current, joins, pass == 0);
            transpose_pass(&mut current, joins);
            let count = crossings(&current, joins);
            if count < best_crossings {
                best_crossings = count;
                best = current.clone();
            }
        }
    }
    best
}

/// For each slot, which slots it is joined to in the rank above and in the rank below.
///
/// Keyed by the slot itself rather than by position, because positions move and this does not.
type Joins = HashMap<SlotKey, (Vec<SlotKey>, Vec<SlotKey>)>;

/// A slot, as something that can go in a map.
type SlotKey = (u8, usize, usize);

fn key(slot: Slot, rank: usize) -> SlotKey {
    match slot {
        Slot::Entity(index) => (0, index, 0),
        Slot::Dummy(edge) => (1, edge, rank),
    }
}

/// Work out what is joined to what, once, so the sweeps do not keep rediscovering it.
///
/// Every edge is a **run** of links: the entity it starts at, its dummies in rank order, then the
/// entity it ends at. Walking the run and joining each link to the next is the whole of it, and it
/// does not matter whether a link is an entity or a dummy. The run is in rank order rather than in
/// drawing order, which is why a reversed edge is read through [`ends`] first: the ordering and the
/// positioning both work down the page, and only the arrowhead cares which way it was written.
fn build_joins(chains: &Chains, edges: &[Lifted], reversed: &[bool]) -> Joins {
    let mut joins: Joins = HashMap::new();
    for (index, edge) in edges.iter().enumerate() {
        let (from, to) = ends(edge, reversed[index]);
        let mut run: Vec<SlotKey> = Vec::with_capacity(chains[index].len() + 2);
        run.push((0, from, 0));
        for &rank in &chains[index] {
            run.push((1, index, rank));
        }
        run.push((0, to, 0));
        for pair in run.windows(2) {
            joins.entry(pair[0]).or_default().1.push(pair[1]);
            joins.entry(pair[1]).or_default().0.push(pair[0]);
        }
    }
    joins
}

/// Move each slot to the median position of what it is joined to in the neighbouring rank.
fn median_pass(layers: &mut [Vec<Slot>], joins: &Joins, downwards: bool) {
    let count = layers.len();
    let order: Vec<usize> =
        if downwards { (1..count).collect() } else { (0..count - 1).rev().collect() };
    for rank in order {
        let neighbour = if downwards { rank - 1 } else { rank + 1 };
        let positions: HashMap<SlotKey, usize> = layers[neighbour]
            .iter()
            .enumerate()
            .map(|(at, slot)| (key(*slot, neighbour), at))
            .collect();
        let mut scored: Vec<(f32, usize, Slot)> = layers[rank]
            .iter()
            .enumerate()
            .map(|(at, slot)| {
                let median =
                    median_of(key(*slot, rank), joins, &positions, downwards).unwrap_or(at as f32);
                (median, at, *slot)
            })
            .collect();
        // Ties keep the order they had, which is what makes this deterministic.
        scored.sort_by(|a, b| {
            a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1))
        });
        layers[rank] = scored.into_iter().map(|(_, _, slot)| slot).collect();
    }
}

/// The median position of everything one slot is joined to in the neighbouring rank.
fn median_of(
    slot: SlotKey,
    joins: &Joins,
    positions: &HashMap<SlotKey, usize>,
    downwards: bool,
) -> Option<f32> {
    let (above, below) = joins.get(&slot)?;
    let neighbours = if downwards { above } else { below };
    let mut found: Vec<usize> =
        neighbours.iter().filter_map(|other| positions.get(other).copied()).collect();
    if found.is_empty() {
        return None;
    }
    found.sort_unstable();
    let middle = found.len() / 2;
    if found.len() % 2 == 1 {
        Some(found[middle] as f32)
    } else {
        Some((found[middle - 1] + found[middle]) as f32 / 2.0)
    }
}

/// Swap neighbouring pairs where doing so crosses fewer edges. One pass, so it always ends.
fn transpose_pass(layers: &mut [Vec<Slot>], joins: &Joins) {
    for rank in 0..layers.len() {
        let mut at = 0;
        while at + 1 < layers[rank].len() {
            let before = crossings_at(layers, joins, rank);
            layers[rank].swap(at, at + 1);
            if crossings_at(layers, joins, rank) >= before {
                layers[rank].swap(at, at + 1);
            }
            at += 1;
        }
    }
}

/// How many edges cross between this rank and the one above it, and the one below it.
fn crossings_at(layers: &[Vec<Slot>], joins: &Joins, rank: usize) -> usize {
    let mut total = 0;
    if rank > 0 {
        total += crossings_between(layers, joins, rank - 1, rank);
    }
    if rank + 1 < layers.len() {
        total += crossings_between(layers, joins, rank, rank + 1);
    }
    total
}

/// How many edges cross in the whole drawing.
fn crossings(layers: &[Vec<Slot>], joins: &Joins) -> usize {
    (1..layers.len()).map(|rank| crossings_between(layers, joins, rank - 1, rank)).sum()
}

/// Count the crossings between two neighbouring ranks, by counting inversions.
fn crossings_between(layers: &[Vec<Slot>], joins: &Joins, upper: usize, lower: usize) -> usize {
    let lower_at: HashMap<SlotKey, usize> =
        layers[lower].iter().enumerate().map(|(at, slot)| (key(*slot, lower), at)).collect();
    let mut ends: Vec<usize> = Vec::new();
    for slot in &layers[upper] {
        let Some((_, below)) = joins.get(&key(*slot, upper)) else {
            continue;
        };
        let mut here: Vec<usize> =
            below.iter().filter_map(|other| lower_at.get(other).copied()).collect();
        here.sort_unstable();
        ends.extend(here);
    }
    let mut total = 0;
    for first in 0..ends.len() {
        for second in first + 1..ends.len() {
            if ends[first] > ends[second] {
                total += 1;
            }
        }
    }
    total
}

/// Where every slot ended up.
struct Positions {
    /// The centre of each slot, by rank and position within the rank.
    centres: Vec<Vec<Point>>,
    /// How tall each rank is, so an edge knows where the band it passes through starts and stops.
    heights: Vec<f32>,
    layers: Vec<Vec<Slot>>,
    entities: Vec<Rect>,
    size: Size,
}

/// Give every slot a position: down the page by rank, across it by relaxation towards its
/// neighbours.
///
/// A rank holding nothing but ports takes no room: the ports along the top are put on the top edge of
/// the contents and the ones along the bottom on the bottom edge, and the container moves them out to
/// its frame afterwards. Otherwise every subgraph with an edge leaving it would be a rank gap taller
/// than its contents for no reason anybody could see.
fn position(
    sizes: &[Size],
    kinds: &[Kind],
    order: &[Vec<Slot>],
    joins: &Joins,
    edges: &[Lifted],
    labels: &LabelSlots,
    labelled: bool,
) -> Positions {
    let widths = slot_widths(sizes, order, labels);
    let mut across = initial_across(&widths);
    for sweep in 0..POSITION_SWEEPS {
        relax(&mut across, &widths, joins, order, sweep % 2 == 0);
    }
    normalise(&mut across, &widths);

    let heights = rank_heights(sizes, order, labels);
    // Half the gap between twice the ranks, so the distance from one node to the next is what it was
    // and a label's own height is what is added to it.
    let gap = match labelled {
        true => LABELLED_GAP,
        false => rank_gap(edges),
    };
    let only_ports = |layer: &Vec<Slot>| {
        !layer.is_empty()
            && layer.iter().all(|slot| {
                matches!(slot, Slot::Entity(index) if matches!(kinds[*index], Kind::Port { .. }))
            })
    };
    let mut centres: Vec<Vec<Point>> = Vec::with_capacity(order.len());
    let mut top = 0.0;
    let mut bottom = 0.0_f32;
    for (rank, layer) in order.iter().enumerate() {
        let height = heights[rank];
        let y = match only_ports(layer) {
            true if rank == 0 => 0.0,
            true => bottom,
            false => top + height / 2.0,
        };
        let row: Vec<Point> =
            layer.iter().enumerate().map(|(at, _)| Point::new(across[rank][at], y)).collect();
        centres.push(row);
        if !only_ports(layer) {
            bottom = top + height;
            top += height + gap;
        }
    }
    let mut entities = vec![Rect::default(); sizes.len()];
    for (rank, layer) in order.iter().enumerate() {
        for (at, slot) in layer.iter().enumerate() {
            if let Slot::Entity(index) = slot {
                entities[*index] = Rect::around(centres[rank][at], sizes[*index]);
            }
        }
    }
    let mut size = extent(&entities, &centres);
    for (rank, layer) in order.iter().enumerate() {
        for (at, slot) in layer.iter().enumerate() {
            if let Some(room) = labels.get(&key(*slot, rank)) {
                let panel = Rect::around(centres[rank][at], *room);
                size.width = size.width.max(panel.right());
                size.height = size.height.max(panel.bottom());
            }
        }
    }
    Positions { centres, heights, layers: order.to_vec(), entities, size }
}

/// How wide every slot is, rank by rank.
fn slot_widths(sizes: &[Size], order: &[Vec<Slot>], labels: &LabelSlots) -> Vec<Vec<f32>> {
    order
        .iter()
        .enumerate()
        .map(|(rank, layer)| {
            layer
                .iter()
                .map(|slot| match slot {
                    Slot::Entity(index) => sizes[*index].width,
                    Slot::Dummy(_) => labels.get(&key(*slot, rank)).map_or(LANE, |room| room.width),
                })
                .collect()
        })
        .collect()
}

/// Pack every rank left to right, which is the starting point the relaxation improves on.
fn initial_across(widths: &[Vec<f32>]) -> Vec<Vec<f32>> {
    widths
        .iter()
        .map(|row| {
            let mut left = 0.0;
            row.iter()
                .map(|width| {
                    let centre = left + width / 2.0;
                    left += width + NODE_GAP;
                    centre
                })
                .collect()
        })
        .collect()
}

/// Move each slot towards the median of its neighbours, then push everything apart again.
///
/// The pushing apart is what makes this safe: whatever the medians asked for, no two slots in a rank
/// end up closer than [`NODE_GAP`], so the "no two nodes overlap" property every diagram type is
/// tested for holds by construction rather than by luck.
fn relax(
    across: &mut [Vec<f32>],
    widths: &[Vec<f32>],
    joins: &Joins,
    order: &[Vec<Slot>],
    downwards: bool,
) {
    let count = order.len();
    let ranks: Vec<usize> =
        if downwards { (0..count).collect() } else { (0..count).rev().collect() };
    for rank in ranks {
        let neighbour =
            if downwards { rank.checked_sub(1) } else { (rank + 1 < count).then_some(rank + 1) };
        let Some(neighbour) = neighbour else {
            continue;
        };
        let positions: HashMap<SlotKey, f32> = order[neighbour]
            .iter()
            .enumerate()
            .map(|(at, slot)| (key(*slot, neighbour), across[neighbour][at]))
            .collect();
        let wanted: Vec<f32> = order[rank]
            .iter()
            .enumerate()
            .map(|(at, slot)| {
                average_of(key(*slot, rank), joins, &positions, downwards)
                    .unwrap_or(across[rank][at])
            })
            .collect();
        across[rank] = separate(&wanted, &widths[rank]);
    }
}

/// The average position of everything a slot is joined to in the neighbouring rank.
///
/// The average rather than the median, because this one is about where a node should sit rather than
/// about what order it should be in, and a node with two parents belongs between them.
fn average_of(
    slot: SlotKey,
    joins: &Joins,
    positions: &HashMap<SlotKey, f32>,
    downwards: bool,
) -> Option<f32> {
    let (above, below) = joins.get(&slot)?;
    let neighbours = if downwards { above } else { below };
    let found: Vec<f32> =
        neighbours.iter().filter_map(|other| positions.get(other).copied()).collect();
    if found.is_empty() {
        return None;
    }
    Some(found.iter().sum::<f32>() / found.len() as f32)
}

/// Push a rank apart so nothing overlaps, staying as near to `wanted` as the widths allow.
///
/// Twice: once from the left, which guarantees the separation, and once from the right, which takes
/// up the slack the first pass left when the crowding was at the left hand end. Averaging the two
/// and separating once more keeps the result valid and centred.
fn separate(wanted: &[f32], widths: &[f32]) -> Vec<f32> {
    let left = pack(wanted, widths, true);
    let right = pack(wanted, widths, false);
    let middle: Vec<f32> =
        left.iter().zip(&right).map(|(first, second)| (first + second) / 2.0).collect();
    pack(&middle, widths, true)
}

/// One packing pass, from the left or from the right.
fn pack(wanted: &[f32], widths: &[f32], from_left: bool) -> Vec<f32> {
    let mut out = wanted.to_vec();
    if out.is_empty() {
        return out;
    }
    if from_left {
        for at in 1..out.len() {
            let lowest = out[at - 1] + widths[at - 1] / 2.0 + NODE_GAP + widths[at] / 2.0;
            out[at] = out[at].max(lowest);
        }
    } else {
        for at in (0..out.len() - 1).rev() {
            let highest = out[at + 1] - widths[at + 1] / 2.0 - NODE_GAP - widths[at] / 2.0;
            out[at] = out[at].min(highest);
        }
    }
    out
}

/// Slide everything so the leftmost edge is at zero.
///
/// The leftmost **edge**, not the leftmost centre. Sliding by the centre leaves the widest node in
/// the first rank hanging half its width off the left of the diagram, where it is clipped away — a
/// fault the shared "nothing is placed outside the scene" test caught on the first run.
fn normalise(across: &mut [Vec<f32>], widths: &[Vec<f32>]) {
    let smallest = across
        .iter()
        .zip(widths)
        .flat_map(|(row, sizes)| row.iter().zip(sizes).map(|(centre, width)| centre - width / 2.0))
        .fold(f32::INFINITY, f32::min);
    if !smallest.is_finite() {
        return;
    }
    for row in across.iter_mut() {
        for value in row.iter_mut() {
            *value -= smallest;
        }
    }
}

/// How tall each rank is: the tallest thing in it.
fn rank_heights(sizes: &[Size], order: &[Vec<Slot>], labels: &LabelSlots) -> Vec<f32> {
    order
        .iter()
        .enumerate()
        .map(|(rank, layer)| {
            layer
                .iter()
                .map(|slot| match slot {
                    Slot::Entity(index) => sizes[*index].height,
                    Slot::Dummy(_) => labels.get(&key(*slot, rank)).map_or(0.0, |room| room.height),
                })
                .fold(0.0_f32, f32::max)
        })
        .collect()
}

/// The gap between two ranks: the usual one, widened to hold the largest edge label.
///
/// One gap for the whole diagram rather than one per rank. A diagram whose rows were different
/// distances apart because one of them happened to carry a two-line label reads as though the
/// spacing meant something, and it does not.
fn rank_gap(edges: &[Lifted]) -> f32 {
    RANK_GAP + edges.iter().map(|edge| edge.label.height).fold(0.0_f32, f32::max)
}

/// How much room everything placed takes up.
fn extent(entities: &[Rect], centres: &[Vec<Point>]) -> Size {
    let mut size = Size::default();
    for rect in entities {
        size.width = size.width.max(rect.right());
        size.height = size.height.max(rect.bottom());
    }
    for row in centres {
        for point in row {
            size.width = size.width.max(point.x);
            size.height = size.height.max(point.y);
        }
    }
    size
}

/// What routing needs to know beyond where the slots are.
struct Routing<'a> {
    kinds: &'a [Kind],
    anchors: &'a Anchors,
    boundary: Boundary,
    /// How far an edge runs straight out of a band before it turns, so an arrow meets a box head on.
    stub: f32,
}

/// One edge's run through the ranks: the entity at the top, the dummies, the entity at the bottom.
struct Run {
    top: usize,
    dummies: Vec<SlotKey>,
    bottom: usize,
}

/// Turn the placed slots into one polyline an edge, and say where its label goes.
///
/// **An edge runs straight through a rank and turns only in the gap between two** (`task-2194`).
/// Every edge used to go from the centre of one slot to the centre of the next, so it bent in the
/// middle of every rank it crossed and left a box at whatever angle its neighbour happened to be at.
/// Now it leaves a box straight down, runs down through each band it passes, and makes each change
/// of lane in the gap between two bands, where nothing else is. The caller rounds the corners.
///
/// **Several edges on one side of a box leave it side by side** rather than all from its middle, in
/// the order of where they are going, so two edges to neighbouring boxes do not start out on top of
/// each other.
fn route(
    edges: &[Lifted],
    reversed: &[bool],
    chains: &Chains,
    placed: &Positions,
    labels: &LabelSlots,
    routing: &Routing,
) -> Arranged {
    let mut where_is: HashMap<SlotKey, (Point, usize)> = HashMap::new();
    for (rank, layer) in placed.layers.iter().enumerate() {
        for (at, slot) in layer.iter().enumerate() {
            where_is.insert(key(*slot, rank), (placed.centres[rank][at], rank));
        }
    }
    let band = |slot: &SlotKey| -> (f32, f32) {
        let (centre, rank) = where_is[slot];
        let half = placed.heights[rank] / 2.0;
        (centre.y - half, centre.y + half)
    };
    let runs: Vec<Run> = edges
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let (top, bottom) = ends(edge, reversed[index]);
            let dummies = chains[index].iter().map(|&rank| (1, index, rank)).collect();
            Run { top, dummies, bottom }
        })
        .collect();
    let mut at_end = side_positions(edges, &runs, placed, routing, &where_is);
    // A port sits straight above or below whatever it is joined to, so the edge crosses the frame
    // without a kink.
    for (index, run) in runs.iter().enumerate() {
        if matches!(routing.kinds[run.top], Kind::Port { .. }) {
            let next = match run.dummies.first() {
                Some(dummy) => where_is[dummy].0.x,
                None => at_end[&(index, false)],
            };
            at_end.insert((index, true), next);
        }
        if matches!(routing.kinds[run.bottom], Kind::Port { .. }) {
            let previous = match run.dummies.last() {
                Some(dummy) => where_is[dummy].0.x,
                None => at_end[&(index, true)],
            };
            at_end.insert((index, false), previous);
        }
    }
    let stub = routing.stub;
    let mut paths = Vec::with_capacity(edges.len());
    for (index, run) in runs.iter().enumerate() {
        let top_x = at_end[&(index, true)];
        let bottom_x = at_end[&(index, false)];
        let top_key = (0, run.top, 0);
        let bottom_key = (0, run.bottom, 0);
        let mut points = vec![inner_point(run.top, top_x, true, placed, routing)];
        if !matches!(routing.kinds[run.top], Kind::Port { .. }) {
            points.push(Point::new(top_x, band(&top_key).1 + stub));
        }
        for dummy in &run.dummies {
            let (top, bottom) = band(dummy);
            let x = where_is[dummy].0.x;
            points.push(Point::new(x, top - stub));
            points.push(Point::new(x, bottom + stub));
        }
        if !matches!(routing.kinds[run.bottom], Kind::Port { .. }) {
            points.push(Point::new(bottom_x, band(&bottom_key).0 - stub));
        }
        points.push(inner_point(run.bottom, bottom_x, false, placed, routing));
        let mut points = simplify(&points);
        // The run was built from the top down, which for a reversed edge is from its target.
        // Drawing it the way it was written means walking it the other way.
        if reversed[index] {
            points.reverse();
        }
        // The label's own slot when it has one, which is a point on the line by construction.
        let label = run
            .dummies
            .iter()
            .find(|slot| labels.contains_key(slot))
            .map(|slot| where_is[slot].0)
            .unwrap_or_else(|| midpoint(&points));
        paths.push(Path { points, label });
    }
    Arranged { entities: placed.entities.clone(), paths, size: placed.size }
}

/// Where each edge meets the entity at each of its two ends, across the page.
///
/// Keyed by the edge and whether it is the end at the top. An edge reaching a subgraph that was laid
/// out with a crossing for it uses that crossing; every other edge on one side of an entity is spread
/// across the middle of that side in the order of where it goes next.
fn side_positions(
    edges: &[Lifted],
    runs: &[Run],
    placed: &Positions,
    routing: &Routing,
    where_is: &HashMap<SlotKey, (Point, usize)>,
) -> HashMap<(usize, bool), f32> {
    let centre_of = |entity: usize| placed.entities[entity].centre().x;
    // For every entity and side, the edges on it and where each goes next.
    let mut sides: HashMap<(usize, bool), Vec<(f32, usize)>> = HashMap::new();
    let mut out = HashMap::new();
    for (index, run) in runs.iter().enumerate() {
        let next = run.dummies.first().map_or_else(|| centre_of(run.bottom), |d| where_is[d].0.x);
        let previous = run.dummies.last().map_or_else(|| centre_of(run.top), |d| where_is[d].0.x);
        for (entity, at_top, towards) in [(run.top, true, next), (run.bottom, false, previous)] {
            let rect = placed.entities[entity];
            match routing.kinds[entity] {
                Kind::Port { .. } => {}
                Kind::Group if routing.anchors.contains_key(&(edges[index].edge, entity)) => {
                    let x = rect.x + routing.anchors[&(edges[index].edge, entity)];
                    out.insert((index, at_top), x);
                }
                // The edge leaves the top entity by its bottom side and reaches the bottom one by its
                // top side, so "the end at the top" and "the bottom side" are the same question.
                _ => sides.entry((entity, at_top)).or_default().push((towards, index)),
            }
        }
    }
    let mut keys: Vec<(usize, bool)> = sides.keys().copied().collect();
    keys.sort_unstable();
    for (entity, bottom_side) in keys {
        let mut uses = sides.remove(&(entity, bottom_side)).unwrap_or_default();
        uses.sort_by(|a, b| {
            a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1))
        });
        let rect = placed.entities[entity];
        let xs = spread(rect.centre().x, rect.width, uses.len());
        for ((_, index), x) in uses.into_iter().zip(xs) {
            out.insert((index, bottom_side), x);
        }
    }
    out
}

/// `count` positions across the middle of a side `width` wide, centred on `centre`.
fn spread(centre: f32, width: f32, count: usize) -> Vec<f32> {
    if count <= 1 {
        return vec![centre; count];
    }
    let step = (width * SIDE_SHARE / (count - 1) as f32).min(SIDE_STEP);
    let first = centre - step * (count - 1) as f32 / 2.0;
    (0..count).map(|at| first + step * at as f32).collect()
}

/// Where an edge's line begins inside the entity at one of its ends.
///
/// Inside a node, at its middle height, so the caller can cut it back to whatever shape it drew. On a
/// subgraph's frame, because the part inside the frame is drawn by the layout inside it, or because
/// the edge names the subgraph itself. On this container's own frame for a port.
fn inner_point(
    entity: usize,
    x: f32,
    at_top: bool,
    placed: &Positions,
    routing: &Routing,
) -> Point {
    let rect = placed.entities[entity];
    match routing.kinds[entity] {
        Kind::Node => Point::new(x, rect.centre().y),
        Kind::Group if at_top => Point::new(x, rect.bottom()),
        Kind::Group => Point::new(x, rect.top()),
        Kind::Port { top: true } => Point::new(x, routing.boundary.top),
        Kind::Port { top: false } => {
            Point::new(x, placed.size.height + routing.boundary.bottom_margin)
        }
    }
}

/// The same line with repeated points and points in the middle of a straight run taken out.
fn simplify(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &point in points {
        if out.last().is_some_and(|last| last.distance(point) < 0.01) {
            continue;
        }
        if out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let cross = (b.x - a.x) * (point.y - b.y) - (b.y - a.y) * (point.x - b.x);
            let forwards = (b.x - a.x) * (point.x - b.x) + (b.y - a.y) * (point.y - b.y) >= 0.0;
            if cross.abs() < 0.01 && forwards {
                out.pop();
            }
        }
        out.push(point);
    }
    out
}

/// The middle of a polyline, measured along it rather than between its ends.
fn midpoint(points: &[Point]) -> Point {
    if points.len() < 2 {
        return points.first().copied().unwrap_or_default();
    }
    let total: f32 = points.windows(2).map(|pair| pair[0].distance(pair[1])).sum();
    let mut walked = 0.0;
    for pair in points.windows(2) {
        let length = pair[0].distance(pair[1]);
        if walked + length >= total / 2.0 && length > 0.0 {
            return pair[0].towards(pair[1], (total / 2.0 - walked) / length);
        }
        walked += length;
    }
    points[points.len() / 2]
}

/// Reflect the whole layout across the diagonal, turning a downward diagram into a rightward one.
fn transpose(placed: &mut Placed) {
    for rect in placed.nodes.iter_mut().chain(placed.groups.iter_mut()) {
        *rect = Rect::new(rect.y, rect.x, rect.height, rect.width);
    }
    for path in &mut placed.edges {
        for point in path.iter_mut() {
            *point = Point::new(point.y, point.x);
        }
    }
    for label in &mut placed.labels {
        *label = Point::new(label.y, label.x);
    }
    placed.size = Size::new(placed.size.height, placed.size.width);
}

fn flip_vertically(placed: &mut Placed) {
    let height = placed.size.height;
    for rect in placed.nodes.iter_mut().chain(placed.groups.iter_mut()) {
        rect.y = height - rect.bottom();
    }
    for path in &mut placed.edges {
        for point in path.iter_mut() {
            point.y = height - point.y;
        }
    }
    for label in &mut placed.labels {
        label.y = height - label.y;
    }
}

fn flip_horizontally(placed: &mut Placed) {
    let width = placed.size.width;
    for rect in placed.nodes.iter_mut().chain(placed.groups.iter_mut()) {
        rect.x = width - rect.right();
    }
    for path in &mut placed.edges {
        for point in path.iter_mut() {
            point.x = width - point.x;
        }
    }
    for label in &mut placed.labels {
        label.x = width - label.x;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(count: usize) -> Graph {
        let mut graph = Graph::default();
        for _ in 0..count {
            graph.add_node(Size::new(80.0, 40.0), None);
        }
        for index in 1..count {
            graph.edges.push(EdgeSpec::new(index - 1, index));
        }
        graph
    }

    /// Whether an edge's end is inside a node at its middle height, which is where every edge begins
    /// and ends: beside the middle when several share one side of the box.
    fn in_the_middle_of(point: Point, rect: Rect) -> bool {
        rect.contains(point) && (point.y - rect.centre().y).abs() < 0.01
    }

    /// Whether a polyline passes through the inside of `rect`, sampled finely enough for a box of forty
    /// points.
    fn runs_through(path: &[Point], rect: Rect) -> bool {
        let inner = Rect::new(rect.x + 1.0, rect.y + 1.0, rect.width - 2.0, rect.height - 2.0);
        path.windows(2).any(|pair| {
            (0..=100).any(|step| {
                let at = pair[0].towards(pair[1], step as f32 / 100.0);
                at.x > inner.x && at.x < inner.right() && at.y > inner.y && at.y < inner.bottom()
            })
        })
    }

    /// An edge from outside a subgraph to the last of three stacked nodes inside it reaches that node
    /// without running through the two above it or across the frame. `task-2063` photographed lines
    /// running through three boxes to reach a fourth.
    #[test]
    fn an_edge_into_a_subgraph_does_not_run_through_the_nodes_inside_it() {
        let mut graph = Graph::default();
        graph.groups.push(GroupSpec { title: Size::new(60.0, 18.0), parent: None });
        let a = graph.add_node(Size::new(120.0, 40.0), Some(0));
        let b = graph.add_node(Size::new(120.0, 40.0), Some(0));
        let c = graph.add_node(Size::new(120.0, 40.0), Some(0));
        let outside = graph.add_node(Size::new(120.0, 40.0), None);
        graph.edges.push(EdgeSpec::new(a, b));
        graph.edges.push(EdgeSpec::new(b, c));
        graph.edges.push(EdgeSpec::new(outside, c));
        let placed = layout(&graph);
        let path = &placed.edges[2];
        for blocker in [a, b] {
            assert!(
                !runs_through(path, placed.nodes[blocker]),
                "{path:?} runs through node {blocker}"
            );
        }
        assert!(
            in_the_middle_of(*path.last().expect("an end"), placed.nodes[c]),
            "and it ends on the node: {path:?}"
        );
    }

    #[test]
    fn a_chain_runs_down_the_page_in_order() {
        let placed = layout(&chain(4));
        for index in 1..4 {
            assert!(
                placed.nodes[index].top() > placed.nodes[index - 1].top(),
                "node {index} should be below node {}",
                index - 1
            );
        }
        assert_eq!(placed.nodes.len(), 4);
        assert_eq!(placed.edges.len(), 3);
    }

    #[test]
    fn left_to_right_runs_across_the_page_and_keeps_the_node_sizes() {
        let mut graph = chain(3);
        graph.direction = Direction::Right;
        let placed = layout(&graph);
        for index in 1..3 {
            assert!(
                placed.nodes[index].left() > placed.nodes[index - 1].left(),
                "node {index} should be to the right of node {}",
                index - 1
            );
        }
        // Turning the layout must not turn the boxes: they are still eighty by forty.
        for rect in &placed.nodes {
            assert_eq!(rect.size(), Size::new(80.0, 40.0), "the node keeps its own shape");
        }
    }

    #[test]
    fn bottom_to_top_is_the_same_layout_upside_down() {
        let mut graph = chain(3);
        graph.direction = Direction::Up;
        let placed = layout(&graph);
        for index in 1..3 {
            assert!(placed.nodes[index].top() < placed.nodes[index - 1].top());
        }
        assert!(placed.nodes.iter().all(|rect| rect.top() >= -0.01), "nothing above the top edge");
    }

    #[test]
    fn two_nodes_in_the_same_rank_never_overlap() {
        // One node pointing at six, which puts all six in one rank.
        let mut graph = Graph::default();
        let root = graph.add_node(Size::new(90.0, 40.0), None);
        for _ in 0..6 {
            let leaf = graph.add_node(Size::new(90.0, 40.0), None);
            graph.edges.push(EdgeSpec::new(root, leaf));
        }
        let placed = layout(&graph);
        for first in 0..placed.nodes.len() {
            for second in first + 1..placed.nodes.len() {
                assert!(
                    !placed.nodes[first].overlaps(&placed.nodes[second]),
                    "{first} and {second} overlap: {:?} {:?}",
                    placed.nodes[first],
                    placed.nodes[second]
                );
            }
        }
    }

    #[test]
    fn a_cycle_is_laid_out_rather_than_looping_for_ever() {
        let mut graph = chain(3);
        graph.edges.push(EdgeSpec::new(2, 0));
        let placed = layout(&graph);
        assert_eq!(placed.nodes.len(), 3);
        // The edge that closes the cycle is still drawn, and still from 2 to 0.
        let back = &placed.edges[2];
        assert!(back.len() >= 2);
        assert!(in_the_middle_of(back[0], placed.nodes[2]), "{back:?}");
        assert!(in_the_middle_of(back[back.len() - 1], placed.nodes[0]), "{back:?}");
    }

    #[test]
    fn a_node_pointing_at_itself_is_left_for_the_caller() {
        // A self loop cannot be ranked, so it is not laid out here. The caller draws it as a loop
        // beside the node, which is what Mermaid does too.
        let mut graph = Graph::default();
        let only = graph.add_node(Size::new(60.0, 30.0), None);
        graph.edges.push(EdgeSpec::new(only, only));
        let placed = layout(&graph);
        assert_eq!(placed.nodes.len(), 1);
        assert!(placed.edges[0].is_empty(), "a self loop gets no route");
    }

    #[test]
    fn an_edge_spanning_several_ranks_bends_through_them() {
        // Zero to three directly, past two ranks: the route should have points in between.
        let mut graph = chain(4);
        graph.edges.push(EdgeSpec::new(0, 3));
        let placed = layout(&graph);
        assert!(
            placed.edges[3].len() > 2,
            "the long edge should bend through the ranks it passes, not cut across them"
        );
    }

    #[test]
    fn a_subgraph_is_placed_as_one_box_with_its_members_inside_it() {
        let mut graph = Graph::default();
        graph.groups.push(GroupSpec { title: Size::new(60.0, 18.0), parent: None });
        let outside = graph.add_node(Size::new(80.0, 40.0), None);
        let first = graph.add_node(Size::new(80.0, 40.0), Some(0));
        let second = graph.add_node(Size::new(80.0, 40.0), Some(0));
        graph.edges.push(EdgeSpec::new(first, second));
        graph.edges.push(EdgeSpec::new(outside, first));
        let placed = layout(&graph);
        let frame = placed.groups[0];
        for member in [first, second] {
            let rect = placed.nodes[member];
            assert!(
                rect.left() >= frame.left() - 0.01 && rect.right() <= frame.right() + 0.01,
                "member {member} should be inside the frame across"
            );
            assert!(
                rect.top() >= frame.top() - 0.01 && rect.bottom() <= frame.bottom() + 0.01,
                "member {member} should be inside the frame down"
            );
        }
        assert!(
            !placed.nodes[outside].overlaps(&frame),
            "a node outside the subgraph must not sit on top of it"
        );
    }

    #[test]
    fn an_edge_into_a_subgraph_still_ends_on_the_real_node() {
        let mut graph = Graph::default();
        graph.groups.push(GroupSpec { title: Size::new(40.0, 18.0), parent: None });
        let outside = graph.add_node(Size::new(80.0, 40.0), None);
        let inside = graph.add_node(Size::new(80.0, 40.0), Some(0));
        graph.edges.push(EdgeSpec::new(outside, inside));
        let placed = layout(&graph);
        let path = &placed.edges[0];
        assert!(in_the_middle_of(path[0], placed.nodes[outside]), "{path:?}");
        assert!(
            in_the_middle_of(path[path.len() - 1], placed.nodes[inside]),
            "it points at the node, not at the frame round it: {path:?}"
        );
    }

    #[test]
    fn laying_the_same_graph_out_twice_gives_exactly_the_same_answer() {
        // The whole of the screenshot testing rests on this.
        let mut graph = chain(6);
        graph.edges.push(EdgeSpec::new(0, 4));
        graph.edges.push(EdgeSpec::new(5, 1));
        assert_eq!(layout(&graph), layout(&graph));
    }

    #[test]
    fn nothing_is_placed_above_or_left_of_the_origin() {
        let mut graph = chain(5);
        graph.edges.push(EdgeSpec::new(0, 4));
        graph.edges.push(EdgeSpec::new(3, 1));
        let placed = layout(&graph);
        for rect in &placed.nodes {
            assert!(rect.left() >= -0.01 && rect.top() >= -0.01, "{rect:?} is outside the scene");
        }
    }

    #[test]
    fn an_empty_graph_lays_out_to_nothing_rather_than_panicking() {
        assert_eq!(layout(&Graph::default()), Placed::default());
    }
}
