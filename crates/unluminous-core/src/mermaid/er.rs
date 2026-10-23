//! Entity relationship diagrams: `erDiagram`.
//!
//! An entity is a titled table of attributes; a relationship is a line whose two ends carry the
//! crow's foot notation. The boxes go through the shared layered layout.
//!
//! ## The cardinality markers, which are the whole point of the notation
//!
//! Each end of a relationship carries two marks: the **maximum** on the outside and the **minimum**
//! on the inside. So `}o--||` reads, from the left, "zero or more" and, from the right, "exactly
//! one". Four pairs exist and the left-hand and right-hand spellings are mirror images:
//!
//! | Left | Right | Means |
//! |---|---|---|
//! | `\|o` | `o\|` | zero or one |
//! | `\|\|` | `\|\|` | exactly one |
//! | `}o` | `o{` | zero or more |
//! | `}\|` | `\|{` | one or more |
//!
//! They are drawn rather than lettered, which is what a reader of one of these expects: a bar for
//! "one", a circle for "zero", and a crow's foot for "many".
//!
//! The line between them is solid when the relationship is **identifying** — the child cannot exist
//! without the parent — and dashed when it is not. That is `--` against `..`.

use std::collections::HashMap;

use super::layered::{self, Direction, EdgeSpec};
use super::parts::{self, Outline};
use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke};
use super::source::{self, Line, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// How far the marks at the end of a relationship reach back from the entity's edge.
const MARK: f32 = 16.0;
/// How tall one attribute row is, as a multiple of its font's line height.
const ROW_SPACING: f32 = 1.4;

/// How many there may be at one end of a relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Count {
    ZeroOrOne,
    ExactlyOne,
    ZeroOrMore,
    OneOrMore,
}

/// One attribute of an entity.
#[derive(Debug, Clone, PartialEq)]
struct Attribute {
    kind: String,
    name: String,
    /// `PK`, `FK`, `UK`, or nothing.
    key: String,
    comment: String,
}

#[derive(Debug, Clone, PartialEq)]
struct Entity {
    /// The name it is written as, which a subgraph of the same name stands for.
    id: String,
    name: String,
    attributes: Vec<Attribute>,
    /// The subgraph it was first mentioned in.
    group: Option<usize>,
}

/// A subgraph: a frame round the entities first mentioned inside it (`task-2339`).
#[derive(Debug, Clone, PartialEq)]
struct Group {
    /// The name a relationship refers to it by: `subgraph id[Title]` or `subgraph id`.
    id: String,
    title: String,
    parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct Relationship {
    from: usize,
    to: usize,
    from_count: Count,
    to_count: Count,
    /// True when the child cannot exist without the parent, which is drawn as a solid line.
    identifying: bool,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Diagram {
    entities: Vec<Entity>,
    by_name: HashMap<String, usize>,
    relationships: Vec<Relationship>,
    groups: Vec<Group>,
    direction: Direction,
    /// The subgraph the lines being read are inside, while they are read.
    current: Option<usize>,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    Ok(draw(&diagram, source, options))
}

fn read(source: &Source) -> Result<Diagram, Problem> {
    let mut diagram = Diagram {
        direction: Direction::parse(&source.header).unwrap_or(Direction::Down),
        ..Diagram::default()
    };
    let mut open: Option<usize> = None;
    let mut groups: Vec<usize> = Vec::new();
    let cleaned: Vec<Line> = source
        .statements()
        .into_iter()
        .map(|line| Line { text: without_classes(&line.text), ..line.clone() })
        .collect();
    for line in &cleaned {
        let text = line.text.trim();
        if text == "}" {
            open = None;
            continue;
        }
        if open.is_none() && line.starts_with_word("end") {
            groups.pop();
            diagram.current = groups.last().copied();
            continue;
        }
        if let Some(rest) = line.after_word("subgraph").filter(|_| open.is_none()) {
            diagram.groups.push(Group {
                id: group_id(rest),
                title: group_title(rest),
                parent: groups.last().copied(),
            });
            groups.push(diagram.groups.len() - 1);
            diagram.current = groups.last().copied();
            continue;
        }
        if let Some(entity) = open {
            if !text.is_empty() {
                read_attribute(&mut diagram.entities[entity], text);
            }
            continue;
        }
        if let Some(rest) = line.after_word("direction") {
            // Only the diagram's own `direction`: one inside a subgraph is about that subgraph, and
            // taking it turned a whole left to right diagram on its side (`task-2339`).
            if let Some(direction) = Direction::parse(rest).filter(|_| diagram.current.is_none()) {
                diagram.direction = direction;
            }
            continue;
        }
        if is_decoration(line) {
            continue;
        }
        if let Some(relationship) = read_relationship(&mut diagram, line)? {
            diagram.relationships.push(relationship);
            continue;
        }
        // `CUSTOMER {` opens a block of attributes, and `CUSTOMER["Buyer"]` renames one.
        if let Some(name) = text.strip_suffix('{') {
            open = Some(entity_of(&mut diagram, name.trim()));
            continue;
        }
        if !text.is_empty() {
            entity_of(&mut diagram, text);
        }
    }
    Ok(diagram)
}

/// A line with every `:::class` outside quotes taken off: `CAR:::someclass {` is `CAR {`.
fn without_classes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut quoted = false;
    let mut rest = text;
    while let Some(character) = rest.chars().next() {
        if character == '"' {
            quoted = !quoted;
        }
        if !quoted && rest.starts_with(":::") {
            let after = &rest[3..];
            let end = after
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == ','))
                .unwrap_or(after.len());
            rest = &after[end..];
            continue;
        }
        out.push(character);
        rest = &rest[character.len_utf8()..];
    }
    out
}

