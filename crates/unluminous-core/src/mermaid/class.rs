//! Class diagrams: `classDiagram`.
//!
//! A class is a box in three compartments — its name with any `<<annotation>>` above it, then its
//! attributes, then its methods — joined by the eight relationship arrows. The boxes are placed by
//! the shared layered layout, so a class diagram and a flowchart agree about what a good arrangement
//! looks like.
//!
//! ## Which way an arrow points, and which way the layout reads it
//!
//! These two are not the same thing and conflating them is the easy mistake. `Animal <|-- Dog` puts
//! the hollow triangle at **Animal's** end, but the parent still belongs **above** the child. So the
//! layout edge always runs left to right as it was written, and each end carries its own ending. The
//! ranking then puts `Animal` above `Dog` because that is the direction of the edge, and the
//! triangle is drawn at the tail rather than at the head.
//!
//! Getting that backwards gives a picture that is upside down *and* has its arrowheads at the wrong
//! end, which is why it has a test of its own.

use std::collections::HashMap;

use super::layered::{self, Direction, EdgeSpec, GroupSpec};
use super::parts::{self, Ending, Outline};
use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke};
use super::source::{self, Line, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// How tall a member row is, as a multiple of the font's own line height.
const ROW_SPACING: f32 = 1.35;
/// The radius of an interface drawn as a lollipop.
const LOLLIPOP: f32 = 8.0;

/// One class.
#[derive(Debug, Clone, PartialEq, Default)]
struct Class {
    name: String,
    /// `<<Interface>>`, `<<Abstract>>` and the rest, without the brackets.
    annotation: Option<String>,
    /// Members with no brackets in them.
    attributes: Vec<String>,
    /// Members with brackets in them.
    methods: Vec<String>,
    group: Option<usize>,
    /// Written as `()` beside a relationship, `bar ()-- foo`: an interface drawn as a small circle
    /// with its name under it, the UML lollipop (`task-2339`).
    lollipop: bool,
    /// A note rather than a class: its `name` is the note's words.
    note: bool,
}

/// One relationship.
#[derive(Debug, Clone, PartialEq)]
struct Relation {
    from: usize,
    to: usize,
    /// Drawn at the `from` end.
    tail: Ending,
    /// Drawn at the `to` end.
    head: Ending,
    dashed: bool,
    label: String,
    /// `"1"`, `"0..*"` and the like, beside each end.
    from_count: String,
    to_count: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Diagram {
    classes: Vec<Class>,
    by_name: HashMap<String, usize>,
    relations: Vec<Relation>,
    namespaces: Vec<String>,
    /// The namespace each namespace is written inside, for `namespace A { namespace B { … } }`.
    namespace_parents: Vec<Option<usize>>,
    direction: Direction,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    Ok(draw(&diagram, source, options))
}

fn read(source: &Source) -> Result<Diagram, Problem> {
    let mut diagram = Diagram::default();
    // The namespaces open round the line being read, innermost last (`task-2339`: a namespace inside
    // a namespace closed the outer one at its own `}` and was drawn as a frame of its own).
    let mut open_namespaces: Vec<usize> = Vec::new();
    let mut namespace: Option<usize> = None;
    // The class a `{` block belongs to, while its members are being read.
    let mut open_class: Option<usize> = None;
    for line in source.statements() {
        let text = line.text.trim();
        if text == "}" {
            if open_class.take().is_none() {
                open_namespaces.pop();
                namespace = open_namespaces.last().copied();
            }
            continue;
        }
        if let Some(class) = open_class {
            if text.is_empty() {
                continue;
            }
            add_member(&mut diagram.classes[class], text);
            continue;
        }
        if let Some(rest) = line.after_word("namespace") {
            diagram.namespaces.push(namespace_title(rest));
            diagram.namespace_parents.push(namespace);
            namespace = Some(diagram.namespaces.len() - 1);
            open_namespaces.extend(namespace);
            continue;
        }
        if let Some(rest) = line.after_word("direction") {
            if let Some(direction) = Direction::parse(rest) {
                diagram.direction = direction;
            }
            continue;
        }
        if is_decoration(line) {
            continue;
        }
        // `note "text"` and `note for Duck "text"`: a note, joined to its class by a dotted line
        // (`task-2339`). They used to be read and thrown away with the colours.
        if let Some(rest) = line.after_word("note") {
            read_note(&mut diagram, rest, namespace);
            continue;
        }
        if let Some(rest) = line.after_word("class") {
            open_class = read_class(&mut diagram, rest, namespace);
            continue;
        }
        if read_relation(&mut diagram, line, namespace)? {
            continue;
        }
        // `Animal : +int age` — one member on a line of its own.
        if let Some((name, member)) = text.split_once(':') {
            let name = name.trim();
            if !name.is_empty() && !member.trim().is_empty() {
                let class = class_of(&mut diagram, name, namespace);
                add_member(&mut diagram.classes[class], member.trim());
                continue;
            }
        }
        // `<<interface>> Shape` on a line of its own, which is the other way of writing an
        // annotation and the one Mermaid's own examples use.
        if text.contains("<<") {
            if let (name, Some(annotation)) = split_annotation(text) {
                if !name.is_empty() {
                    let class = class_of(&mut diagram, name, namespace);
                    diagram.classes[class].annotation = Some(annotation);
                    continue;
                }
            }
        }
        // A bare name introduces a class, which is how an interface with no members is written.
        if !text.is_empty() && text.chars().all(is_name_character) {
            class_of(&mut diagram, text, namespace);
        }
    }
    Ok(diagram)
}

/// True for a line about colour or about clicking, which is read and thrown away.
fn is_decoration(line: &Line) -> bool {
    ["style", "classDef", "cssClass", "click", "callback", "link"]
        .iter()
        .any(|word| line.starts_with_word(word))
}

fn is_name_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '.' | '~' | '-')
}

