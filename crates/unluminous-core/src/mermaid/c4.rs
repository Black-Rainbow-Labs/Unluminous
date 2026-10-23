//! C4 diagrams: `C4Context`, `C4Container`, `C4Component`, `C4Dynamic` and `C4Deployment`.
//!
//! The notation of Simon Brown's C4 model, written the way C4-PlantUML writes it: every statement is a
//! call such as `Person(customer, "Customer", "A customer of the bank")`, a boundary is a call followed
//! by `{` with its members inside, and `Rel(a, b, "Uses", "HTTPS")` joins two of them.
//!
//! ## Laid out by the layered layout, not by Mermaid's rows
//!
//! Mermaid places C4 shapes in rows of four in the order they were written and draws every relation
//! as a straight line from one shape to the other, so its pictures regularly have lines through boxes
//! and labels drawn over each other; its own documentation answers that with `UpdateRelStyle` offsets
//! typed in by hand. `task-2339` asks for exactly the opposite of that, so elements are nodes,
//! boundaries are groups and relations are labelled edges in the layered layout the flowchart uses,
//! which keeps every box, label and frame apart by construction. `Rel_U` and `Rel_L` turn the edge
//! round for the layering so the target lands above, and the offsets and the layout statements are
//! read and ignored.
//!
//! ## What a box says
//!
//! C4's own three parts, centred: the kind in guillemets, the name in bold with the technology in
//! brackets under it, and the description. Elements are filled by their level in the model, strongest
//! for a person or a system and lighter for a container and lighter again for a component, and grey
//! when they are external, which is how C4 is drawn everywhere. The colours are worked out from the
//! theme so a light theme and a dark one both read.

use std::collections::HashMap;

use super::layered::{self, Direction, EdgeSpec, GroupSpec};
use super::parts::{self, Ending, Outline};
use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke};
use super::shapes::Shape;
use super::source::{self, Line, Source};
use super::text::{self, Label};
use super::{Options, Problem};
use crate::style::Color;

/// The narrowest an element's box is, so a short name still makes a box that reads as C4.
const SMALLEST_WIDTH: f32 = 170.0;
/// The space between the three parts of an element's words.
const GAP: f32 = 4.0;
/// How wide an element's words may get before they wrap.
const ELEMENT_WRAP: f32 = 210.0;

/// Where an element sits in the C4 model, which decides its colour and the word above its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Person,
    System,
    Container,
    Component,
    Node,
}

/// The outline an element is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Box,
    Database,
    Queue,
    Person,
}

#[derive(Debug, Clone, PartialEq)]
struct Element {
    alias: String,
    label: String,
    technology: String,
    description: String,
    level: Level,
    form: Form,
    external: bool,
    group: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct Boundary {
    alias: String,
    label: String,
    /// The line in brackets under the name: `[System]`, `[Enterprise]`, a deployment node's type.
    kind: String,
    /// A deployment node is a solid frame and every other boundary a dashed one.
    deployment: bool,
    parent: Option<usize>,
}

/// One end of a relation: an element or a boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Element(usize),
    Boundary(usize),
}