/// `subgraph title`, `subgraph "Customer Domain"` and `subgraph id["Title"]`.
/// The name a subgraph is referred to by: what comes before its brackets.
fn group_id(rest: &str) -> String {
    let rest = rest.trim();
    let id = rest.find('[').map_or(rest, |open| &rest[..open]);
    source::unquote(id).trim().to_owned()
}

fn group_title(rest: &str) -> String {
    match (rest.find('['), rest.rfind(']')) {
        (Some(open), Some(close)) if close > open => source::label(&rest[open + 1..close]),
        _ => source::label(rest),
    }
}

fn is_decoration(line: &Line) -> bool {
    ["style", "classDef", "class", "click"].iter().any(|word| line.starts_with_word(word))
}

/// Read `string name PK "the customer's name"`.
///
/// The type comes first and the name second; a `PK`, `FK` or `UK` may follow either, and a comment
/// in quotes comes last. Mermaid also allows a `?` on the type for "nullable", which is kept as part
/// of the type because that is what it means.
fn read_attribute(entity: &mut Entity, text: &str) {
    let (body, comment) = match text.find('"') {
        Some(open) => match text[open + 1..].find('"') {
            Some(close) => (&text[..open], text[open + 1..open + 1 + close].to_owned()),
            None => (text, String::new()),
        },
        None => (text, String::new()),
    };
    let mut words: Vec<&str> = body.split_whitespace().collect();
    let mut key = String::new();
    words.retain(|word| {
        let is_key = matches!(word.to_ascii_uppercase().as_str(), "PK" | "FK" | "UK");
        if is_key {
            if !key.is_empty() {
                key.push_str(", ");
            }
            key.push_str(&word.to_ascii_uppercase());
        }
        !is_key
    });
    let kind = words.first().copied().unwrap_or_default().to_owned();
    let name = words.get(1).copied().unwrap_or_default().to_owned();
    if kind.is_empty() && name.is_empty() {
        return;
    }
    entity.attributes.push(Attribute { kind, name, key, comment });
}

/// Find or make the entity a name refers to, reading `NAME["An alias"]` when it is written that way.
fn entity_of(diagram: &mut Diagram, text: &str) -> usize {
    let text = text.trim();
    let (id, shown) = match (text.find('['), text.rfind(']')) {
        (Some(open), Some(close)) if close > open => {
            (text[..open].trim(), source::label(&text[open + 1..close]))
        }
        _ => (text, source::label(text)),
    };
    // Mermaid sets an entity's name as markdown, so `**is**` and `_Markdown_` lose their marks
    // (`task-2339`).
    let shown = source::without_emphasis(&shown);
    let id = source::unquote(id).trim().to_owned();
    if let Some(&known) = diagram.by_name.get(&id) {
        if shown != id {
            diagram.entities[known].name = shown;
        }
        return known;
    }
    diagram.entities.push(Entity {
        id: id.clone(),
        name: shown,
        attributes: Vec::new(),
        group: diagram.current,
    });
    diagram.by_name.insert(id, diagram.entities.len() - 1);
    diagram.entities.len() - 1
}