/// `namespace A.B ["Shown"]`, or just `namespace A.B {`.
fn namespace_title(rest: &str) -> String {
    let rest = rest.trim().trim_end_matches('{').trim();
    if let (Some(open), Some(close)) = (rest.find('['), rest.rfind(']')) {
        if close > open {
            return source::label(&rest[open + 1..close]);
        }
    }
    source::label(rest)
}

/// Read `class Duck`, `class Duck["A duck"]`, `class Duck { ... ` and `class Duck:::styled`.
///
/// Returns the class's index when the line opened a `{` block, so the lines that follow are read as
/// its members.
fn read_class(diagram: &mut Diagram, rest: &str, namespace: Option<usize>) -> Option<usize> {
    let rest = rest.trim();
    let opens = rest.ends_with('{');
    let body = rest.trim_end_matches('{').trim();
    // `class A, B, C` declares several at once.
    let mut last = None;
    for piece in source::split_outside_quotes(body, ',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let piece = match piece.find(":::") {
            Some(at) => piece[..at].trim(),
            None => piece,
        };
        // An inline `<<Interface>>` on the same line as the class.
        let (piece, annotation) = split_annotation(piece);
        let (name, shown) = match (piece.find('['), piece.rfind(']')) {
            (Some(open), Some(close)) if close > open => {
                (piece[..open].trim(), Some(source::label(&piece[open + 1..close])))
            }
            _ => (piece, None),
        };
        if name.is_empty() {
            continue;
        }
        let index = class_of(diagram, name, namespace);
        if let Some(shown) = shown {
            diagram.classes[index].name = shown;
        }
        if let Some(annotation) = annotation {
            diagram.classes[index].annotation = Some(annotation);
        }
        last = Some(index);
    }
    opens.then_some(last).flatten()
}

/// Take an `<<Annotation>>` off a piece of text, wherever in it it was written.
fn split_annotation(piece: &str) -> (&str, Option<String>) {
    let Some(open) = piece.find("<<") else {
        return (piece, None);
    };
    let Some(close) = piece[open..].find(">>") else {
        return (piece, None);
    };
    let annotation = piece[open + 2..open + close].trim().to_owned();
    // Whatever is left either side of it is the name.
    let before = piece[..open].trim();
    let after = piece[open + close + 2..].trim();
    let name = if before.is_empty() { after } else { before };
    (name, Some(annotation))
}

/// Find or make the class of this name.
fn class_of(diagram: &mut Diagram, name: &str, namespace: Option<usize>) -> usize {
    let name = source::unquote(name.trim());
    // A name with spaces or punctuation in it is written between backticks, which are not part of it.
    let name = match name.strip_prefix('`').and_then(|rest| rest.strip_suffix('`')) {
        Some(inner) => inner.to_owned(),
        None => name,
    };
    // A generic — `Square~Shape~` — is the class `Square`, shown as `Square<Shape>` the way a reader
    // writes it (`task-2339`). The tildes used to be dropped, so `Square~Shape~ {` and a later
    // `Square : member` were two classes, one called `SquareShape`.
    let key = name.split('~').next().unwrap_or(&name).trim().to_owned();
    let shown = generic(&name);
    if let Some(&known) = diagram.by_name.get(&key) {
        if shown.len() > diagram.classes[known].name.len() && diagram.classes[known].name == key {
            diagram.classes[known].name = shown;
        }
        return known;
    }
    diagram.classes.push(Class { name: shown, group: namespace, ..Class::default() });
    diagram.by_name.insert(key, diagram.classes.len() - 1);
    diagram.classes.len() - 1
}

