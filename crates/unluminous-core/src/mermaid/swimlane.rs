//! Swimlane diagrams: `swimlane-beta`.
//!
//! A flowchart whose top level subgraphs are lanes: bands that run the whole length of the diagram,
//! one for each person, team or system that owns the steps inside it. The source is the flowchart's
//! own, so it is read by [`flowchart::read`] and only the layout is this module's.
//!
//! ## Why the layered layout is not used
//!
//! The layered layout draws a subgraph as a frame sized to what is in it and places it as one box, so
//! three subgraphs come out as three boxes of three sizes in whatever places suit the edges. A lane
//! has to be a band from one end of the picture to the other, in the order the lanes were written,
//! with every step in it lined up against the steps in the other lanes by when it happens.
//!
//! So the layout here is a grid. Every node gets a rank, the length of the longest path to it once the
//! cycles are broken, which is the column it sits in; its lane is the band. Nodes in one lane and one
//! rank stand side by side across the band. Edges run along the gaps between the columns, which are
//! made wide enough for the labels, so a line never crosses a box and a label never covers one.
//!
//! Everything is worked out along two axes, `u` the way the diagram flows and `v` across it, and turned
//! into `x` and `y` at the end, so left to right and top to bottom are one piece of arithmetic.

use super::flowchart::{self, Chart, LinkStyle};
use super::layered::Direction;
use super::parts;
use super::scene::{Dash, Item, Paint, Point, Rect, Scene, Size, Stroke};
use super::source::Source;
use super::text::{self, Label};
use super::{Options, Problem};

/// How wide a gap between two columns is before any label asks for more.
const GAP: f32 = 56.0;
/// The space between two nodes standing side by side in one cell.
const STACK_GAP: f32 = 18.0;
/// The space left inside a lane round the nodes.
const LANE_PADDING: f32 = 16.0;
/// The strip at the start of each lane that a line going backwards runs along.
const CORRIDOR: f32 = 16.0;
/// How far apart two lines sharing a gap are drawn.
const TRACK: f32 = 8.0;
/// How wide a lane's title may get before it wraps, in a left to right diagram.
const TITLE_WRAP: f32 = 150.0;

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let chart = flowchart::read(source)?;
    Ok(draw(&chart, source, options))
}

/// The lane each node is in: the subgraph at the top of the ones it is inside.
fn lanes_of(chart: &Chart) -> (Vec<String>, Vec<usize>) {
    let tops: Vec<usize> =
        (0..chart.groups.len()).filter(|at| chart.groups[*at].parent.is_none()).collect();
    let mut titles: Vec<String> = tops.iter().map(|at| chart.groups[*at].title.clone()).collect();
    let mut lane = Vec::with_capacity(chart.nodes.len());
    let mut loose = false;
    for node in &chart.nodes {
        let mut group = node.group;
        let mut top = None;
        while let Some(at) = group {
            top = Some(at);
            group = chart.groups[at].parent;
        }
        match top.and_then(|at| tops.iter().position(|known| *known == at)) {
            Some(index) => lane.push(index),
            None => {
                loose = true;
                lane.push(usize::MAX);
            }
        }
    }
    // A node in no lane at all is given a lane of its own at the end, with no title.
    if loose {
        let index = titles.len();
        titles.push(String::new());
        for value in &mut lane {
            if *value == usize::MAX {
                *value = index;
            }
        }
    }
    if titles.is_empty() {
        titles.push(String::new());
    }
    (titles, lane)
}