/// The left-hand markers, longest first so `}o` is never read as `}` and a stray `o`.
///
/// Each side also takes the other side's spelling, `PERSON o{--|| HOUSE`, which Mermaid's own
/// documentation writes (`task-2339`).
const LEFT_COUNTS: &[(&str, Count)] = &[
    ("|o", Count::ZeroOrOne),
    ("||", Count::ExactlyOne),
    ("}o", Count::ZeroOrMore),
    ("}|", Count::OneOrMore),
    ("o|", Count::ZeroOrOne),
    ("o{", Count::ZeroOrMore),
    ("|{", Count::OneOrMore),
];
/// The right-hand markers, which are the mirror images.
const RIGHT_COUNTS: &[(&str, Count)] = &[
    ("o|", Count::ZeroOrOne),
    ("||", Count::ExactlyOne),
    ("o{", Count::ZeroOrMore),
    ("|{", Count::OneOrMore),
    ("|o", Count::ZeroOrOne),
    ("}o", Count::ZeroOrMore),
    ("}|", Count::OneOrMore),
];

/// Read `CUSTOMER ||--o{ ORDER : places`.
fn read_relationship(diagram: &mut Diagram, line: &Line) -> Result<Option<Relationship>, Problem> {
    let (head, label) = match line.text.split_once(':') {
        Some((head, label)) => (head, source::label(label)),
        None => (line.text.as_str(), String::new()),
    };
    let Some((at, length, identifying)) = find_line(head) else {
        return Ok(read_words(diagram, head, label));
    };
    let before = head[..at].trim_end();
    let after = head[at + length..].trim_start();
    let Some((from_name, from_count)) = take_count(before, LEFT_COUNTS, true) else {
        return Err(Problem::at(
            line,
            "the mark before the line is not one Unluminous knows. It should be one of `|o`, `||`, `}o` or `}|`.",
        ));
    };
    let Some((to_name, to_count)) = take_count(after, RIGHT_COUNTS, false) else {
        return Err(Problem::at(
            line,
            "the mark after the line is not one Unluminous knows. It should be one of `o|`, `||`, `o{` or `|{`.",
        ));
    };
    if from_name.trim().is_empty() || to_name.trim().is_empty() {
        return Err(Problem::at(line, "a relationship needs an entity at each end of it"));
    }
    let from = entity_of(diagram, from_name);
    let to = entity_of(diagram, to_name);
    Ok(Some(Relationship { from, to, from_count, to_count, identifying, label }))
}

/// The words a cardinality may be written as, and what each means (`task-2339`).
///
/// Longest first, so `one or more` is never read as a trailing `1`.
const WORDS: &[(&str, Count)] = &[
    ("zero or more", Count::ZeroOrMore),
    ("zero or many", Count::ZeroOrMore),
    ("one or more", Count::OneOrMore),
    ("one or many", Count::OneOrMore),
    ("one or zero", Count::ZeroOrOne),
    ("zero or one", Count::ZeroOrOne),
    ("only one", Count::ExactlyOne),
    ("many(1)", Count::OneOrMore),
    ("many(0)", Count::ZeroOrMore),
    ("1+", Count::OneOrMore),
    ("0+", Count::ZeroOrMore),
    ("1", Count::ExactlyOne),
];

/// `MANUFACTURER only one to zero or more CAR`, the relationship written in words: a cardinality,
/// `to` (identifying) or `optionally to` (not), and another cardinality.
fn read_words(diagram: &mut Diagram, head: &str, label: String) -> Option<Relationship> {
    let (left, right, identifying) = match head.find(" optionally to ") {
        Some(at) => (&head[..at], &head[at + " optionally to ".len()..], false),
        None => {
            let at = head.find(" to ")?;
            (&head[..at], &head[at + " to ".len()..], true)
        }
    };
    let lower_left = left.to_ascii_lowercase();
    let lower_right = right.trim_start().to_ascii_lowercase();
    let (from_name, from_count) = WORDS.iter().find_map(|(words, count)| {
        let rest = lower_left.strip_suffix(words)?;
        rest.ends_with(' ').then(|| (left[..rest.len()].trim(), *count))
    })?;
    let right = right.trim_start();
    let (to_name, to_count) = WORDS.iter().find_map(|(words, count)| {
        let rest = lower_right.strip_prefix(words)?;
        rest.starts_with(' ').then(|| (right[words.len()..].trim(), *count))
    })?;
    if from_name.is_empty() || to_name.is_empty() {
        return None;
    }
    let from = entity_of(diagram, from_name);
    let to = entity_of(diagram, to_name);
    Some(Relationship { from, to, from_count, to_count, identifying, label })
}