/// A note, and for `note for X` a dotted line from it to its class.
fn read_note(diagram: &mut Diagram, rest: &str, namespace: Option<usize>) {
    let (target, words) = match rest.strip_prefix("for ") {
        Some(after) => match after.trim().split_once(char::is_whitespace) {
            Some((target, words)) => (Some(target.trim().to_owned()), words),
            None => (Some(after.trim().to_owned()), ""),
        },
        None => (None, rest),
    };
    diagram.classes.push(Class {
        name: source::label(words),
        group: namespace,
        note: true,
        ..Class::default()
    });
    let note = diagram.classes.len() - 1;
    if let Some(target) = target {
        let to = class_of(diagram, &target, namespace);
        diagram.relations.push(Relation {
            from: note,
            to,
            tail: Ending::None,
            head: Ending::None,
            dashed: true,
            label: String::new(),
            from_count: String::new(),
            to_count: String::new(),
        });
    }
}

/// Mermaid's tildes turned into angle brackets: `List~List~int~~` is `List<List<int>>`.
///
/// A tilde that comes before a word opens a type argument and any other closes one.
fn generic(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    characters
        .iter()
        .enumerate()
        .map(|(at, character)| match character {
            '~' if characters
                .get(at + 1)
                .is_some_and(|next| next.is_alphanumeric() || *next == '_') =>
            {
                '<'
            }
            '~' => '>',
            other => *other,
        })
        .collect()
}

/// Put a member in the right compartment: brackets make it a method, and nothing else does.
fn add_member(class: &mut Class, text: &str) {
    let text = text.trim().trim_end_matches(';');
    if text.is_empty() {
        return;
    }
    if let (Some(_), Some(annotation)) = (text.find("<<"), split_annotation(text).1) {
        class.annotation = Some(annotation);
        return;
    }
    let shown = generic(text);
    if text.contains('(') {
        class.methods.push(shown);
    } else {
        class.attributes.push(shown);
    }
}

/// The endings a relationship can have, written at the left of the line and at the right.
///
/// Longest first, so `<|` is never read as `<`.
const LEFT_ENDINGS: &[(&str, Ending)] = &[
    ("<|", Ending::Hollow),
    ("*", Ending::Diamond),
    ("o", Ending::HollowDiamond),
    ("<", Ending::Arrow),
];
const RIGHT_ENDINGS: &[(&str, Ending)] = &[
    ("|>", Ending::Hollow),
    ("*", Ending::Diamond),
    ("o", Ending::HollowDiamond),
    (">", Ending::Arrow),
];

/// Read `Animal <|-- Duck : says`, with cardinalities and a label.
///
/// Returns false when the line holds no relationship at all, so the caller can try the other forms.
fn read_relation(
    diagram: &mut Diagram,
    line: &Line,
    namespace: Option<usize>,
) -> Result<bool, Problem> {
    let (head, label) = match line.text.split_once(':') {
        Some((head, label)) => (head, source::label(label)),
        None => (line.text.as_str(), String::new()),
    };
    let Some((at, line_length, dashed)) = find_line(head) else {
        return Ok(false);
    };
    let before = &head[..at];
    let after = &head[at + line_length..];
    let (before, tail) = take_ending(before, LEFT_ENDINGS, true);
    let (after, head_ending) = take_ending(after, RIGHT_ENDINGS, false);
    // `bar ()-- foo` and `Class01 --() bar`: the side with `()` is an interface drawn as a lollipop.
    let (before, from_lollipop) = match before.trim_end().strip_suffix("()") {
        Some(rest) => (rest, true),
        None => (before, false),
    };
    let (after, to_lollipop) = match after.trim_start().strip_prefix("()") {
        Some(rest) => (rest, true),
        None => (after, false),
    };
    let (from_name, from_count) = split_count(before, true);
    let (to_name, to_count) = split_count(after, false);
    if from_name.is_empty() || to_name.is_empty() {
        return Err(Problem::at(line, "a relationship needs a class at each end of it"));
    }
    let from = class_of(diagram, &from_name, namespace);
    let to = class_of(diagram, &to_name, namespace);
    diagram.classes[from].lollipop |= from_lollipop;
    diagram.classes[to].lollipop |= to_lollipop;
    diagram.relations.push(Relation {
        from,
        to,
        tail,
        head: head_ending,
        dashed,
        label,
        from_count,
        to_count,
    });
    Ok(true)
}

/// Where the `--` or `..` of a relationship is, how long it is, and whether it is dashed.
fn find_line(head: &str) -> Option<(usize, usize, bool)> {
    let bytes = head.as_bytes();
    let mut at = 0;
    let mut quoted = false;
    while at + 1 < bytes.len() {
        if bytes[at] == b'"' {
            quoted = !quoted;
            at += 1;
            continue;
        }
        if !quoted && (bytes[at] == b'-' || bytes[at] == b'.') && bytes[at + 1] == bytes[at] {
            let mark = bytes[at];
            let mut end = at;
            while end < bytes.len() && bytes[end] == mark {
                end += 1;
            }
            return Some((at, end - at, mark == b'.'));
        }
        at += 1;
    }
    None
}