#[derive(Debug, Clone, PartialEq)]
struct Relation {
    from: End,
    to: End,
    label: String,
    technology: String,
    /// `BiRel`: an arrow at both ends.
    both: bool,
    /// `Rel_Back`: the arrow is at the first element rather than the second.
    back: bool,
    /// `Rel_U` and `Rel_L`: the second element is laid out above the first.
    upward: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Diagram {
    elements: Vec<Element>,
    boundaries: Vec<Boundary>,
    relations: Vec<Relation>,
    title: Option<String>,
    /// `C4Dynamic`, whose relations are numbered in the order they were written.
    numbered: bool,
    /// Every element and boundary with the boundary it is in, in the order they were written.
    members: Vec<(Option<usize>, End)>,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    Ok(draw(&diagram, source, options))
}

/// A statement split into its name and its arguments: `Rel(a, b, "Uses")`.
#[derive(Debug, Clone, PartialEq)]
struct Call {
    name: String,
    /// The arguments with no `$name=` in front of them, in order, quotes and markup taken off.
    positional: Vec<String>,
    /// The `$name="value"` arguments, by name without the `$`.
    named: HashMap<String, String>,
    /// True when the call is followed by `{`, which makes it a boundary holding what comes next.
    opens: bool,
}

impl Call {
    /// The argument at `index`, or the one named `name`, or nothing.
    fn argument(&self, index: usize, name: &str) -> String {
        self.named
            .get(name)
            .cloned()
            .or_else(|| self.positional.get(index).cloned())
            .unwrap_or_default()
    }
}

/// Read a statement as a call, or `None` when it is not one.
fn read_call(text: &str) -> Option<Call> {
    let open = text.find('(')?;
    let name = text[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let close = closing_bracket(text, open)?;
    let inner = &text[open + 1..close];
    let after = text[close + 1..].trim();
    let mut positional = Vec::new();
    let mut named = HashMap::new();
    for argument in source::split_outside_quotes(inner, ',') {
        let argument = argument.trim();
        if let Some(rest) = argument.strip_prefix('$') {
            if let Some((key, value)) = rest.split_once('=') {
                named.insert(key.trim().to_ascii_lowercase(), source::label(value));
            }
            continue;
        }
        positional.push(source::label(argument));
    }
    Some(Call { name: name.to_owned(), positional, named, opens: after.starts_with('{') })
}

/// Where the bracket that closes the one at `open` is, skipping anything in quotes.
fn closing_bracket(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0_i32;
    let mut quoted = false;
    for (at, character) in text.char_indices().skip_while(|(at, _)| *at < open) {
        match character {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// What a call names, once its name has been looked up.
enum Statement {
    Element(Level, Form, bool),
    Boundary(&'static str),
    /// A deployment node, which is a boundary when it holds something and an element when not.
    Deployment,
    Relation {
        both: bool,
        back: bool,
        upward: bool,
        indexed: bool,
    },
    Ignored,
}

/// Look a call's name up. Matched without regard to case, as Mermaid's own keywords are elsewhere.
fn statement(name: &str) -> Option<Statement> {
    let lower = name.to_ascii_lowercase();
    let (stem, external) = match lower.strip_suffix("_ext") {
        Some(stem) => (stem.to_owned(), true),
        None => (lower.clone(), false),
    };
    let element = |level, form| Some(Statement::Element(level, form, external));
    match stem.as_str() {
        "person" => element(Level::Person, Form::Person),
        "system" => element(Level::System, Form::Box),
        "systemdb" => element(Level::System, Form::Database),
        "systemqueue" => element(Level::System, Form::Queue),
        "container" => element(Level::Container, Form::Box),
        "containerdb" => element(Level::Container, Form::Database),
        "containerqueue" => element(Level::Container, Form::Queue),
        "component" => element(Level::Component, Form::Box),
        "componentdb" => element(Level::Component, Form::Database),
        "componentqueue" => element(Level::Component, Form::Queue),
        "boundary" => Some(Statement::Boundary("")),
        "enterprise_boundary" => Some(Statement::Boundary("Enterprise")),
        "system_boundary" => Some(Statement::Boundary("System")),
        "container_boundary" => Some(Statement::Boundary("Container")),
        "deployment_node" | "node" | "node_l" | "node_r" => Some(Statement::Deployment),
        "rel" | "rel_d" | "rel_down" | "rel_r" | "rel_right" => {
            Some(Statement::Relation { both: false, back: false, upward: false, indexed: false })
        }
        "rel_u" | "rel_up" | "rel_l" | "rel_left" => {
            Some(Statement::Relation { both: false, back: false, upward: true, indexed: false })
        }
        "birel" | "birel_d" | "birel_u" | "birel_l" | "birel_r" => {
            Some(Statement::Relation { both: true, back: false, upward: false, indexed: false })
        }
        "rel_back" => {
            Some(Statement::Relation { both: false, back: true, upward: false, indexed: false })
        }
        "relindex" => {
            Some(Statement::Relation { both: false, back: false, upward: false, indexed: true })
        }
        // Colours, hand placement, legends and tags: the document's business is the model, and the
        // window's is how it looks.
        "updateelementstyle"
        | "updaterelstyle"
        | "updatelayoutconfig"
        | "addelementtag"
        | "addreltag"
        | "updateboundarystyle"
        | "show_legend"
        | "show_floating_legend"
        | "hide_stereotype"
        | "lay_u"
        | "lay_up"
        | "lay_d"
        | "lay_down"
        | "lay_l"
        | "lay_left"
        | "lay_r"
        | "lay_right"
        | "lay_distance" => Some(Statement::Ignored),
        _ => None,
    }
}

/// A relation written before the elements it joins, kept by alias until everything has been read.
struct Pending {
    line: Line,
    from: String,
    to: String,
    relation: Relation,
}

fn read(source: &Source) -> Result<Diagram, Problem> {
    let mut diagram = Diagram {
        numbered: source.keyword.eq_ignore_ascii_case("c4dynamic"),
        ..Diagram::default()
    };
    let mut open: Vec<usize> = Vec::new();
    // A boundary call whose `{` is on the line after it, which C4-PlantUML allows.
    let mut waiting: Option<Call> = None;
    let mut pending: Vec<Pending> = Vec::new();
    for line in source.statements() {
        let text = line.text.trim();
        if let Some(rest) = line.after_word("title") {
            diagram.title = Some(source::label(rest));
            continue;
        }
        if text.starts_with('}') {
            for _ in text.chars().take_while(|c| *c == '}') {
                open.pop();
            }
            continue;
        }
        if text == "{" {
            if let Some(call) = waiting.take() {
                open_boundary(&mut diagram, &mut open, &call);
            }
            continue;
        }
        waiting = None;
        let Some(call) = read_call(text) else {
            // Anything that is not a call, such as the body of a multi line `accDescr` block, is
            // skipped. Refusing it would take the whole picture away over a line that draws nothing.
            continue;
        };
        let Some(kind) = statement(&call.name) else {
            return Err(Problem::at(
                line,
                format!(
                    "`{}` is not a C4 statement. Elements look like `Person(alias, \"Label\")` and relations like `Rel(from, to, \"Label\")`.",
                    call.name
                ),
            ));
        };
        if call.positional.len() < 2 && !matches!(kind, Statement::Ignored) {
            return Err(Problem::at(
                line,
                "a C4 statement needs at least an alias and a label, or a from and a to.",
            ));
        }
        if matches!(kind, Statement::Element(..) | Statement::Deployment) && !call.opens {
            diagram.members.push((open.last().copied(), End::Element(diagram.elements.len())));
        }
        match kind {
            Statement::Element(level, form, external) => diagram.elements.push(element_from(
                &call,
                level,
                form,
                external,
                open.last().copied(),
            )),
            Statement::Boundary(_) if call.opens => open_boundary(&mut diagram, &mut open, &call),
            Statement::Boundary(_) => waiting = Some(call),
            Statement::Deployment if call.opens => open_boundary(&mut diagram, &mut open, &call),
            Statement::Deployment => {
                let mut node =
                    element_from(&call, Level::Node, Form::Box, false, open.last().copied());
                node.technology = call.argument(2, "type");
                node.description = call.argument(3, "descr");
                diagram.elements.push(node);
            }
            Statement::Relation { both, back, upward, indexed } => {
                let skip = usize::from(indexed);
                let relation = Relation {
                    from: End::Element(0),
                    to: End::Element(0),
                    label: call.argument(skip + 2, "label"),
                    technology: call.argument(skip + 3, "techn"),
                    both,
                    back,
                    upward,
                };
                pending.push(Pending {
                    line: (*line).clone(),
                    from: call.positional.get(skip).cloned().unwrap_or_default(),
                    to: call.positional.get(skip + 1).cloned().unwrap_or_default(),
                    relation,
                });
            }
            Statement::Ignored => {}
        }
    }
    resolve_relations(&mut diagram, pending)?;
    Ok(diagram)
}

/// An element, from the arguments of the call that declared it.
fn element_from(
    call: &Call,
    level: Level,
    form: Form,
    external: bool,
    group: Option<usize>,
) -> Element {
    // A person or a system has no technology argument; a container and a component have it third.
    let (technology, description) = match level {
        Level::Person | Level::System => (String::new(), call.argument(2, "descr")),
        _ => (call.argument(2, "techn"), call.argument(3, "descr")),
    };
    Element {
        alias: call.positional[0].clone(),
        label: call.argument(1, "label"),
        technology,
        description,
        level,
        form,
        external,
        group,
    }
}

/// Open a boundary named by `call`, inside whichever boundary is open.
fn open_boundary(diagram: &mut Diagram, open: &mut Vec<usize>, call: &Call) {
    let deployment = matches!(statement(&call.name), Some(Statement::Deployment));
    let kind = match statement(&call.name) {
        Some(Statement::Boundary(word)) if !word.is_empty() => word.to_owned(),
        _ => call.argument(2, "type"),
    };
    diagram.members.push((open.last().copied(), End::Boundary(diagram.boundaries.len())));
    diagram.boundaries.push(Boundary {
        alias: call.positional.first().cloned().unwrap_or_default(),
        label: call.argument(1, "label"),
        kind,
        deployment,
        parent: open.last().copied(),
    });
    open.push(diagram.boundaries.len() - 1);
}

/// Join each relation to the elements or boundaries its aliases name.
fn resolve_relations(diagram: &mut Diagram, pending: Vec<Pending>) -> Result<(), Problem> {
    let elements: HashMap<&str, usize> = diagram
        .elements
        .iter()
        .enumerate()
        .map(|(index, element)| (element.alias.as_str(), index))
        .collect();
    let boundaries: HashMap<&str, usize> = diagram
        .boundaries
        .iter()
        .enumerate()
        .map(|(index, boundary)| (boundary.alias.as_str(), index))
        .collect();
    let find = |alias: &str| {
        elements
            .get(alias)
            .map(|index| End::Element(*index))
            .or_else(|| boundaries.get(alias).map(|index| End::Boundary(*index)))
    };
    let mut relations = Vec::new();
    for item in pending {
        let (Some(from), Some(to)) = (find(&item.from), find(&item.to)) else {
            let missing = if find(&item.from).is_none() { &item.from } else { &item.to };
            return Err(Problem::at(
                &item.line,
                format!("`{missing}` is not the alias of anything declared in this diagram."),
            ));
        };
        relations.push(Relation { from, to, ..item.relation });
    }
    diagram.relations = relations;
    Ok(())
}

/// An element's words, measured and ready to be drawn.
struct Words {
    kind: Label,
    name: Label,
    technology: Option<Label>,
    description: Option<Label>,
}

impl Words {
    fn width(&self) -> f32 {
        let mut width = self.kind.width.max(self.name.width);
        if let Some(technology) = &self.technology {
            width = width.max(technology.width);
        }
        if let Some(description) = &self.description {
            width = width.max(description.width);
        }
        width
    }

    fn height(&self) -> f32 {
        let mut height = self.kind.height + GAP + self.name.height;
        if let Some(technology) = &self.technology {
            height += technology.height;
        }
        if let Some(description) = &self.description {
            height += GAP * 2.0 + description.height;
        }
        height
    }
}

/// The word in guillemets above an element's name.
fn kind_word(element: &Element) -> String {
    let base = match (element.level, element.form) {
        (Level::Person, _) => "person",
        (Level::System, Form::Database) => "system database",
        (Level::System, Form::Queue) => "system queue",
        (Level::System, _) => "system",
        (Level::Container, Form::Database) => "container database",
        (Level::Container, Form::Queue) => "container queue",
        (Level::Container, _) => "container",
        (Level::Component, Form::Database) => "component database",
        (Level::Component, Form::Queue) => "component queue",
        (Level::Component, _) => "component",
        (Level::Node, _) => "node",
    };
    match element.external {
        true => format!("«external {base}»"),
        false => format!("«{base}»"),
    }
}

fn measure_words(element: &Element, options: &Options) -> Words {
    let small = options.style(0.75, false);
    let bold = options.style(1.0, true);
    let body = options.style(0.82, false);
    let technology = (!element.technology.trim().is_empty()).then(|| {
        text::measure(&format!("[{}]", element.technology), &small, options.metrics, ELEMENT_WRAP)
    });
    let description = (!element.description.trim().is_empty())
        .then(|| text::measure(&element.description, &body, options.metrics, ELEMENT_WRAP));
    Words {
        kind: text::measure_unwrapped(&kind_word(element), &small, options.metrics),
        name: text::measure(&element.label, &bold, options.metrics, ELEMENT_WRAP),
        technology,
        description,
    }
}

/// The shape an element is drawn with, which also says where its words go.
fn shape_of(form: Form) -> Shape {
    match form {
        Form::Box => Shape::Round,
        Form::Database => Shape::Cylinder,
        Form::Queue => Shape::HorizontalCylinder,
        Form::Person => Shape::Person,
    }
}

/// How large an element's box is for its words.
fn element_size(element: &Element, words: &Words) -> Size {
    let width = (words.width() + parts::PADDING_X * 2.0 + 8.0).max(SMALLEST_WIDTH);
    let height = words.height() + parts::PADDING_Y * 2.0 + 6.0;
    match element.form {
        Form::Box => Size::new(width, height),
        Form::Database => Size::new(width, height + 22.0),
        Form::Queue => Size::new(width + 40.0, height),
        Form::Person => Size::new(width, height + 30.0),
    }
}

/// A boundary's title: its name in bold and its kind in brackets.
fn boundary_title(boundary: &Boundary, options: &Options) -> (Label, Option<Label>) {
    let name =
        text::measure_unwrapped(&boundary.label, &options.style(0.95, true), options.metrics);
    let kind = (!boundary.kind.trim().is_empty()).then(|| {
        text::measure_unwrapped(
            &format!("[{}]", boundary.kind),
            &options.style(0.75, false),
            options.metrics,
        )
    });
    (name, kind)
}

/// A relation's label: its words and, on a second line, its technology in brackets.
fn relation_label(
    relation: &Relation,
    number: Option<usize>,
    options: &Options,
) -> (Label, Option<Label>) {
    let words = match number {
        Some(number) if relation.label.is_empty() => format!("{number}"),
        Some(number) => format!("{number}: {}", relation.label),
        None => relation.label.clone(),
    };
    let label =
        text::measure(&words, &options.style(0.82, false), options.metrics, text::EDGE_WRAP + 30.0);
    let technology = (!relation.technology.trim().is_empty()).then(|| {
        text::measure(
            &format!("[{}]", relation.technology),
            &options.style(0.72, false),
            options.metrics,
            text::EDGE_WRAP + 30.0,
        )
    });
    (label, technology)
}

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let mut titled = source.clone();
    if diagram.title.is_some() {
        titled.title.clone_from(&diagram.title);
    }
    let mut scene = Scene::new();
    if diagram.elements.is_empty() {
        parts::title(&mut scene, &titled, options, 200.0);
        parts::finish(&mut scene);
        return scene;
    }
    let words: Vec<Words> =
        diagram.elements.iter().map(|element| measure_words(element, options)).collect();
    let titles: Vec<(Label, Option<Label>)> =
        diagram.boundaries.iter().map(|boundary| boundary_title(boundary, options)).collect();
    let labels: Vec<(Label, Option<Label>)> = diagram
        .relations
        .iter()
        .enumerate()
        .map(|(index, relation)| {
            relation_label(relation, diagram.numbered.then_some(index + 1), options)
        })
        .collect();

    let mut graph = layered::Graph { direction: Direction::Down, ..layered::Graph::default() };
    for (index, boundary) in diagram.boundaries.iter().enumerate() {
        let (name, kind) = &titles[index];
        let height = name.height + kind.as_ref().map_or(0.0, |kind| kind.height) + 8.0;
        let width = name.width.max(kind.as_ref().map_or(0.0, |kind| kind.width));
        graph.groups.push(GroupSpec { title: Size::new(width, height), parent: boundary.parent });
    }
    for (index, element) in diagram.elements.iter().enumerate() {
        graph.add_node(element_size(element, &words[index]), element.group);
    }
    for (index, relation) in diagram.relations.iter().enumerate() {
        let (label, technology) = &labels[index];
        let size = Size::new(
            label.width.max(technology.as_ref().map_or(0.0, |technology| technology.width)),
            label.height + technology.as_ref().map_or(0.0, |technology| technology.height),
        );
        let (from, from_group) = end_index(relation.from);
        let (to, to_group) = end_index(relation.to);
        let (from, to, from_group, to_group) = match relation.upward {
            true => (to, from, to_group, from_group),
            false => (from, to, from_group, to_group),
        };
        graph.edges.push(EdgeSpec { from, to, label: size, span: 1, from_group, to_group });
    }
    stack_the_unconnected(diagram, &mut graph);
    let placed = layered::layout(&graph);

    let top = parts::title(&mut scene, &titled, options, placed.size.width);
    let origin = Point::new(parts::MARGIN, top + parts::MARGIN);
    draw_boundaries(&mut scene, diagram, &placed, origin, options);
    draw_relations(&mut scene, diagram, &placed, origin, &labels, options);
    draw_elements(&mut scene, diagram, &placed, origin, &words, options);
    draw_boundary_titles(&mut scene, diagram, &placed, origin, &titles, options);
    parts::finish(&mut scene);
    scene
}

/// How many elements with no relations at all stand side by side before the next row starts.
///
/// Mermaid's own `c4ShapeInRow` default. Without it every unconnected element in a boundary is in the
/// first rank, and the documentation's context example came out one row of eleven boxes, shrunk until
/// it could not be read.
const IN_A_ROW: usize = 4;

/// Join the elements no relation touches into rows of [`IN_A_ROW`], each under the one before it, by
/// edges the layout reads and nothing draws. They come after the relations, so the relations keep
/// their own numbers in what the layout hands back.
fn stack_the_unconnected(diagram: &Diagram, graph: &mut layered::Graph) {
    let mut touched_elements = vec![false; diagram.elements.len()];
    let mut touched_boundaries = vec![false; diagram.boundaries.len()];
    for relation in &diagram.relations {
        for end in [relation.from, relation.to] {
            match end {
                End::Element(index) => touched_elements[index] = true,
                End::Boundary(index) => touched_boundaries[index] = true,
            }
        }
    }
    // A boundary counts as touched when anything inside it is, so it is never moved under another
    // item by an edge that would fight a relation's.
    for (index, element) in diagram.elements.iter().enumerate() {
        let mut group = element.group;
        while let Some(at) = group.filter(|_| touched_elements[index]) {
            touched_boundaries[at] = true;
            group = diagram.boundaries[at].parent;
        }
    }
    let touched = |end: End| match end {
        End::Element(index) => touched_elements[index],
        End::Boundary(index) => touched_boundaries[index],
    };
    let mut groups: Vec<Option<usize>> = Vec::new();
    for (group, _) in &diagram.members {
        if !groups.contains(group) {
            groups.push(*group);
        }
    }
    for group in groups {
        let items: Vec<End> = diagram
            .members
            .iter()
            .filter(|(owner, _)| *owner == group)
            .map(|(_, end)| *end)
            .collect();
        for at in IN_A_ROW..items.len() {
            if touched(items[at]) {
                continue;
            }
            let (from, from_group) = end_index(items[at - IN_A_ROW]);
            let (to, to_group) = end_index(items[at]);
            graph.edges.push(EdgeSpec { from_group, to_group, ..EdgeSpec::new(from, to) });
        }
    }
}

/// The index the layered layout knows an end by, and whether it is a group's.
fn end_index(end: End) -> (usize, bool) {
    match end {
        End::Element(index) => (index, false),
        End::Boundary(index) => (index, true),
    }
}

/// A colour this far from `from` towards `to`.
fn mix(from: Color, to: Color, share: f32) -> Color {
    let blend =
        |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * share).round().clamp(0.0, 255.0) as u8;
    Color::rgb(blend(from.r, to.r), blend(from.g, to.g), blend(from.b, to.b))
}

/// The fill and the border of an element, by its level and whether it is external.
fn colours(element: &Element, options: &Options) -> (Paint, Color) {
    let theme = &options.theme;
    let base = theme.node_fill.color;
    if element.external {
        return (Paint::solid(mix(base, theme.dim, 0.32)), theme.dim);
    }
    let share = match element.level {
        Level::Person => 0.6,
        Level::System => 0.5,
        Level::Container => 0.36,
        Level::Component => 0.22,
        Level::Node => 0.12,
    };
    (Paint::solid(mix(base, theme.accent, share)), mix(theme.node_stroke, theme.accent, 0.7))
}

fn draw_boundaries(
    scene: &mut Scene,
    diagram: &Diagram,
    placed: &layered::Placed,
    origin: Point,
    options: &Options,
) {
    let theme = &options.theme;
    for (index, boundary) in diagram.boundaries.iter().enumerate() {
        let frame = placed.groups[index].moved(origin.x, origin.y);
        if frame.width <= 0.0 {
            continue;
        }
        let stroke = Stroke::new(theme.group_stroke.max_with(theme.dim), parts::LINE);
        match boundary.deployment {
            true => scene.add(Item::Rect {
                rect: frame,
                radius: parts::CORNER,
                fill: Some(theme.group_fill),
                stroke: Some(stroke),
            }),
            false => {
                scene.add(Item::Rect {
                    rect: frame,
                    radius: parts::CORNER,
                    fill: Some(theme.group_fill),
                    stroke: None,
                });
                dashed_frame(scene, frame, stroke);
            }
        }
    }
}

/// A rectangle's outline as four dashed lines, which is how C4 draws a boundary.
fn dashed_frame(scene: &mut Scene, frame: Rect, stroke: Stroke) {
    let corners = [
        Point::new(frame.left(), frame.top()),
        Point::new(frame.right(), frame.top()),
        Point::new(frame.right(), frame.bottom()),
        Point::new(frame.left(), frame.bottom()),
        Point::new(frame.left(), frame.top()),
    ];
    scene.add(Item::Line { points: corners.to_vec(), stroke, dash: Dash::Dashed(7.0, 5.0) });
}

/// Each boundary's name and kind, at the top left inside its frame, on a panel so a relation crossing
/// the top of the frame passes behind the words.
fn draw_boundary_titles(
    scene: &mut Scene,
    diagram: &Diagram,
    placed: &layered::Placed,
    origin: Point,
    titles: &[(Label, Option<Label>)],
    options: &Options,
) {
    let theme = &options.theme;
    for (index, (name, kind)) in titles.iter().enumerate().take(diagram.boundaries.len()) {
        let frame = placed.groups[index].moved(origin.x, origin.y);
        if frame.width <= 0.0 {
            continue;
        }
        let at = Point::new(frame.left() + 12.0, frame.top() + 5.0);
        let width = name.width.max(kind.as_ref().map_or(0.0, |kind| kind.width));
        let height = name.height + kind.as_ref().map_or(0.0, |kind| kind.height);
        scene.add(Item::Rect {
            rect: Rect::new(at.x - 4.0, at.y - 1.0, width + 8.0, height + 2.0),
            radius: 3.0,
            fill: Some(Paint::solid(theme.node_fill.color)),
            stroke: None,
        });
        parts::label_at(
            scene,
            name,
            at,
            &parts::text_style(options, 0.95, true, theme.text),
            Anchor::Start,
        );
        if let Some(kind) = kind {
            parts::label_at(
                scene,
                kind,
                Point::new(at.x, at.y + name.height),
                &parts::text_style(options, 0.75, false, theme.dim),
                Anchor::Start,
            );
        }
    }
}

fn draw_elements(
    scene: &mut Scene,
    diagram: &Diagram,
    placed: &layered::Placed,
    origin: Point,
    words: &[Words],
    options: &Options,
) {
    let theme = &options.theme;
    for (index, element) in diagram.elements.iter().enumerate() {
        let rect = placed.nodes[index].moved(origin.x, origin.y);
        let (fill, border) = colours(element, options);
        let shape = shape_of(element.form);
        shape.draw(scene, rect, fill, Stroke::new(border, parts::LINE));
        let area = shape.text_area(rect);
        let words = &words[index];
        let mut y = area.centre().y - words.height() / 2.0;
        let x = area.centre().x;
        parts::label_at(
            scene,
            &words.kind,
            Point::new(x, y),
            &parts::text_style(options, 0.75, false, theme.text),
            Anchor::Middle,
        );
        y += words.kind.height + GAP;
        parts::label_at(
            scene,
            &words.name,
            Point::new(x, y),
            &parts::text_style(options, 1.0, true, theme.text),
            Anchor::Middle,
        );
        y += words.name.height;
        if let Some(technology) = &words.technology {
            parts::label_at(
                scene,
                technology,
                Point::new(x, y),
                &parts::text_style(options, 0.75, false, theme.text),
                Anchor::Middle,
            );
            y += technology.height;
        }
        if let Some(description) = &words.description {
            y += GAP * 2.0;
            parts::label_at(
                scene,
                description,
                Point::new(x, y),
                &parts::text_style(options, 0.82, false, theme.text),
                Anchor::Middle,
            );
        }
    }
}

/// The outline a relation is cut back to at one end.
fn outline_of(placed: &layered::Placed, origin: Point, end: End) -> Outline {
    match end {
        End::Element(index) => Outline::Rect(placed.nodes[index].moved(origin.x, origin.y)),
        End::Boundary(index) => Outline::Rect(placed.groups[index].moved(origin.x, origin.y)),
    }
}

fn draw_relations(
    scene: &mut Scene,
    diagram: &Diagram,
    placed: &layered::Placed,
    origin: Point,
    labels: &[(Label, Option<Label>)],
    options: &Options,
) {
    let theme = &options.theme;
    for (index, relation) in diagram.relations.iter().enumerate() {
        let path: Vec<Point> = placed.edges[index]
            .iter()
            .map(|point| Point::new(point.x + origin.x, point.y + origin.y))
            .collect();
        if path.len() < 2 {
            continue;
        }
        // The layout's line runs from the end it laid out first, which is the second element for
        // `Rel_U`; the arrow still has to arrive at the element the relation points at.
        let (first, last) = match relation.upward {
            true => (relation.to, relation.from),
            false => (relation.from, relation.to),
        };
        let path = parts::edge_path(
            &path,
            &outline_of(placed, origin, first),
            &outline_of(placed, origin, last),
        );
        if path.len() < 2 {
            continue;
        }
        let points_at_last = relation.back == relation.upward;
        let (head, tail) = match (relation.both, points_at_last) {
            (true, _) => (Ending::Arrow, Ending::Arrow),
            (false, true) => (Ending::Arrow, Ending::None),
            (false, false) => (Ending::None, Ending::Arrow),
        };
        let drawn = parts::trimmed(&path, parts::ending_inset(tail), parts::ending_inset(head));
        scene.add(Item::Line {
            points: drawn,
            stroke: Stroke::new(theme.line, parts::LINE),
            dash: Dash::Dashed(8.0, 4.0),
        });
        parts::ending(
            scene,
            head,
            path[path.len() - 1],
            parts::heading(&path),
            theme.line,
            theme.node_fill,
        );
        parts::ending(
            scene,
            tail,
            path[0],
            parts::tail_heading(&path),
            theme.line,
            theme.node_fill,
        );
        let at = placed.labels[index];
        draw_relation_label(
            scene,
            &labels[index],
            Point::new(at.x + origin.x, at.y + origin.y),
            options,
        );
    }
}

/// A relation's label on a panel, the technology under the words in a smaller, dimmer face.
fn draw_relation_label(
    scene: &mut Scene,
    label: &(Label, Option<Label>),
    at: Point,
    options: &Options,
) {
    let (words, technology) = label;
    if words.is_empty() && technology.is_none() {
        return;
    }
    let width = words.width.max(technology.as_ref().map_or(0.0, |technology| technology.width));
    let height = words.height + technology.as_ref().map_or(0.0, |technology| technology.height);
    let panel = Rect::around(at, Size::new(width + 8.0, height + 4.0));
    scene.add(Item::Rect {
        rect: panel,
        radius: 3.0,
        fill: Some(Paint::solid(options.theme.node_fill.color)),
        stroke: None,
    });
    let top = panel.top() + 2.0;
    parts::label_at(
        scene,
        words,
        Point::new(at.x, top),
        &parts::text_style(options, 0.82, false, options.theme.text),
        Anchor::Middle,
    );
    if let Some(technology) = technology {
        parts::label_at(
            scene,
            technology,
            Point::new(at.x, top + words.height),
            &parts::text_style(options, 0.72, false, options.theme.dim),
            Anchor::Middle,
        );
    }
}

/// The lighter of two colours, so a boundary's line shows on a dark ground and a light one.
trait Brighter {
    fn max_with(self, other: Color) -> Color;
}

impl Brighter for Color {
    fn max_with(self, other: Color) -> Color {
        let light = |colour: Color| colour.r as u32 + colour.g as u32 + colour.b as u32;
        if light(other) > light(self) {
            other
        } else {
            self
        }
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

    const CONTEXT: &str = "C4Context
      title System Context diagram for Internet Banking System
      Enterprise_Boundary(b0, \"BankBoundary0\") {
        Person(customerA, \"Banking Customer A\", \"A customer of the bank, with personal bank accounts.\")
        Person_Ext(customerC, \"Banking Customer C\", \"desc\")
        System(SystemAA, \"Internet Banking System\", \"Allows customers to view information about their bank accounts, and make payments.\")
        Enterprise_Boundary(b1, \"BankBoundary\") {
          SystemDb_Ext(SystemE, \"Mainframe Banking System\", \"Stores all of the core banking information.\")
          System_Boundary(b2, \"BankBoundary2\") {
            System(SystemA, \"Banking System A\")
          }
          SystemQueue(SystemF, \"Banking System F Queue\", \"A system of the bank.\")
        }
      }
      BiRel(customerA, SystemAA, \"Uses\")
      Rel(SystemAA, SystemE, \"Sends e-mails\", \"SMTP\")
      Rel(SystemAA, SystemA, \"Uses\")
      UpdateElementStyle(customerA, $fontColor=\"red\", $bgColor=\"grey\", $borderColor=\"red\")
      UpdateRelStyle(customerA, SystemAA, $textColor=\"blue\", $lineColor=\"blue\", $offsetX=\"5\")
      UpdateLayoutConfig($c4ShapeInRow=\"3\", $c4BoundaryInRow=\"1\")
";

    #[test]
    fn every_element_and_boundary_is_read_with_its_parts() {
        let diagram = diagram(CONTEXT);
        assert_eq!(diagram.elements.len(), 6);
        assert_eq!(diagram.boundaries.len(), 3);
        let person = &diagram.elements[0];
        assert_eq!(person.level, Level::Person);
        assert_eq!(person.label, "Banking Customer A");
        assert!(person.description.starts_with("A customer"));
        assert!(diagram.elements[1].external, "Person_Ext is external");
        let mainframe = &diagram.elements[3];
        assert_eq!((mainframe.form, mainframe.external), (Form::Database, true));
        assert_eq!(diagram.elements[5].form, Form::Queue);
        assert_eq!(diagram.boundaries[2].parent, Some(1), "boundaries nest");
        assert_eq!(
            diagram.elements[4].group,
            Some(2),
            "an element belongs to the innermost boundary"
        );
        assert_eq!(diagram.boundaries[0].kind, "Enterprise");
    }

    #[test]
    fn a_container_reads_its_technology_third_and_a_relation_its_technology_fourth() {
        let diagram = diagram(
            "C4Container\n Container(spa, \"Single-Page App\", \"JavaScript, Angular\", \"Provides the functionality\")\n ContainerDb(db, \"Database\", \"SQL Database\")\n Rel(spa, db, \"Reads\", \"JDBC\")\n",
        );
        assert_eq!(diagram.elements[0].technology, "JavaScript, Angular");
        assert_eq!(diagram.elements[0].description, "Provides the functionality");
        assert_eq!(diagram.relations[0].technology, "JDBC");
    }

    #[test]
    fn named_arguments_are_read_and_tags_are_ignored() {
        let diagram = diagram(
            "C4Container\n System_Ext(mail, \"E-Mail System\", \"The internal system\", $tags=\"v1.0\")\n Person(customer, Customer, $descr=\"A customer\")\n",
        );
        assert_eq!(diagram.elements[0].description, "The internal system");
        assert_eq!(diagram.elements[1].label, "Customer", "an unquoted label is read");
        assert_eq!(diagram.elements[1].description, "A customer");
    }

    #[test]
    fn every_relation_form_is_read() {
        let diagram = diagram(
            "C4Context\n Person(a, \"A\")\n System(b, \"B\")\n Rel(a, b, \"one\")\n BiRel(a, b, \"two\")\n Rel_U(a, b, \"three\")\n Rel_Back(a, b, \"four\")\n RelIndex(1, a, b, \"five\")\n Rel_R(a, b, \"six\")\n",
        );
        let relations = &diagram.relations;
        assert_eq!(relations.len(), 6);
        assert!(relations[1].both);
        assert!(relations[2].upward);
        assert!(relations[3].back);
        assert_eq!(relations[4].label, "five", "RelIndex's index is not the label");
        assert_eq!(relations[4].from, End::Element(0));
    }

    #[test]
    fn a_deployment_node_holds_what_is_inside_it_and_is_a_box_when_it_holds_nothing() {
        let diagram = diagram(
            "C4Deployment\n Deployment_Node(mob, \"Customer's mobile device\", \"Apple IOS or Android\"){\n  Container(mobile, \"Mobile App\", \"Xamarin\")\n }\n Node(db, \"Database server\", \"Ubuntu\")\n",
        );
        assert_eq!(diagram.boundaries.len(), 1);
        assert!(diagram.boundaries[0].deployment);
        assert_eq!(diagram.boundaries[0].kind, "Apple IOS or Android");
        assert_eq!(diagram.elements[0].group, Some(0));
        assert_eq!(diagram.elements[1].level, Level::Node);
        assert_eq!(diagram.elements[1].technology, "Ubuntu");
    }

    #[test]
    fn a_dynamic_diagram_numbers_its_relations() {
        let scene = check::drawn(
            "C4Dynamic\n Container(c1, \"App\")\n Component(c2, \"Controller\")\n Rel(c1, c2, \"Submits credentials to\", \"JSON/HTTPS\")\n Rel(c2, c1, \"Answers\")\n",
            &options(),
            &["App", "Controller"],
        );
        let texts = scene.texts();
        assert!(texts.iter().any(|text| text.starts_with("1: Submits")), "{texts:?}");
        assert!(texts.iter().any(|text| text.starts_with("2: Answers")), "{texts:?}");
    }

    #[test]
    fn a_relation_to_an_alias_nobody_declared_says_which_one() {
        let problem =
            check::refused("C4Context\n Person(a, \"A\")\n Rel(a, nobody, \"Uses\")\n", &options());
        assert_eq!(problem.line, Some(3));
        assert!(problem.reason.contains("nobody"));
    }

    #[test]
    fn a_statement_that_is_not_c4_says_what_one_looks_like() {
        let problem = check::refused("C4Context\n Persn(a, \"A\")\n", &options());
        assert!(problem.reason.contains("Persn"));
    }

    #[test]
    fn the_context_example_is_drawn_with_every_word_and_boxes_that_nest_or_miss() {
        let scene = check::drawn(
            CONTEXT,
            &options(),
            &[
                "System Context diagram for Internet Banking System",
                "Banking Customer A",
                "«person»",
                "«external person»",
                "Internet Banking System",
                "«external system database»",
                "BankBoundary2",
                "[Enterprise]",
                "[SMTP]",
            ],
        );
        check::boxes_nest_or_miss(&scene.rects());
    }

    #[test]
    fn every_words_of_an_element_are_inside_its_box() {
        // The kind, the name, the technology and the description are all centred in the box's text
        // area and the box is sized from them, so the widest line plus its padding fits.
        let text = "C4Container\n Container(web, \"Web Application\", \"Java and Spring MVC\", \"Delivers the static content and the Internet banking single page application to everyone who asks for it.\")\n";
        let diagram = diagram(text);
        let words = measure_words(&diagram.elements[0], &options());
        let size = element_size(&diagram.elements[0], &words);
        assert!(size.width >= words.width() + parts::PADDING_X * 2.0);
        assert!(size.height >= words.height() + parts::PADDING_Y * 2.0);
        assert!(
            words.description.as_ref().is_some_and(|label| label.lines.len() > 1),
            "a long description wraps"
        );
    }
}