/// Where the `--` or `..` is, how long it is, and whether it is the identifying kind.
fn find_line(head: &str) -> Option<(usize, usize, bool)> {
    let bytes = head.as_bytes();
    for at in 0..bytes.len().saturating_sub(1) {
        if (bytes[at] == b'-' || bytes[at] == b'.') && bytes[at + 1] == bytes[at] {
            let mark = bytes[at];
            let mut end = at;
            while end < bytes.len() && bytes[end] == mark {
                end += 1;
            }
            return Some((at, end - at, mark == b'-'));
        }
    }
    None
}

/// Take a cardinality marker off the inside end of a piece of text.
fn take_count<'a>(
    text: &'a str,
    counts: &[(&str, Count)],
    at_the_end: bool,
) -> Option<(&'a str, Count)> {
    for (mark, count) in counts {
        let found = if at_the_end { text.strip_suffix(mark) } else { text.strip_prefix(mark) };
        if let Some(rest) = found {
            return Some((rest.trim(), *count));
        }
    }
    // Mermaid also accepts the words — `one or more`, `zero or one` — but only in the newer form,
    // and a relationship with no marks at all is still a relationship.
    text.is_empty().then_some((text, Count::ExactlyOne)).or(Some((text, Count::ExactlyOne)))
}

/// One entity box, measured.
struct Measured {
    title: Label,
    rows: Vec<(Label, Label, Label)>,
    size: Size,
    /// Where the line under the title goes, from the top of the box.
    title_height: f32,
    /// How wide each of the three columns is.
    columns: [f32; 3],
}

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let boxes: Vec<Measured> =
        diagram.entities.iter().map(|entity| measure(entity, options)).collect();
    let label_style = options.style(0.85, false);
    let labels: Vec<Label> = diagram
        .relationships
        .iter()
        .map(|r| text::measure(&r.label, &label_style, options.metrics, text::EDGE_WRAP))
        .collect();

    let mut graph = layered::Graph { direction: diagram.direction, ..layered::Graph::default() };
    let title_style = options.style(0.95, true);
    for group in &diagram.groups {
        let title = text::measure_unwrapped(&group.title, &title_style, options.metrics);
        graph.groups.push(layered::GroupSpec {
            title: Size::new(title.width, title.height + 6.0),
            parent: group.parent,
        });
    }
    // **A subgraph's name is the subgraph**: `title1 ||--|| title2` joins two frames, which is
    // Mermaid's own example, and an entity of that name is not drawn a second time (`task-2339`).
    let ends = Ends::of(diagram);
    for (index, measured) in boxes.iter().enumerate() {
        if ends.node[index].is_some() {
            graph.add_node(measured.size, diagram.entities[index].group);
        }
    }
    for (index, relationship) in diagram.relationships.iter().enumerate() {
        // The room asked for is the label **plus both sets of markers**. The crow's foot, the bar
        // and the circle all reach back from the entity's edge, so a gap sized for the words alone
        // leaves the two ends' markers touching each other in the middle.
        let wanted = Size::new(labels[index].width + MARK * 4.0, labels[index].height + MARK * 2.0);
        let (from, from_group) = ends.end(relationship.from);
        let (to, to_group) = ends.end(relationship.to);
        graph.edges.push(EdgeSpec { from, to, label: wanted, span: 1, from_group, to_group });
    }
    let placed = layered::layout(&graph);

    let mut scene = Scene::new();
    let top = parts::title(&mut scene, source, options, placed.size.width);
    let origin = Point::new(parts::MARGIN, top + parts::MARGIN);
    draw_groups(&mut scene, diagram, &placed, origin, options, false);
    draw_relationships(&mut scene, diagram, &ends, &placed, origin, &labels, options);
    // The titles go on after the lines, on a panel, because a line into a subgraph crosses the top
    // of its frame where the title is.
    draw_groups(&mut scene, diagram, &placed, origin, options, true);
    for (index, entity) in diagram.entities.iter().enumerate() {
        let Some(node) = ends.node[index] else {
            continue;
        };
        draw_entity(
            &mut scene,
            entity,
            &boxes[index],
            placed.nodes[node].moved(origin.x, origin.y),
            options,
        );
    }
    parts::finish(&mut scene);
    scene
}

/// Where each entity is in the laid out graph: a node of its own, or the frame of the subgraph of the
/// same name.
struct Ends {
    absorbed: Vec<Option<usize>>,
    node: Vec<Option<usize>>,
}