/// Take an ending off the inside end of a piece of text.
///
/// `at_the_end` says which side of the text the line was on: the left half's ending is at its right
/// hand end, and the right half's is at its left.
fn take_ending<'a>(
    text: &'a str,
    endings: &[(&str, Ending)],
    at_the_end: bool,
) -> (&'a str, Ending) {
    let trimmed = if at_the_end { text.trim_end() } else { text.trim_start() };
    for (mark, ending) in endings {
        let found =
            if at_the_end { trimmed.strip_suffix(mark) } else { trimmed.strip_prefix(mark) };
        if let Some(rest) = found {
            return (rest, *ending);
        }
    }
    (trimmed, Ending::None)
}

/// Split a class name from the cardinality written beside it in quotes.
fn split_count(text: &str, count_last: bool) -> (String, String) {
    let text = text.trim();
    let Some(open) = text.find('"') else {
        return (text.to_owned(), String::new());
    };
    let Some(close) = text[open + 1..].find('"').map(|at| at + open + 1) else {
        return (text.to_owned(), String::new());
    };
    let count = text[open + 1..close].to_owned();
    let name = if count_last { text[..open].trim() } else { text[close + 1..].trim() };
    let _ = count_last;
    (name.to_owned(), count)
}

/// A class box, measured.
struct Measured {
    name: Label,
    annotation: Option<Label>,
    attributes: Vec<Label>,
    methods: Vec<Label>,
    size: Size,
    /// Where the line under the name goes, from the top of the box.
    name_height: f32,
    /// Where the line under the attributes goes.
    attribute_height: f32,
}

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let boxes: Vec<Measured> =
        diagram.classes.iter().map(|class| measure(class, options)).collect();
    let label_style = options.style(0.85, false);
    let labels: Vec<Label> = diagram
        .relations
        .iter()
        .map(|relation| {
            text::measure(&relation.label, &label_style, options.metrics, text::EDGE_WRAP)
        })
        .collect();

    let mut graph = layered::Graph { direction: diagram.direction, ..layered::Graph::default() };
    for (index, name) in diagram.namespaces.iter().enumerate() {
        let title = text::measure_unwrapped(name, &options.style(0.95, true), options.metrics);
        graph.groups.push(GroupSpec {
            title: Size::new(title.width, title.height + 6.0),
            parent: diagram.namespace_parents.get(index).copied().flatten(),
        });
    }
    for (index, class) in diagram.classes.iter().enumerate() {
        graph.add_node(boxes[index].size, class.group);
    }
    let count_style = options.style(0.8, false);
    for (index, relation) in diagram.relations.iter().enumerate() {
        // The room asked for is the label **plus a count at each end**. A relationship carrying both
        // wants the label in the middle and a count clear of each node and of each arrowhead, and on
        // a gap sized for the label alone there is nowhere for all three to go: the counts end up
        // under the label's own panel. Asking for the room is the fix; nudging the counts about is
        // not, and was tried first.
        let counts = [relation.from_count.as_str(), relation.to_count.as_str()]
            .iter()
            .filter(|words| !words.trim().is_empty())
            .map(|words| text::measure_unwrapped(words, &count_style, options.metrics))
            .fold(Size::default(), |so_far, label| {
                Size::new(so_far.width + label.width + 24.0, so_far.height + label.height * 2.0)
            });
        graph.edges.push(EdgeSpec {
            from: relation.from,
            to: relation.to,
            label: Size::new(
                labels[index].width.max(counts.width),
                labels[index].height + counts.height,
            ),
            span: 1,
            ..EdgeSpec::new(0, 0)
        });
    }
    let placed = layered::layout(&graph);

    let mut scene = Scene::new();
    let top = parts::title(&mut scene, source, options, placed.size.width);
    let origin = Point::new(parts::MARGIN, top + parts::MARGIN);
    draw_namespaces(&mut scene, diagram, &placed, origin, options, false);
    draw_relations(&mut scene, diagram, &placed, origin, &labels, options);
    // The titles go on after the lines, on a panel, because a line into a namespace crosses the top
    // of its frame where the title is (`task-2339`).
    draw_namespaces(&mut scene, diagram, &placed, origin, options, true);
    for (index, class) in diagram.classes.iter().enumerate() {
        draw_class(
            &mut scene,
            class,
            &boxes[index],
            placed.nodes[index].moved(origin.x, origin.y),
            options,
        );
    }
    parts::finish(&mut scene);
    scene
}