/// The rank of every node: the longest path to it once every edge that closes a cycle is ignored.
fn ranks(count: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut outgoing = vec![Vec::new(); count];
    for (index, (from, to)) in edges.iter().enumerate() {
        if from != to {
            outgoing[*from].push((*to, index));
        }
    }
    // A depth first walk in the order the nodes were written; an edge to a node still on the walk's
    // stack closes a cycle.
    let mut state = vec![0_u8; count];
    let mut backwards = vec![false; edges.len()];
    for start in 0..count {
        if state[start] != 0 {
            continue;
        }
        let mut stack = vec![(start, 0_usize)];
        state[start] = 1;
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            if let Some(&(to, edge)) = outgoing[node].get(*next) {
                *next += 1;
                match state[to] {
                    0 => {
                        state[to] = 1;
                        stack.push((to, 0));
                    }
                    1 => backwards[edge] = true,
                    _ => {}
                }
            } else {
                state[node] = 2;
                stack.pop();
            }
        }
    }
    let mut rank = vec![0_usize; count];
    // At most `count` passes are ever needed on what is left, which has no cycle.
    for _ in 0..count {
        let mut changed = false;
        for (index, (from, to)) in edges.iter().enumerate() {
            if from != to && !backwards[index] && rank[*to] < rank[*from] + 1 {
                rank[*to] = rank[*from] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    rank
}

/// A rectangle measured along the flow and across it.
#[derive(Debug, Clone, Copy, Default)]
struct Span {
    u: f32,
    v: f32,
    du: f32,
    dv: f32,
}

/// Everything the drawing needs, worked out in `u` and `v`.
struct Plan {
    nodes: Vec<Span>,
    lanes: Vec<Span>,
    /// How far along the flow the lane titles take, before the first column.
    header: f32,
    /// Where each gap between columns starts and how wide it is. Gap `r` follows column `r`.
    gaps: Vec<(f32, f32)>,
    /// Along the flow and across it.
    total: Size,
    horizontal: bool,
    /// Right to left or bottom to top: the flow runs from the far end.
    reversed: bool,
}

impl Plan {
    /// A span turned into a rectangle in the diagram's own `x` and `y`.
    fn rect(&self, span: Span) -> Rect {
        let u = if self.reversed { self.total.width - span.u - span.du } else { span.u };
        match self.horizontal {
            true => Rect::new(u, span.v, span.du, span.dv),
            false => Rect::new(span.v, u, span.dv, span.du),
        }
    }

    /// A point given along the flow and across it, in the diagram's own `x` and `y`.
    fn point(&self, u: f32, v: f32) -> Point {
        let u = if self.reversed { self.total.width - u } else { u };
        match self.horizontal {
            true => Point::new(u, v),
            false => Point::new(v, u),
        }
    }

    /// The whole grid's size in `x` and `y`.
    fn rect_total(&self) -> Size {
        match self.horizontal {
            true => self.total,
            false => Size::new(self.total.height, self.total.width),
        }
    }
}

fn draw(chart: &Chart, source: &Source, options: &Options) -> Scene {
    let horizontal = matches!(chart.direction, Direction::Right | Direction::Left);
    let (titles, lane_of) = lanes_of(chart);
    let edges: Vec<(usize, usize)> = chart.links.iter().map(|(from, to, _)| (*from, *to)).collect();
    // A link to a whole subgraph has no node to rank against, so it is left out of the grid.
    let usable: Vec<bool> =
        chart.links.iter().map(|(_, _, link)| !link.from_group && !link.to_group).collect();
    let ranked: Vec<(usize, usize)> =
        edges.iter().zip(&usable).filter(|(_, ok)| **ok).map(|(edge, _)| *edge).collect();
    let rank = ranks(chart.nodes.len(), &ranked);
    let style = options.style(1.0, false);
    let labels: Vec<Label> = chart
        .nodes
        .iter()
        .map(|node| text::measure(&node.label, &style, options.metrics, text::WRAP))
        .collect();
    let link_style = options.style(0.85, false);
    let link_labels: Vec<Label> = chart
        .links
        .iter()
        .map(|(_, _, link)| {
            text::measure(&link.label, &link_style, options.metrics, text::EDGE_WRAP)
        })
        .collect();
    let plan =
        plan(chart, &titles, &lane_of, &rank, &labels, &link_labels, &usable, options, horizontal);

    let mut scene = Scene::new();
    let top = parts::title(&mut scene, source, options, plan.total.width);
    let mut body = Scene::new();
    draw_lanes(&mut body, &plan, &titles, options);
    let label_spots = draw_links(&mut body, chart, &plan, &rank, &link_labels, &usable, options);
    draw_nodes(&mut body, chart, &plan, &labels, options);
    for (index, spot) in label_spots.iter().enumerate() {
        if let Some(at) = spot {
            draw_link_label(&mut body, &link_labels[index], *at, options);
        }
    }
    body.claim(Rect::new(0.0, 0.0, plan.rect_total().width, plan.rect_total().height));
    scene.add_scene(body, parts::MARGIN, top + parts::MARGIN);
    scene.claim(Rect::new(
        0.0,
        0.0,
        parts::MARGIN + plan.rect_total().width,
        top + parts::MARGIN + plan.rect_total().height,
    ));
    parts::finish(&mut scene);
    scene
}

/// Work out every node's, every lane's and every gap's place.
#[allow(clippy::too_many_arguments)]
fn plan(
    chart: &Chart,
    titles: &[String],
    lane_of: &[usize],
    rank: &[usize],
    labels: &[Label],
    link_labels: &[Label],
    usable: &[bool],
    options: &Options,
    horizontal: bool,
) -> Plan {
    let along = |size: Size| if horizontal { size.width } else { size.height };
    let across = |size: Size| if horizontal { size.height } else { size.width };
    let sizes: Vec<Size> = chart
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| node.shape.size_for(labels[index].size()))
        .collect();
    let columns = rank.iter().copied().max().map_or(1, |most| most + 1);
    let mut column_size = vec![0.0_f32; columns];
    for (index, size) in sizes.iter().enumerate() {
        column_size[rank[index]] = column_size[rank[index]].max(along(*size));
    }
    // Each gap is wide enough for the widest label on a line that turns in it.
    let mut gap_size = vec![GAP; columns];
    for (index, (from, to, _)) in chart.links.iter().enumerate() {
        if !usable[index] || link_labels[index].is_empty() {
            continue;
        }
        let gap = gap_of(rank[*from], rank[*to]);
        if let Some(size) = gap_size.get_mut(gap) {
            *size = size.max(along(link_labels[index].size()) + 30.0);
        }
    }
    let title_style = options.style(0.95, true);
    let title_labels: Vec<Label> = titles
        .iter()
        .map(|title| text::measure(title, &title_style, options.metrics, TITLE_WRAP))
        .collect();
    let header = match horizontal {
        true => title_labels.iter().map(|label| label.width).fold(0.0, f32::max) + 28.0,
        false => title_labels.iter().map(|label| label.height).fold(0.0, f32::max) + 20.0,
    };
    // The gap before the first column takes a line coming back to a node in it.
    let lead = GAP * 0.6;
    let mut column_start = vec![0.0_f32; columns];
    let mut gaps = Vec::with_capacity(columns);
    let mut u = header + lead;
    for column in 0..columns {
        column_start[column] = u;
        u += column_size[column];
        gaps.push((u, gap_size[column]));
        u += gap_size[column];
    }
    let total_u = u - gap_size[columns - 1] + lead;

    // Across the flow: each lane is as deep as its deepest cell.
    let mut nodes = vec![Span::default(); chart.nodes.len()];
    let mut lanes = Vec::with_capacity(titles.len());
    let mut v = 0.0_f32;
    for (lane, title) in title_labels.iter().enumerate() {
        let cells: Vec<Vec<usize>> = (0..columns)
            .map(|column| {
                (0..chart.nodes.len())
                    .filter(|at| lane_of[*at] == lane && rank[*at] == column)
                    .collect()
            })
            .collect();
        let depth = |cell: &Vec<usize>| -> f32 {
            let sum: f32 = cell.iter().map(|at| across(sizes[*at])).sum();
            sum + STACK_GAP * cell.len().saturating_sub(1) as f32
        };
        let deepest = cells.iter().map(depth).fold(0.0, f32::max);
        let title_room = if horizontal { title.height + 12.0 } else { 0.0 };
        let content = deepest.max(title_room).max(40.0);
        let lane_top = v;
        let inner_top = lane_top + CORRIDOR + LANE_PADDING;
        for (column, cell) in cells.iter().enumerate() {
            let mut at = inner_top + (content - depth(cell)) / 2.0;
            for &node in cell {
                let size = sizes[node];
                nodes[node] = Span {
                    u: column_start[column] + (column_size[column] - along(size)) / 2.0,
                    v: at,
                    du: along(size),
                    dv: across(size),
                };
                at += across(size) + STACK_GAP;
            }
        }
        let lane_depth = CORRIDOR + LANE_PADDING * 2.0 + content;
        lanes.push(Span { u: 0.0, v: lane_top, du: total_u, dv: lane_depth });
        v += lane_depth;
    }
    let total = Size::new(total_u, v);
    let reversed = matches!(chart.direction, Direction::Left | Direction::Up);
    Plan { nodes, lanes, header, gaps, total, horizontal, reversed }
}

/// Which gap between columns a line from rank `from` to rank `to` turns in: the one just before its
/// target, or for a line going backwards the one just after its source.
fn gap_of(from: usize, to: usize) -> usize {
    if to > from {
        to - 1
    } else {
        from
    }
}

/// The lanes: alternating washes, a line between each, and the titles in the strip at their start.
fn draw_lanes(scene: &mut Scene, plan: &Plan, titles: &[String], options: &Options) {
    let theme = &options.theme;
    let title_style = options.style(0.95, true);
    for (index, lane) in plan.lanes.iter().enumerate() {
        let rect = plan.rect(*lane);
        let wash = if index % 2 == 0 {
            theme.group_fill
        } else {
            Paint::faded(theme.group_fill.color, theme.group_fill.alpha / 2)
        };
        scene.add(Item::Rect {
            rect,
            radius: 0.0,
            fill: Some(wash),
            stroke: Some(Stroke::new(theme.group_stroke, parts::LINE)),
        });
        let header = plan.rect(Span { u: lane.u, v: lane.v, du: plan.header, dv: lane.dv });
        scene.add(Item::Rect {
            rect: header,
            radius: 0.0,
            fill: Some(Paint::faded(theme.node_fill.color, 200)),
            stroke: Some(Stroke::new(theme.group_stroke, parts::LINE)),
        });
        let wrap = if plan.horizontal { TITLE_WRAP } else { header.width - 16.0 };
        let label = text::measure(&titles[index], &title_style, options.metrics, wrap.max(40.0));
        parts::centred_label(
            scene,
            &label,
            header,
            &parts::text_style(options, 0.95, true, theme.text),
        );
    }
}

/// Every link, routed along the gaps. Returns where each label goes.
fn draw_links(
    scene: &mut Scene,
    chart: &Chart,
    plan: &Plan,
    rank: &[usize],
    link_labels: &[Label],
    usable: &[bool],
    options: &Options,
) -> Vec<Option<Point>> {
    let theme = &options.theme;
    let mut spots = vec![None; chart.links.len()];
    // How many lines already run along each gap, so the next one takes a track of its own.
    let mut used = vec![0_usize; plan.gaps.len()];
    for (index, (from, to, link)) in chart.links.iter().enumerate() {
        if !usable[index] || link.style == LinkStyle::Invisible || from == to {
            continue;
        }
        let (a, b) = (plan.nodes[*from], plan.nodes[*to]);
        let gap = gap_of(rank[*from], rank[*to]).min(plan.gaps.len() - 1);
        let (gap_start, gap_width) = plan.gaps[gap];
        let tracks = ((gap_width - 24.0) / TRACK).max(1.0) as usize;
        let track = used[gap] % tracks;
        used[gap] += 1;
        let middle = gap_start + gap_width / 2.0;
        let offset = (track as f32 - (tracks.min(used[gap].max(1)) as f32 - 1.0) / 2.0) * TRACK;
        let channel = (middle + offset).clamp(gap_start + 10.0, gap_start + gap_width - 10.0);
        let (av, bv) = (a.v + a.dv / 2.0, b.v + b.dv / 2.0);
        let route: Vec<(f32, f32)> = if rank[*to] > rank[*from] {
            vec![(a.u + a.du, av), (channel, av), (channel, bv), (b.u, bv)]
        } else if rank[*to] == rank[*from] {
            // Same column: straight across the flow from one box's side to the other's.
            let (start, end) = if bv > av { (a.v + a.dv, b.v) } else { (a.v, b.v + b.dv) };
            let u = a.u + a.du / 2.0;
            vec![(u, start), (u, end)]
        } else {
            // Backwards: out of the source's far side, along the corridor at the top of the target's
            // lane, and into the target from its near side.
            let lane = plan
                .lanes
                .iter()
                .find(|lane| bv >= lane.v && bv <= lane.v + lane.dv)
                .copied()
                .unwrap_or_default();
            let corridor = lane.v + CORRIDOR / 2.0 + (track as f32 * 3.0).min(CORRIDOR / 2.0 - 2.0);
            let (lead_start, lead_width) = if rank[*to] == 0 {
                (plan.header, plan.gaps.first().map_or(GAP, |_| GAP * 0.6))
            } else {
                plan.gaps[rank[*to] - 1]
            };
            let entry = lead_start + lead_width / 2.0;
            vec![
                (a.u + a.du, av),
                (channel, av),
                (channel, corridor),
                (entry, corridor),
                (entry, bv),
                (b.u, bv),
            ]
        };
        let points: Vec<Point> = route.iter().map(|(u, v)| plan.point(*u, *v)).collect();
        let points = dedupe(points);
        if points.len() < 2 {
            continue;
        }
        let rounded = parts::rounded(&points, 8.0, parts::HEAD);
        let width = if link.style == LinkStyle::Thick { parts::THICK } else { parts::LINE };
        let dash = if link.style == LinkStyle::Dotted { parts::DASH } else { Dash::Solid };
        let drawn = parts::trimmed(
            &rounded,
            parts::ending_inset(link.tail),
            parts::ending_inset(link.head),
        );
        scene.add(Item::Line { points: drawn, stroke: Stroke::new(theme.line, width), dash });
        let last = rounded.len() - 1;
        parts::ending(
            scene,
            link.head,
            rounded[last],
            parts::heading(&rounded),
            theme.line,
            theme.node_fill,
        );
        parts::ending(
            scene,
            link.tail,
            rounded[0],
            parts::tail_heading(&rounded),
            theme.line,
            theme.node_fill,
        );
        spots[index] = (!link_labels[index].is_empty()).then(|| {
            let (u, v) = match route.len() {
                2 => ((route[0].0 + route[1].0) / 2.0, (route[0].1 + route[1].1) / 2.0),
                _ => (route[1].0, (route[1].1 + route[2].1) / 2.0),
            };
            plan.point(u, v)
        });
    }
    spots
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

fn draw_nodes(scene: &mut Scene, chart: &Chart, plan: &Plan, labels: &[Label], options: &Options) {
    let theme = &options.theme;
    let style = parts::text_style(options, 1.0, false, theme.text);
    for (index, node) in chart.nodes.iter().enumerate() {
        let rect = plan.rect(plan.nodes[index]);
        node.shape.draw(scene, rect, theme.node_fill, Stroke::new(theme.node_stroke, parts::LINE));
        if node.shape.shows_label() {
            parts::centred_label(scene, &labels[index], node.shape.text_area(rect), &style);
        }
    }
}

/// A link's label on a small panel, so the line behind it does not run through the words.
fn draw_link_label(scene: &mut Scene, label: &Label, at: Point, options: &Options) {
    let panel = Rect::around(at, Size::new(label.width + 8.0, label.height + 2.0));
    scene.add(Item::Rect {
        rect: panel,
        radius: 3.0,
        fill: Some(Paint::solid(options.theme.node_fill.color)),
        stroke: None,
    });
    parts::centred_label(
        scene,
        label,
        panel,
        &parts::text_style(options, 0.85, false, options.theme.dim),
    );
}

#[cfg(test)]
mod tests {
    use super::super::{check, Options};
    use super::*;
    use crate::metrics::FixedMetrics;

    fn options() -> Options<'static> {
        Options::new(Box::leak(Box::new(FixedMetrics::default())))
    }

    const SUPPORT: &str = "swimlane-beta LR\n  subgraph Customer\n    request[Request service]\n    receive[Receive update]\n  end\n  subgraph Support\n    triage[Triage request]\n    answer[Send answer]\n  end\n  subgraph Engineering\n    investigate[Investigate issue]\n    fix[Prepare fix]\n  end\n  request --> triage\n  triage -->|Known issue| answer\n  triage -->|Needs code change| investigate\n  investigate --> fix --> answer\n  answer --> receive\n";

    /// Every node's rectangle, by its label, and every lane band, out of a drawn scene.
    fn plan_of(text: &str) -> (Plan, Chart) {
        let source = Source::read(text).expect("a diagram");
        let chart = flowchart::read(&source).expect("it reads");
        let (titles, lane_of) = lanes_of(&chart);
        let edges: Vec<(usize, usize)> = chart.links.iter().map(|(a, b, _)| (*a, *b)).collect();
        let rank = ranks(chart.nodes.len(), &edges);
        let options = options();
        let style = options.style(1.0, false);
        let labels: Vec<Label> = chart
            .nodes
            .iter()
            .map(|node| text::measure(&node.label, &style, options.metrics, text::WRAP))
            .collect();
        let link_labels: Vec<Label> = chart
            .links
            .iter()
            .map(|(_, _, link)| {
                text::measure(
                    &link.label,
                    &options.style(0.85, false),
                    options.metrics,
                    text::EDGE_WRAP,
                )
            })
            .collect();
        let usable = vec![true; chart.links.len()];
        let horizontal = matches!(chart.direction, Direction::Right | Direction::Left);
        (
            plan(
                &chart,
                &titles,
                &lane_of,
                &rank,
                &labels,
                &link_labels,
                &usable,
                &options,
                horizontal,
            ),
            chart,
        )
    }

    #[test]
    fn every_lane_runs_the_whole_length_and_holds_its_own_nodes() {
        let (plan, chart) = plan_of(SUPPORT);
        assert_eq!(plan.lanes.len(), 3);
        for lane in &plan.lanes {
            let rect = plan.rect(*lane);
            assert_eq!(rect.width, plan.rect_total().width, "a lane is as long as the diagram");
        }
        let (_, lane_of) = lanes_of(&chart);
        for (index, node) in plan.nodes.iter().enumerate() {
            let lane = plan.rect(plan.lanes[lane_of[index]]);
            let rect = plan.rect(*node);
            assert!(
                lane.contains(Point::new(rect.left(), rect.top()))
                    && lane.contains(Point::new(rect.right(), rect.bottom())),
                "{} is outside its lane",
                chart.nodes[index].label
            );
        }
        check::no_two_rectangles_overlap(
            &plan.nodes.iter().map(|node| plan.rect(*node)).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn a_node_comes_after_everything_that_leads_to_it() {
        let (plan, chart) = plan_of(SUPPORT);
        let find = |label: &str| {
            plan.rect(plan.nodes[chart.nodes.iter().position(|node| node.label == label).unwrap()])
        };
        assert!(find("Request service").right() < find("Triage request").left());
        assert!(
            find("Prepare fix").right() < find("Send answer").left(),
            "the answer waits for the fix"
        );
        assert!(find("Send answer").right() < find("Receive update").left());
    }

    #[test]
    fn a_cycle_is_broken_rather_than_ranked_for_ever() {
        let rank = ranks(3, &[(0, 1), (1, 2), (2, 0)]);
        assert_eq!(rank, vec![0, 1, 2]);
    }

    #[test]
    fn top_to_bottom_puts_the_lanes_side_by_side() {
        let (plan, _) = plan_of("swimlane-beta\n subgraph Sales\n lead[Qualify lead]\n quote[Prepare quote]\n end\n subgraph Finance\n review[Review terms]\n end\n lead --> quote --> review\n");
        let first = plan.rect(plan.lanes[0]);
        let second = plan.rect(plan.lanes[1]);
        assert!(first.right() <= second.left() + 0.5, "{first:?} {second:?}");
        assert_eq!(first.height, second.height);
    }

    #[test]
    fn the_documentation_example_draws_with_every_word() {
        check::drawn(
            SUPPORT,
            &options(),
            &[
                "Customer",
                "Support",
                "Engineering",
                "Triage request",
                "Known issue",
                "Needs code change",
            ],
        );
    }

    #[test]
    fn right_to_left_and_a_backward_line_still_draw() {
        check::drawn(
            "swimlane-beta RL\n subgraph Intake\n start([Start])\n task[Do work]\n fix[Fix issues]\n end\n subgraph Review\n decision{Ready?}\n end\n start --> task --> decision\n decision -->|No| fix\n fix --> task\n",
            &options(),
            &["Start", "Do work", "Ready?", "No"],
        );
    }
}