impl Ends {
    fn of(diagram: &Diagram) -> Ends {
        let absorbed: Vec<Option<usize>> = diagram
            .entities
            .iter()
            .map(|entity| {
                // Only an entity with nothing of its own; one with attributes is a real table.
                if !entity.attributes.is_empty() {
                    return None;
                }
                diagram.groups.iter().position(|group| group.id == entity.id)
            })
            .collect();
        let mut next = 0;
        let node = absorbed
            .iter()
            .map(|group| {
                group.is_none().then(|| {
                    next += 1;
                    next - 1
                })
            })
            .collect();
        Ends { absorbed, node }
    }

    fn end(&self, entity: usize) -> (usize, bool) {
        match (self.absorbed[entity], self.node[entity]) {
            (Some(group), _) => (group, true),
            (None, Some(node)) => (node, false),
            (None, None) => (0, false),
        }
    }

    fn rect(&self, placed: &layered::Placed, entity: usize, origin: Point) -> Rect {
        match self.end(entity) {
            (group, true) => placed.groups[group].moved(origin.x, origin.y),
            (node, false) => placed.nodes[node].moved(origin.x, origin.y),
        }
    }
}

/// Each subgraph's frame, or with `titles` each one's title on a panel of its own.
fn draw_groups(
    scene: &mut Scene,
    diagram: &Diagram,
    placed: &layered::Placed,
    origin: Point,
    options: &Options,
    titles: bool,
) {
    let theme = &options.theme;
    for (index, group) in diagram.groups.iter().enumerate() {
        let frame = placed.groups[index].moved(origin.x, origin.y);
        if frame.width <= 0.0 {
            continue;
        }
        if !titles {
            scene.add(Item::Rect {
                rect: frame,
                radius: parts::CORNER,
                fill: Some(theme.group_fill),
                stroke: Some(Stroke::new(theme.group_stroke, parts::LINE)),
            });
            continue;
        }
        let style = options.style(0.95, true);
        let width = text::width_of(&group.title, &style, options.metrics);
        let height = options.metrics.line_metrics(&style).height();
        let at = Point::new(frame.left() + 12.0, frame.top() + 6.0);
        scene.add(Item::Rect {
            rect: Rect::new(at.x - 4.0, at.y - 1.0, width + 8.0, height + 2.0),
            radius: 3.0,
            fill: Some(Paint::solid(theme.node_fill.color)),
            stroke: None,
        });
        parts::one_line(
            scene,
            &group.title,
            at,
            &parts::text_style(options, 0.95, true, theme.text),
            Anchor::Start,
            width,
        );
    }
}

/// Measure an entity's box: its title and its three columns of attributes.
fn measure(entity: &Entity, options: &Options) -> Measured {
    let title_style = options.style(1.0, true);
    let row_style = options.style(0.82, false);
    let title = text::measure_unwrapped(&entity.name, &title_style, options.metrics);
    let rows: Vec<(Label, Label, Label)> = entity
        .attributes
        .iter()
        .map(|attribute| {
            let last = if attribute.comment.is_empty() {
                attribute.key.clone()
            } else if attribute.key.is_empty() {
                attribute.comment.clone()
            } else {
                format!("{} {}", attribute.key, attribute.comment)
            };
            (
                text::measure_unwrapped(&attribute.kind, &row_style, options.metrics),
                text::measure_unwrapped(&attribute.name, &row_style, options.metrics),
                text::measure_unwrapped(&last, &row_style, options.metrics),
            )
        })
        .collect();
    let columns = [
        rows.iter().map(|row| row.0.width).fold(0.0_f32, f32::max),
        rows.iter().map(|row| row.1.width).fold(0.0_f32, f32::max),
        rows.iter().map(|row| row.2.width).fold(0.0_f32, f32::max),
    ];
    let gap = 14.0;
    let across = columns.iter().sum::<f32>() + gap * 2.0;
    let title_height = title.height + parts::PADDING_Y * 2.0;
    let row_height = row_style.size * ROW_SPACING;
    let body =
        if rows.is_empty() { 0.0 } else { row_height * rows.len() as f32 + parts::PADDING_Y * 2.0 };
    Measured {
        size: Size::new(across.max(title.width) + parts::PADDING_X * 2.0, title_height + body),
        title,
        rows,
        title_height,
        columns,
    }
}