/// Measure a class box: its name, its annotation, and its two lists of members.
fn measure(class: &Class, options: &Options) -> Measured {
    let name_style = options.style(1.0, true);
    let member_style = options.style(0.85, false);
    let name = text::measure_unwrapped(&class.name, &name_style, options.metrics);
    let annotation = class.annotation.as_ref().map(|words| {
        text::measure_unwrapped(&format!("«{words}»"), &member_style, options.metrics)
    });
    let attributes: Vec<Label> = class
        .attributes
        .iter()
        .map(|words| text::measure_unwrapped(words, &member_style, options.metrics))
        .collect();
    let methods: Vec<Label> = class
        .methods
        .iter()
        .map(|words| text::measure_unwrapped(words, &member_style, options.metrics))
        .collect();

    if class.note {
        let words = text::measure(&class.name, &member_style, options.metrics, text::WRAP);
        return Measured {
            size: Size::new(
                words.width + parts::PADDING_X * 2.0 + 10.0,
                words.height + parts::PADDING_Y * 2.0,
            ),
            name: words,
            annotation: None,
            attributes: Vec::new(),
            methods: Vec::new(),
            name_height: 0.0,
            attribute_height: 0.0,
        };
    }
    if class.lollipop {
        return Measured {
            size: Size::new(
                name.width.max(LOLLIPOP * 2.0) + 8.0,
                LOLLIPOP * 2.0 + 4.0 + name.height,
            ),
            name,
            annotation: None,
            attributes: Vec::new(),
            methods: Vec::new(),
            name_height: 0.0,
            attribute_height: 0.0,
        };
    }
    let row = member_style.size * ROW_SPACING;
    let widest = [name.width]
        .into_iter()
        .chain(annotation.iter().map(|label| label.width))
        .chain(attributes.iter().map(|label| label.width))
        .chain(methods.iter().map(|label| label.width))
        .fold(0.0_f32, f32::max);
    let name_height = name.height
        + annotation.as_ref().map_or(0.0, |label| label.height)
        + parts::PADDING_Y * 2.0;
    let attribute_height = if attributes.is_empty() {
        row * 0.5
    } else {
        row * attributes.len() as f32 + parts::PADDING_Y
    };
    let method_height =
        if methods.is_empty() { row * 0.5 } else { row * methods.len() as f32 + parts::PADDING_Y };
    Measured {
        size: Size::new(
            widest + parts::PADDING_X * 2.0,
            name_height + attribute_height + method_height,
        ),
        name,
        annotation,
        attributes,
        methods,
        name_height,
        attribute_height,
    }
}

/// Draw one class: the box, the two dividers, and the three compartments.
fn draw_class(
    scene: &mut Scene,
    class: &Class,
    measured: &Measured,
    rect: Rect,
    options: &Options,
) {
    let theme = &options.theme;
    let stroke = Stroke::new(theme.node_stroke, parts::LINE);
    if class.note {
        super::shapes::Shape::NotchedRect.draw(
            scene,
            rect,
            Paint::faded(theme.series(2), 36),
            Stroke::new(theme.series(2), parts::LINE),
        );
        parts::centred_label(
            scene,
            &measured.name,
            rect,
            &parts::text_style(options, 0.85, false, theme.text),
        );
        return;
    }
    if class.lollipop {
        scene.add(Item::Circle {
            centre: Point::new(rect.centre().x, rect.top() + LOLLIPOP),
            radius: LOLLIPOP,
            fill: Some(theme.node_fill),
            stroke: Some(Stroke::new(theme.line, parts::LINE)),
        });
        parts::one_line(
            scene,
            &class.name,
            Point::new(rect.centre().x, rect.top() + LOLLIPOP * 2.0 + 4.0),
            &parts::text_style(options, 1.0, true, theme.text),
            Anchor::Middle,
            measured.name.width,
        );
        return;
    }
    scene.add(Item::Rect {
        rect,
        radius: parts::CORNER,
        fill: Some(theme.node_fill),
        stroke: Some(stroke),
    });
    let mut y = rect.top() + parts::PADDING_Y;
    if let Some(annotation) = &measured.annotation {
        parts::one_line(
            scene,
            &format!("«{}»", class.annotation.as_deref().unwrap_or_default()),
            Point::new(rect.centre().x, y),
            &parts::text_style(options, 0.85, false, theme.dim),
            Anchor::Middle,
            annotation.width,
        );
        y += annotation.height;
    }
    parts::one_line(
        scene,
        &class.name,
        Point::new(rect.centre().x, y),
        &parts::text_style(options, 1.0, true, theme.text),
        Anchor::Middle,
        measured.name.width,
    );

    let member_style = parts::text_style(options, 0.85, false, theme.text);
    let row = options.base.size * 0.85 * ROW_SPACING;
    for (offset, list, labels) in [
        (measured.name_height, &class.attributes, &measured.attributes),
        (measured.name_height + measured.attribute_height, &class.methods, &measured.methods),
    ] {
        let divider = rect.top() + offset;
        scene.add(Item::Line {
            points: vec![Point::new(rect.left(), divider), Point::new(rect.right(), divider)],
            stroke,
            dash: Dash::Solid,
        });
        for (index, words) in list.iter().enumerate() {
            parts::one_line(
                scene,
                words,
                Point::new(rect.left() + parts::PADDING_X, divider + 4.0 + row * index as f32),
                &member_style,
                Anchor::Start,
                labels[index].width,
            );
        }
    }
}