/// Draw one entity: the box, the title bar, and a row per attribute.
fn draw_entity(
    scene: &mut Scene,
    entity: &Entity,
    measured: &Measured,
    rect: Rect,
    options: &Options,
) {
    let theme = &options.theme;
    let stroke = Stroke::new(theme.node_stroke, parts::LINE);
    scene.add(Item::Rect {
        rect,
        radius: parts::CORNER,
        fill: Some(theme.node_fill),
        stroke: Some(stroke),
    });
    parts::one_line(
        scene,
        &entity.name,
        Point::new(rect.centre().x, rect.top() + parts::PADDING_Y),
        &parts::text_style(options, 1.0, true, theme.text),
        Anchor::Middle,
        measured.title.width,
    );
    if measured.rows.is_empty() {
        return;
    }
    let divider = rect.top() + measured.title_height;
    scene.add(Item::Line {
        points: vec![Point::new(rect.left(), divider), Point::new(rect.right(), divider)],
        stroke,
        dash: Dash::Solid,
    });
    let row_height = options.base.size * 0.82 * ROW_SPACING;
    let kind_style = parts::text_style(options, 0.82, false, theme.dim);
    let name_style = parts::text_style(options, 0.82, false, theme.text);
    for (index, attribute) in entity.attributes.iter().enumerate() {
        let y = divider + parts::PADDING_Y + row_height * index as f32;
        let mut x = rect.left() + parts::PADDING_X;
        let (kind, name, last) = &measured.rows[index];
        parts::one_line(
            scene,
            &attribute.kind,
            Point::new(x, y),
            &kind_style,
            Anchor::Start,
            kind.width,
        );
        x += measured.columns[0] + 14.0;
        parts::one_line(
            scene,
            &attribute.name,
            Point::new(x, y),
            &name_style,
            Anchor::Start,
            name.width,
        );
        x += measured.columns[1] + 14.0;
        let words = if attribute.comment.is_empty() {
            attribute.key.clone()
        } else if attribute.key.is_empty() {
            attribute.comment.clone()
        } else {
            format!("{} {}", attribute.key, attribute.comment)
        };
        if !words.is_empty() {
            parts::one_line(
                scene,
                &words,
                Point::new(x, y),
                &kind_style,
                Anchor::Start,
                last.width,
            );
        }
    }
}

/// Draw every relationship: the line, the marks at each end, and the label.
fn draw_relationships(
    scene: &mut Scene,
    diagram: &Diagram,
    ends: &Ends,
    placed: &layered::Placed,
    origin: Point,
    labels: &[Label],
    options: &Options,
) {
    let theme = &options.theme;
    for (index, relationship) in diagram.relationships.iter().enumerate() {
        let path: Vec<Point> = placed.edges[index]
            .iter()
            .map(|point| Point::new(point.x + origin.x, point.y + origin.y))
            .collect();
        if path.len() < 2 {
            continue;
        }
        // The marks are laid along the line as it leaves the box, which is the straight first and
        // last stretch before the corners are rounded. Read off the rounded line, the first point
        // after the border can already be round the bend, and the marks leant with it.
        let sharp = parts::clipped(
            &path,
            &Outline::Rect(ends.rect(placed, relationship.from, origin)),
            &Outline::Rect(ends.rect(placed, relationship.to, origin)),
        );
        let path = parts::rounded(&sharp, parts::BEND, MARK);
        let sharp_last = sharp.len() - 1;
        let stroke = Stroke::new(theme.line, parts::LINE);
        let dash = if relationship.identifying { Dash::Solid } else { parts::DASH };
        scene.add(Item::Line { points: path.clone(), stroke, dash });
        draw_count(scene, relationship.from_count, sharp[0], sharp[1], stroke);
        draw_count(scene, relationship.to_count, sharp[sharp_last], sharp[sharp_last - 1], stroke);
        if labels[index].is_empty() {
            continue;
        }
        let at = placed.labels[index];
        let panel = Rect::around(
            Point::new(at.x + origin.x, at.y + origin.y),
            Size::new(labels[index].width + 8.0, labels[index].height + 2.0),
        );
        scene.add(Item::Rect {
            rect: panel,
            radius: 3.0,
            fill: Some(Paint::solid(theme.node_fill.color)),
            stroke: None,
        });
        parts::centred_label(
            scene,
            &labels[index],
            panel,
            &parts::text_style(options, 0.85, false, theme.dim),
        );
    }
}

/// Draw the crow's foot notation at one end of a relationship.
///
/// `at` is the point on the entity's border and `towards` is the next point along the line, so the
/// marks are laid out along the line rather than always horizontally.
fn draw_count(scene: &mut Scene, count: Count, at: Point, towards: Point, stroke: Stroke) {
    let length = at.distance(towards).max(1.0);
    let along = Point::new((towards.x - at.x) / length, (towards.y - at.y) / length);
    let across = Point::new(-along.y, along.x);
    let step = |distance: f32| Point::new(at.x + along.x * distance, at.y + along.y * distance);
    let bar = |scene: &mut Scene, distance: f32, half: f32| {
        let middle = step(distance);
        scene.add(Item::Line {
            points: vec![
                Point::new(middle.x - across.x * half, middle.y - across.y * half),
                Point::new(middle.x + across.x * half, middle.y + across.y * half),
            ],
            stroke,
            dash: Dash::Solid,
        });
    };
    let foot = |scene: &mut Scene| {
        // Three lines from one point on the line out to the entity's edge, which is the crow's foot.
        let back = step(MARK);
        for offset in [-7.0, 0.0, 7.0] {
            scene.add(Item::Line {
                points: vec![back, Point::new(at.x + across.x * offset, at.y + across.y * offset)],
                stroke,
                dash: Dash::Solid,
            });
        }
    };
    match count {
        Count::ExactlyOne => {
            bar(scene, MARK * 0.35, 7.0);
            bar(scene, MARK * 0.75, 7.0);
        }
        Count::ZeroOrOne => {
            bar(scene, MARK * 0.4, 7.0);
            scene.add(Item::Circle {
                centre: step(MARK * 0.85),
                radius: 4.5,
                fill: None,
                stroke: Some(stroke),
            });
        }
        Count::OneOrMore => {
            foot(scene);
            bar(scene, MARK * 1.15, 7.0);
        }
        Count::ZeroOrMore => {
            foot(scene);
            scene.add(Item::Circle {
                centre: step(MARK * 1.3),
                radius: 4.5,
                fill: None,
                stroke: Some(stroke),
            });
        }
    }
}

#[cfg(test)]
mod words_and_subgraphs {
    use super::*;

    fn diagram(text: &str) -> Diagram {
        read(&Source::read(text).expect("a diagram")).expect("it should read")
    }

    #[test]
    fn a_relationship_to_a_subgraph_s_name_joins_the_frame() {
        let text = "erDiagram\n subgraph title1\n  A1 ||--|| A2 : links\n end\n subgraph title2\n  B1 ||--|| B2 : links\n end\n title1 ||--|| title2 : links\n \"This **is** _Markdown_\"\n";
        let diagram = diagram(text);
        let ends = Ends::of(&diagram);
        assert_eq!(ends.end(diagram.by_name["title1"]), (0, true));
        assert_eq!(ends.end(diagram.by_name["title2"]), (1, true));
        assert_eq!(diagram.entities.last().unwrap().name, "This is Markdown");
        let options = Options::new(Box::leak(Box::new(crate::metrics::FixedMetrics::default())));
        let scene = super::super::check::drawn(text, &options, &["A1", "B2", "title1", "title2"]);
        assert_eq!(scene.texts().iter().filter(|text| **text == "title1").count(), 1);
    }

    #[test]
    fn a_direction_inside_a_subgraph_leaves_the_diagram_s_alone() {
        let diagram = diagram(
            "erDiagram\n direction LR\n subgraph TOP\n  direction TB\n  A ||--|| B : links\n end\n",
        );
        assert_eq!(diagram.direction, Direction::Right);
    }

    #[test]
    fn a_relationship_written_in_words_is_read() {
        let diagram = diagram(
            "erDiagram\n MANUFACTURER only one to zero or more CAR : makes\n PERSON many(0) optionally to 0+ NAMED-DRIVER : is\n",
        );
        let first = &diagram.relationships[0];
        assert_eq!(diagram.entities[first.from].name, "MANUFACTURER");
        assert_eq!(diagram.entities[first.to].name, "CAR");
        assert_eq!((first.from_count, first.to_count), (Count::ExactlyOne, Count::ZeroOrMore));
        assert!(first.identifying);
        let second = &diagram.relationships[1];
        assert!(!second.identifying);
        assert_eq!(second.to_count, Count::ZeroOrMore);
        assert_eq!(diagram.entities.len(), 4, "the words are not entities");
    }