/// Draw the frames round the namespaces.
fn draw_namespaces(
    scene: &mut Scene,
    diagram: &Diagram,
    placed: &layered::Placed,
    origin: Point,
    options: &Options,
    titles: bool,
) {
    for (index, name) in diagram.namespaces.iter().enumerate() {
        let frame = placed.groups[index].moved(origin.x, origin.y);
        if frame.width <= 0.0 {
            continue;
        }
        if !titles {
            scene.add(Item::Rect {
                rect: frame,
                radius: parts::CORNER,
                fill: Some(options.theme.group_fill),
                stroke: Some(Stroke::new(options.theme.group_stroke, parts::LINE)),
            });
            continue;
        }
        let width = text::width_of(name, &options.style(0.95, true), options.metrics);
        let height = options.metrics.line_metrics(&options.style(0.95, true)).height();
        scene.add(Item::Rect {
            rect: Rect::new(frame.left() + 8.0, frame.top() + 5.0, width + 8.0, height + 2.0),
            radius: 3.0,
            fill: Some(Paint::solid(options.theme.node_fill.color)),
            stroke: None,
        });
        parts::one_line(
            scene,
            name,
            Point::new(frame.left() + 12.0, frame.top() + 6.0),
            &parts::text_style(options, 0.95, true, options.theme.text),
            Anchor::Start,
            width,
        );
    }
}

/// Draw every relationship: its line, an ending at each end that has one, and its label.
fn draw_relations(
    scene: &mut Scene,
    diagram: &Diagram,
    placed: &layered::Placed,
    origin: Point,
    labels: &[Label],
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
        let outline = |index: usize| {
            let rect = placed.nodes[index].moved(origin.x, origin.y);
            match diagram.classes[index].lollipop {
                true => {
                    Outline::Circle(Point::new(rect.centre().x, rect.top() + LOLLIPOP), LOLLIPOP)
                }
                false => Outline::Rect(rect),
            }
        };
        let path = parts::edge_path(&path, &outline(relation.from), &outline(relation.to));
        let last = path.len() - 1;
        let stroke = Stroke::new(theme.line, parts::LINE);
        let dash = if relation.dashed { parts::DASH } else { Dash::Solid };
        let drawn = parts::trimmed(
            &path,
            parts::ending_inset(relation.tail),
            parts::ending_inset(relation.head),
        );
        scene.add(Item::Line { points: drawn, stroke, dash });
        parts::ending(
            scene,
            relation.head,
            path[last],
            parts::heading(&path),
            theme.line,
            theme.node_fill,
        );
        parts::ending(
            scene,
            relation.tail,
            path[0],
            parts::tail_heading(&path),
            theme.line,
            theme.node_fill,
        );
        if labels[index].is_empty() {
            // Nothing to draw over them, so they can go now.
            draw_counts(scene, relation, &path, options);
            continue;
        }
        let at = placed.labels[index];
        let panel = Rect::around(
            Point::new(at.x + origin.x, at.y + origin.y),
            Size::new(labels[index].width + 8.0, labels[index].height + 2.0),
        );
        // On a panel, so the relationship's own line does not run through the middle of the words.
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
        // After the label, not before it. On a short relationship the label sits close enough to the
        // ends that its panel covered the cardinality entirely, which is how a `"*"` went missing
        // from a picture that was otherwise right.
        draw_counts(scene, relation, &path, options);
    }
}