    #[test]
    fn a_subgraph_holds_the_entities_first_mentioned_in_it() {
        let diagram = diagram(
            "erDiagram\n subgraph title1\n  CUSTOMER\n end\n subgraph title2\n  CAR ||--o{ NAMED-DRIVER : allows\n  subgraph \"Person Domain\"\n   PERSON\n  end\n end\n",
        );
        assert_eq!(diagram.groups.len(), 3);
        assert_eq!(diagram.groups[2].title, "Person Domain");
        assert_eq!(diagram.groups[2].parent, Some(1));
        assert_eq!(diagram.entities[0].group, Some(0));
        assert_eq!(diagram.entities[1].group, Some(1));
        assert_eq!(diagram.entities[3].group, Some(2));
        assert!(!diagram.entities.iter().any(|entity| entity.name == "end"));
    }

    #[test]
    fn a_class_on_an_entity_is_read_and_ignored() {
        let diagram = diagram("erDiagram\n CAR:::someclass {\n string model\n }\n PERSON:::foo ||--|| CAR : owns\n PERSON o{--|| HOUSE:::bar : has\n");
        let names: Vec<&str> = diagram.entities.iter().map(|entity| entity.name.as_str()).collect();
        assert_eq!(names, vec!["CAR", "PERSON", "HOUSE"]);
        assert_eq!(diagram.relationships[1].label, "has");
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
    fn all_four_cardinalities_are_read_on_both_sides() {
        let text = "erDiagram\n\
            A |o--o| B\n C ||--|| D\n E }o--o{ F\n G }|--|{ H\n";
        let diagram = diagram(text);
        let counts: Vec<(Count, Count)> =
            diagram.relationships.iter().map(|r| (r.from_count, r.to_count)).collect();
        assert_eq!(
            counts,
            vec![
                (Count::ZeroOrOne, Count::ZeroOrOne),
                (Count::ExactlyOne, Count::ExactlyOne),
                (Count::ZeroOrMore, Count::ZeroOrMore),
                (Count::OneOrMore, Count::OneOrMore),
            ]
        );
    }

    #[test]
    fn a_dotted_line_is_the_non_identifying_kind() {
        let diagram = diagram("erDiagram\n A ||--o{ B : has\n C ||..o{ D : maybe\n");
        assert!(diagram.relationships[0].identifying);
        assert!(!diagram.relationships[1].identifying);
        assert_eq!(diagram.relationships[0].label, "has");
    }

    #[test]
    fn an_attribute_block_is_read_into_type_name_key_and_comment() {
        let text = "erDiagram\n CUSTOMER {\n string name PK \"what they are called\"\n int age\n string email UK\n }\n";
        let diagram = diagram(text);
        let attributes = &diagram.entities[0].attributes;
        assert_eq!(attributes.len(), 3);
        assert_eq!(attributes[0].kind, "string");
        assert_eq!(attributes[0].name, "name");
        assert_eq!(attributes[0].key, "PK");
        assert_eq!(attributes[0].comment, "what they are called");
        assert_eq!(attributes[1].key, "");
        assert_eq!(attributes[2].key, "UK");
    }

    #[test]
    fn an_alias_is_shown_instead_of_the_name() {
        let diagram = diagram("erDiagram\n CUSTOMER[\"The buyer\"] ||--o{ ORDER : places\n");
        assert_eq!(diagram.entities[0].name, "The buyer");
        assert!(diagram.by_name.contains_key("CUSTOMER"), "it is still found by its real name");
    }

    #[test]
    fn an_er_diagram_is_drawn_and_keeps_every_property() {
        let text = "erDiagram\n\
            CUSTOMER ||--o{ ORDER : places\n\
            ORDER ||--|{ LINE_ITEM : contains\n\
            CUSTOMER }|..|{ DELIVERY_ADDRESS : uses\n\
            CUSTOMER {\n string name PK\n string email\n }\n\
            ORDER {\n int number PK\n date placed\n }\n";
        let scene = check::drawn(
            text,
            &options(),
            &[
                "CUSTOMER",
                "ORDER",
                "LINE_ITEM",
                "DELIVERY_ADDRESS",
                "places",
                "contains",
                "name",
                "email",
            ],
        );
        check::no_two_rectangles_overlap(
            &scene.rects().into_iter().filter(|rect| rect.height > 20.0).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn a_relationship_with_nothing_on_one_end_says_which_line() {
        let problem = check::refused("erDiagram\n A ||--o{ B : ok\n ||--o{ C : bad\n", &options());
        assert_eq!(problem.line, Some(3));
    }

    #[test]
    fn an_entity_with_no_attributes_is_still_a_box_with_its_name_in_it() {
        let scene = check::drawn("erDiagram\n LONE\n", &options(), &["LONE"]);
        assert!(!scene.rects().is_empty());
    }
}