/// Draw the cardinality beside each end of a relationship.
fn draw_counts(scene: &mut Scene, relation: &Relation, path: &[Point], options: &Options) {
    let style = parts::text_style(options, 0.8, false, options.theme.dim);
    let measure = options.style(0.8, false);
    for (words, at, towards) in [
        (&relation.from_count, path[0], path[1]),
        (&relation.to_count, path[path.len() - 1], path[path.len() - 2]),
    ] {
        if words.trim().is_empty() {
            continue;
        }
        let width = text::width_of(words, &measure, options.metrics);
        // Close to the class it belongs to rather than a fraction of the way along, and moved by
        // half a line rather than a whole one. On a short relationship both of those were wrong in
        // the same direction: the count ended up in the middle, underneath the label's own panel, and
        // a `"*"` went missing from a picture that was otherwise right.
        // Past the ending as well as past the node: an arrowhead reaches back about ten points
        // from the border, and a count drawn there is a grey mark on a grey triangle.
        let clear = parts::HEAD + 12.0;
        let along = (clear / at.distance(towards).max(1.0)).min(0.35);
        let inside = at.towards(towards, along);
        parts::one_line(
            scene,
            words,
            Point::new(inside.x + 8.0, inside.y - measure.size * 0.5),
            &style,
            Anchor::Start,
            width,
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

    #[test]
    fn a_class_with_a_block_takes_its_members() {
        let text =
            "classDiagram\n class Duck {\n +String beakColour\n +swim()\n +quack() void\n }\n";
        let diagram = diagram(text);
        assert_eq!(diagram.classes.len(), 1);
        assert_eq!(diagram.classes[0].attributes, vec!["+String beakColour"]);
        assert_eq!(diagram.classes[0].methods, vec!["+swim()", "+quack() void"]);
    }

    #[test]
    fn brackets_are_what_makes_a_member_a_method() {
        let diagram = diagram("classDiagram\n Duck : +int age\n Duck : +fly()\n");
        assert_eq!(diagram.classes[0].attributes, vec!["+int age"]);
        assert_eq!(diagram.classes[0].methods, vec!["+fly()"]);
    }

    #[test]
    fn all_eight_relationship_arrows_are_read() {
        let text = "classDiagram\n\
            A <|-- B\n C *-- D\n E o-- F\n G --> H\n I -- J\n K ..> L\n M ..|> N\n O .. P\n";
        let diagram = diagram(text);
        let read: Vec<(Ending, Ending, bool)> =
            diagram.relations.iter().map(|r| (r.tail, r.head, r.dashed)).collect();
        assert_eq!(
            read,
            vec![
                (Ending::Hollow, Ending::None, false),
                (Ending::Diamond, Ending::None, false),
                (Ending::HollowDiamond, Ending::None, false),
                (Ending::None, Ending::Arrow, false),
                (Ending::None, Ending::None, false),
                (Ending::None, Ending::Arrow, true),
                (Ending::None, Ending::Hollow, true),
                (Ending::None, Ending::None, true),
            ]
        );
    }

    #[test]
    fn inheritance_puts_the_parent_above_the_child_and_the_triangle_at_the_parent() {
        // The two are different questions and getting them confused gives a picture that is upside
        // down and has its arrowheads at the wrong end.
        let diagram = diagram("classDiagram\n Animal <|-- Dog\n");
        let relation = &diagram.relations[0];
        assert_eq!(diagram.classes[relation.from].name, "Animal");
        assert_eq!(relation.tail, Ending::Hollow, "the triangle is at Animal's end");
        assert_eq!(relation.head, Ending::None);

        let scene =
            check::drawn("classDiagram\n Animal <|-- Dog\n", &options(), &["Animal", "Dog"]);
        let boxes = scene.rects();
        assert!(boxes[0].top() < boxes[1].top(), "the parent is drawn above the child");
    }

    #[test]
    fn cardinalities_and_a_label_are_read_from_a_relationship() {
        let diagram = diagram("classDiagram\n Customer \"1\" --> \"0..*\" Order : places\n");
        let relation = &diagram.relations[0];
        assert_eq!(diagram.classes[relation.from].name, "Customer");
        assert_eq!(diagram.classes[relation.to].name, "Order");
        assert_eq!(relation.from_count, "1");
        assert_eq!(relation.to_count, "0..*");
        assert_eq!(relation.label, "places");
    }

    #[test]
    fn an_annotation_is_read_whichever_way_it_is_written() {
        let inline = diagram("classDiagram\n class Shape\n <<interface>> Shape\n");
        assert_eq!(inline.classes[0].annotation.as_deref(), Some("interface"));
        let nested = diagram("classDiagram\n class Colour {\n <<enumeration>>\n RED\n GREEN\n }\n");
        assert_eq!(nested.classes[0].annotation.as_deref(), Some("enumeration"));
        assert_eq!(nested.classes[0].attributes, vec!["RED", "GREEN"]);
    }

    #[test]
    fn a_namespace_inside_a_namespace_is_drawn_inside_it() {
        let text = "classDiagram\n namespace Platform {\n  namespace Auth {\n   class UserService\n  }\n  class Gateway\n }\n class Outside\n";
        let diagram = read(&Source::read(text).unwrap()).unwrap();
        assert_eq!(diagram.namespace_parents, vec![None, Some(0)]);
        let group_of = |name: &str| diagram.classes[diagram.by_name[name]].group;
        assert_eq!(group_of("UserService"), Some(1));
        assert_eq!(group_of("Gateway"), Some(0));
        assert_eq!(group_of("Outside"), None);
    }

    #[test]
    fn a_namespace_groups_the_classes_declared_in_it() {
        let text =
            "classDiagram\n namespace Shapes {\n class Square\n class Circle\n }\n class Loose\n";
        let diagram = diagram(text);
        assert_eq!(diagram.namespaces, vec!["Shapes"]);
        assert_eq!(diagram.classes[0].group, Some(0));
        assert_eq!(diagram.classes[1].group, Some(0));
        assert_eq!(diagram.classes[2].group, None);
    }

    #[test]
    fn a_generic_is_shown_in_angle_brackets_and_is_one_class() {
        let diagram = diagram(
            "classDiagram\n class Box~T~ {\n +List~int~ items\n }\n Box : +nested() List~List~int~~\n",
        );
        assert_eq!(diagram.classes.len(), 1, "Box~T~ and Box are one class");
        assert_eq!(diagram.classes[0].name, "Box<T>");
        assert_eq!(diagram.classes[0].attributes, vec!["+List<int> items"]);
        assert_eq!(diagram.classes[0].methods, vec!["+nested() List<List<int>>"]);
    }

    #[test]
    fn a_note_is_drawn_and_a_note_for_a_class_is_joined_to_it() {
        let diagram = diagram("classDiagram\n note \"From Duck till Zebra\"\n note for Duck \"can fly<br>can swim\"\n class Duck\n");
        assert!(diagram.classes[0].note);
        assert_eq!(diagram.classes[0].name, "From Duck till Zebra");
        assert_eq!(diagram.classes[1].name, "can fly\ncan swim");
        assert_eq!(diagram.relations.len(), 1);
        assert!(diagram.relations[0].dashed);
        assert_eq!(diagram.classes[diagram.relations[0].to].name, "Duck");
    }

    #[test]
    fn a_name_in_backticks_loses_them() {
        let diagram =
            diagram("classDiagram\n class `Animal Class!`\n `Animal Class!` --> `Car Class`\n");
        assert_eq!(diagram.classes[0].name, "Animal Class!");
        assert_eq!(diagram.classes.len(), 2);
    }

    #[test]
    fn a_lollipop_interface_is_read_on_either_side() {
        let diagram =
            diagram("classDiagram\n class Class01\n Class01 --() bar\n foo ()-- Class01\n");
        let named = |name: &str| diagram.classes.iter().find(|class| class.name == name).unwrap();
        assert!(named("bar").lollipop);
        assert!(named("foo").lollipop);
        assert!(!named("Class01").lollipop);
    }

    #[test]
    fn a_class_diagram_is_drawn_and_keeps_every_property() {
        let text = "classDiagram\n\
            direction TB\n\
            class Animal {\n <<abstract>>\n +String name\n +move() void\n }\n\
            class Dog {\n +bark() void\n }\n\
            class Cat\n\
            Animal <|-- Dog\n Animal <|-- Cat\n\
            Dog \"1\" --> \"*\" Bone : buries\n";
        let scene = check::drawn(
            text,
            &options(),
            &["Animal", "Dog", "Cat", "Bone", "+String name", "+bark() void", "buries"],
        );
        check::no_two_rectangles_overlap(&scene.rects());
    }

    #[test]
    fn a_relationship_with_nothing_on_one_end_says_which_line() {
        let problem = check::refused("classDiagram\n <|-- Dog\n", &options());
        assert_eq!(problem.line, Some(2));
    }
}

#[cfg(test)]
mod cardinalities {
    use super::super::{check, Options};
    use super::*;
    use crate::metrics::FixedMetrics;

    fn options() -> Options<'static> {
        Options::new(Box::leak(Box::new(FixedMetrics::default())))
    }

    #[test]
    fn a_cardinality_is_not_drawn_underneath_the_relationships_own_label() {
        // On a short relationship the two ends' counts and the label all want the middle, and the
        // label is drawn on a panel: a count that lands there disappears entirely, which is how a
        // `"*"` went missing from a picture that was otherwise right.
        let text = "classDiagram\n Scene \"1\" --> \"*\" Item : holds\n";
        let scene = check::drawn(text, &options(), &["Scene", "Item", "holds", "1", "*"]);
        let where_is = |words: &str| {
            scene
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Text { at, text, .. } if text == words => Some(*at),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{words} was not drawn"))
        };
        let (one, star, label) = (where_is("1"), where_is("*"), where_is("holds"));
        // The label is drawn on a panel a line tall, centred on where its text starts.
        let line = options().base.size * 1.4;
        assert!(
            (one.y - label.y).abs() > line,
            "the `1` at {one:?} is inside the label's panel at {label:?}"
        );
        assert!(
            (star.y - label.y).abs() > line,
            "the `*` at {star:?} is inside the label's panel at {label:?}"
        );
        assert!(one.y < label.y && label.y < star.y, "one at each end, the label between them");
    }
}
