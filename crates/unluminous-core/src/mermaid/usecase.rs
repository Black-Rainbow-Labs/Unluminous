//! Use case diagrams: `usecase-beta`.
//!
//! UML's picture of who uses a system for what: actors as stick figures, use cases as ellipses, a
//! system boundary round the use cases it offers, and lines between them. The relationships are UML's:
//! an association, `«include»` and `«extend»` as dashed lines with an open arrow, and generalisation
//! with a hollow triangle. A note hangs off one element on a dotted line, and a JSON table is a two
//! column table of paths and values.
//!
//! It is laid out by the layered layout, with each boundary a group, which is what keeps the
//! boundaries, the figures and the labels apart.
//!
//! Classes, styles, an icon for an actor and edge animation are read and ignored, as they are in every
//! other diagram type: a document does not choose the window's colours, and an icon would have to be
//! fetched.

use std::collections::HashMap;

use super::layered::{self, Direction, EdgeSpec, GroupSpec};
use super::parts::{self, Ending, Outline};
use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke};
use super::shapes::Shape;
use super::source::{self, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// How tall an actor's stick figure is.
const FIGURE: f32 = 44.0;
/// How wide an actor's stick figure is.
const FIGURE_WIDTH: f32 = 30.0;

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Actor { hollow: bool },
    UseCase { rectangle: bool },
    Note,
    Json { rows: Vec<(String, String)> },
}

#[derive(Debug, Clone, PartialEq)]
struct Element {
    id: String,
    label: String,
    kind: Kind,
    stereotype: Option<String>,
    business: bool,
    boundary: Option<usize>,
    /// True once a declaration has said what this is, so a later declaration of a relationship's
    /// endpoint does not turn an actor back into a use case.
    declared: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct Boundary {
    id: String,
    title: String,
    package: bool,
}

/// What a relationship means, which decides how it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Meaning {
    Association,
    Include,
    Extend,
    Generalisation,
    NoteLink,
}

#[derive(Debug, Clone, PartialEq)]
struct Relationship {
    from: usize,
    to: usize,
    meaning: Meaning,
    head: Ending,
    tail: Ending,
    label: String,
    span: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Diagram {
    elements: Vec<Element>,
    by_id: HashMap<String, usize>,
    boundaries: Vec<Boundary>,
    relationships: Vec<Relationship>,
    direction: Direction,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    Ok(draw(&diagram, source, options))
}

/// The statements, with a JSON object or a metadata block written over several lines joined into one.
fn joined(source: &Source) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut open = 0_i32;
    for line in source.statements() {
        let text = line.text.trim();
        if open > 0 {
            if let Some((_, last)) = out.last_mut() {
                last.push(' ');
                last.push_str(text);
            }
        } else {
            out.push((line.number, text.to_owned()));
        }
        open = (open + braces(text)).max(0);
    }
    out
}

/// How many more `{` and `[` than `}` and `]` a line has, outside quotes.
fn braces(text: &str) -> i32 {
    let mut count = 0;
    let mut quoted = false;
    let mut escaped = false;
    for character in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '{' if !quoted => count += 1,
            '}' if !quoted => count -= 1,
            _ => {}
        }
    }
    count
}

fn read(source: &Source) -> Result<Diagram, Problem> {
    let mut diagram = Diagram { direction: Direction::Down, ..Diagram::default() };
    let mut boundary: Option<usize> = None;
    for (number, text) in joined(source) {
        let problem = |reason: &str| Problem {
            line: Some(number),
            text: text.clone(),
            reason: reason.to_owned(),
            unsupported: false,
        };
        let lower = text.to_ascii_lowercase();
        if lower == "end" {
            boundary = None;
            continue;
        }
        if let Some(rest) = after(&text, "direction") {
            if let Some(direction) = Direction::parse(rest) {
                diagram.direction = direction;
            }
            continue;
        }
        if ["classdef", "class", "style", "linkstyle", "click"]
            .iter()
            .any(|word| after(&lower, word).is_some())
        {
            continue;
        }
        if let Some(rest) = after(&text, "systemBoundary") {
            boundary = Some(read_boundary(&mut diagram, rest));
            continue;
        }
        if let Some(rest) = after(&text, "actor") {
            let declared = read_declaration(rest).ok_or_else(|| problem("an actor looks like `actor Customer` or `actor Admin(\"Main administrator\")`."))?;
            let hollow = declared.meta.contains("hollow");
            place(&mut diagram, declared, Kind::Actor { hollow }, boundary);
            continue;
        }
        if let Some(rest) = after(&text, "note for") {
            read_note(&mut diagram, rest).ok_or_else(|| {
                problem("a note looks like `note for Login \"Requires a session\"`.")
            })?;
            continue;
        }
        if let Some(rest) = after(&text, "json") {
            read_json(&mut diagram, rest);
            continue;
        }
        if read_relationship(&mut diagram, &text) {
            continue;
        }
        // `id@{ … }` on its own: settings for a boundary, an element or an edge.
        if let Some(at) = text.find("@{") {
            let id = text[..at].trim();
            if let Some(found) = diagram.boundaries.iter_mut().find(|known| known.id == id) {
                if text[at..].contains("package") {
                    found.package = true;
                }
                continue;
            }
            if id.chars().all(|c| c.is_alphanumeric() || c == '_')
                && !text[..at].contains(['(', '[', '"'])
                && !diagram.by_id.contains_key(id)
            {
                // An edge's settings: animation and the like, which draw nothing.
                if text.contains("anim") || text.contains("curve") {
                    continue;
                }
            }
        }
        let declared = read_declaration(&text).ok_or_else(|| {
            problem("this is not a use case statement. A use case looks like `Login(\"Sign in\")` and a relationship like `Customer --> Login`.")
        })?;
        let rectangle = declared.rectangle;
        place(&mut diagram, declared, Kind::UseCase { rectangle }, boundary);
    }
    Ok(diagram)
}

/// What follows `word` when `text` starts with it as a whole word, ignoring case.
fn after<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let head = text.get(..word.len())?;
    let rest = &text[word.len()..];
    (head.eq_ignore_ascii_case(word) && (rest.is_empty() || rest.starts_with(char::is_whitespace)))
        .then(|| rest.trim())
}

/// A declaration of an actor or a use case, read but not yet placed.
struct Declared {
    id: String,
    label: String,
    rectangle: bool,
    meta: String,
    stereotype: Option<String>,
}

/// An id made from a quoted label: every character that is not part of a word becomes `_`.
fn id_from(label: &str) -> String {
    label.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

/// Read `Id`, `Id("Label")`, `Id[Label]` or `"Label"`, with `@{ … }`, `<<Stereotype>>` and `:::class`
/// after it in that order.
fn read_declaration(text: &str) -> Option<Declared> {
    let mut text = text.trim();
    if let Some(at) = rfind_outside_quotes(text, ":::") {
        text = text[..at].trim();
    }
    let mut stereotype = None;
    if let (Some(open), true) = (rfind_outside_quotes(text, "<<"), text.ends_with(">>")) {
        stereotype = Some(text[open + 2..text.len() - 2].trim().to_owned());
        text = text[..open].trim();
    }
    let mut meta = String::new();
    if let Some(at) = rfind_outside_quotes(text, "@{") {
        meta = text[at..].to_owned();
        text = text[..at].trim();
    }
    if text.is_empty() {
        return None;
    }
    if text.starts_with('"') {
        let label = source::label(text);
        return Some(Declared { id: id_from(&label), label, rectangle: false, meta, stereotype });
    }
    let open = text.find(['(', '[']);
    let (id, label, rectangle) = match open {
        Some(at) => {
            let inner = &text[at + 1..text.len().saturating_sub(1).max(at + 1)];
            (text[..at].trim().to_owned(), source::label(inner), text.as_bytes()[at] == b'[')
        }
        None => (text.to_owned(), text.to_owned(), false),
    };
    if id.is_empty() || !id.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(Declared { id, label, rectangle, meta, stereotype })
}

/// Where `needle` last appears in `text` outside double quotes.
fn rfind_outside_quotes(text: &str, needle: &str) -> Option<usize> {
    let mut quoted = false;
    let mut found = None;
    for (at, character) in text.char_indices() {
        if character == '"' {
            quoted = !quoted;
        } else if !quoted && text[at..].starts_with(needle) {
            found = Some(at);
        }
    }
    found
}

/// Add or update the element a declaration names.
fn place(diagram: &mut Diagram, declared: Declared, kind: Kind, boundary: Option<usize>) {
    let business = declared.meta.contains("business") && declared.meta.contains("true");
    if let Some(&known) = diagram.by_id.get(&declared.id) {
        let element = &mut diagram.elements[known];
        element.label = declared.label;
        element.kind = kind;
        element.business |= business;
        element.declared = true;
        if declared.stereotype.is_some() {
            element.stereotype = declared.stereotype;
        }
        if boundary.is_some() {
            element.boundary = boundary;
        }
        return;
    }
    diagram.elements.push(Element {
        id: declared.id.clone(),
        label: declared.label,
        kind,
        stereotype: declared.stereotype,
        business,
        boundary,
        declared: true,
    });
    diagram.by_id.insert(declared.id, diagram.elements.len() - 1);
}

/// The element an id names, made as an ellipse use case if nothing has declared it yet.
fn endpoint(diagram: &mut Diagram, id: &str) -> usize {
    if let Some(&known) = diagram.by_id.get(id) {
        return known;
    }
    diagram.elements.push(Element {
        id: id.to_owned(),
        label: id.to_owned(),
        kind: Kind::UseCase { rectangle: false },
        stereotype: None,
        business: false,
        boundary: None,
        declared: false,
    });
    diagram.by_id.insert(id.to_owned(), diagram.elements.len() - 1);
    diagram.elements.len() - 1
}

/// `systemBoundary sb1["Payment service"]@{ type: package }` or `systemBoundary "Payment service"`.
fn read_boundary(diagram: &mut Diagram, rest: &str) -> usize {
    let package = rest.contains("package");
    let head = rest.split("@{").next().unwrap_or(rest);
    let head = head.split(":::").next().unwrap_or(head).trim();
    let (id, title) = if head.starts_with('"') {
        let title = source::label(head);
        (id_from(&title), title)
    } else {
        match head.find(['[', '(']) {
            Some(at) => (
                head[..at].trim().to_owned(),
                source::label(&head[at + 1..head.len().saturating_sub(1).max(at + 1)]),
            ),
            None => (head.to_owned(), head.to_owned()),
        }
    };
    diagram.boundaries.push(Boundary { id, title, package });
    diagram.boundaries.len() - 1
}

/// `note for Target "text"`.
fn read_note(diagram: &mut Diagram, rest: &str) -> Option<()> {
    let (target, words) = rest.split_once(char::is_whitespace)?;
    let target = endpoint(diagram, target.trim());
    let note = format!("note-{}", diagram.elements.len());
    diagram.elements.push(Element {
        id: note.clone(),
        label: source::label(words.trim()),
        kind: Kind::Note,
        stereotype: None,
        business: false,
        boundary: None,
        declared: true,
    });
    diagram.by_id.insert(note, diagram.elements.len() - 1);
    let from = diagram.elements.len() - 1;
    diagram.relationships.push(Relationship {
        from,
        to: target,
        meaning: Meaning::NoteLink,
        head: Ending::None,
        tail: Ending::None,
        label: String::new(),
        span: 1,
    });
    Some(())
}

/// `json Payload@{ … }:::class`: a table of every leaf in the object, by its path.
fn read_json(diagram: &mut Diagram, rest: &str) {
    let (id, body) = match rest.find("@{") {
        Some(at) => (rest[..at].trim(), &rest[at + 1..]),
        None => (rest.trim(), "{}"),
    };
    let body = match body.rfind('}') {
        Some(end) => &body[..=end],
        None => body,
    };
    let mut rows = Vec::new();
    let mut reader = Json { text: body.as_bytes(), at: 0 };
    if let Some(value) = reader.value() {
        flatten(&value, String::new(), &mut rows);
    }
    let declared = Declared {
        id: id.to_owned(),
        label: id.to_owned(),
        rectangle: true,
        meta: String::new(),
        stereotype: None,
    };
    place(diagram, declared, Kind::Json { rows }, None);
}

/// A JSON value, read just far enough to be shown.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Object(Vec<(String, Value)>),
    Array(Vec<Value>),
    Leaf(String),
}

/// A small reader for the JSON in a `json` table. It reads strings, numbers, `true`, `false`, `null`,
/// arrays and objects, and gives up quietly on anything else.
struct Json<'a> {
    text: &'a [u8],
    at: usize,
}

impl Json<'_> {
    fn skip(&mut self) {
        while self.at < self.text.len() && self.text[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn value(&mut self) -> Option<Value> {
        self.skip();
        match *self.text.get(self.at)? {
            b'{' => {
                self.at += 1;
                let mut pairs = Vec::new();
                loop {
                    self.skip();
                    if self.text.get(self.at) == Some(&b'}') {
                        self.at += 1;
                        return Some(Value::Object(pairs));
                    }
                    let key = self.string()?;
                    self.skip();
                    if self.text.get(self.at) != Some(&b':') {
                        return None;
                    }
                    self.at += 1;
                    let value = self.value()?;
                    match pairs.iter_mut().find(|(known, _)| *known == key) {
                        Some((_, existing)) => *existing = value,
                        None => pairs.push((key, value)),
                    }
                    self.skip();
                    if self.text.get(self.at) == Some(&b',') {
                        self.at += 1;
                    }
                }
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip();
                    if self.text.get(self.at) == Some(&b']') {
                        self.at += 1;
                        return Some(Value::Array(items));
                    }
                    items.push(self.value()?);
                    self.skip();
                    if self.text.get(self.at) == Some(&b',') {
                        self.at += 1;
                    }
                }
            }
            b'"' => self.string().map(Value::Leaf),
            _ => {
                let start = self.at;
                while self.at < self.text.len() && !b",}] \t\r\n".contains(&self.text[self.at]) {
                    self.at += 1;
                }
                (self.at > start).then(|| {
                    Value::Leaf(String::from_utf8_lossy(&self.text[start..self.at]).into_owned())
                })
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        self.skip();
        if self.text.get(self.at) != Some(&b'"') {
            return None;
        }
        self.at += 1;
        let mut out = Vec::new();
        while let Some(&byte) = self.text.get(self.at) {
            self.at += 1;
            match byte {
                b'"' => return Some(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    if let Some(&next) = self.text.get(self.at) {
                        self.at += 1;
                        out.push(match next {
                            b'n' => b' ',
                            other => other,
                        });
                    }
                }
                other => out.push(other),
            }
        }
        None
    }
}

/// Every leaf of a value, by its path: `address.city`, `items[0].name`.
fn flatten(value: &Value, path: String, rows: &mut Vec<(String, String)>) {
    match value {
        Value::Leaf(words) => rows.push((path, words.clone())),
        Value::Object(pairs) if pairs.is_empty() => rows.push((path, "{}".to_owned())),
        Value::Array(items) if items.is_empty() => rows.push((path, "[]".to_owned())),
        Value::Object(pairs) => {
            for (key, inner) in pairs {
                let next = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                flatten(inner, next, rows);
            }
        }
        Value::Array(items) => {
            for (index, inner) in items.iter().enumerate() {
                flatten(inner, format!("{path}[{index}]"), rows);
            }
        }
    }
}

/// Read a relationship, when the statement is one. Returns false when it is not.
fn read_relationship(diagram: &mut Diagram, text: &str) -> bool {
    let Some(found) = operator(text) else {
        return false;
    };
    let left = without_edge_id(text[..found.start].trim());
    let right = text[found.end..].trim();
    if left.is_empty() {
        return false;
    }
    let mut span = found.span;
    let (meaning, head, tail, label, target) = match found.kind {
        Operator::Dotted => {
            let rest = right.trim_start_matches(':').trim();
            let (word, target) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let meaning = if word.eq_ignore_ascii_case("extend") {
                Meaning::Extend
            } else {
                Meaning::Include
            };
            let label = if meaning == Meaning::Extend { "«extend»" } else { "«include»" };
            (meaning, Ending::Arrow, Ending::None, label.to_owned(), target.trim())
        }
        Operator::Line { head, tail, generalisation } => {
            // `A -- "label" --> B`: the words run to the closing dashes.
            let (label, target, head) = match closing(right) {
                Some((at, end, closing_head, closing_span))
                    if head == Ending::None && !generalisation =>
                {
                    // The extra dashes that ask for a longer line go on the closing side.
                    span = closing_span;
                    (source::label(right[..at].trim()), right[end..].trim(), closing_head)
                }
                _ => (String::new(), right, head),
            };
            let meaning =
                if generalisation { Meaning::Generalisation } else { Meaning::Association };
            let head = if generalisation { Ending::Hollow } else { head };
            (meaning, head, tail, label, target)
        }
    };
    let target = target.trim_start_matches('-').trim_start_matches('>').trim();
    if target.is_empty() {
        return false;
    }
    let from = endpoint(diagram, &source::unquote(left));
    let to = endpoint(diagram, &source::unquote(target));
    diagram.relationships.push(Relationship { from, to, meaning, head, tail, label, span });
    true
}

/// An operator found in a statement.
struct Found {
    start: usize,
    end: usize,
    kind: Operator,
    span: usize,
}

enum Operator {
    /// `..>`, followed by `: include` or `: extend`.
    Dotted,
    Line {
        head: Ending,
        tail: Ending,
        generalisation: bool,
    },
}

/// The first relationship operator in `text`, outside quotes and brackets.
fn operator(text: &str) -> Option<Found> {
    let bytes = text.as_bytes();
    let mut quoted = false;
    let mut depth = 0_i32;
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        match byte {
            b'"' => quoted = !quoted,
            b'(' | b'[' | b'{' if !quoted => depth += 1,
            b')' | b']' | b'}' if !quoted => depth -= 1,
            _ => {}
        }
        if quoted || depth > 0 {
            at += 1;
            continue;
        }
        if text[at..].starts_with("..>") {
            return Some(Found { start: at, end: at + 3, kind: Operator::Dotted, span: 1 });
        }
        if text[at..].starts_with("--") {
            let mut end = at;
            while end < bytes.len() && bytes[end] == b'-' {
                end += 1;
            }
            let span = (end - at).saturating_sub(1).max(1);
            let before = at.checked_sub(1).map(|b| bytes[b]);
            let spaced = at < 2 || bytes[at - 2].is_ascii_whitespace();
            let (tail, start) = match before {
                Some(b'<') => (Ending::Arrow, at - 1),
                Some(b'o') if spaced => (Ending::Circle, at - 1),
                Some(b'x') if spaced => (Ending::Cross, at - 1),
                _ => (Ending::None, at),
            };
            let next = bytes.get(end).copied();
            let after_next = bytes.get(end + 1).copied();
            let (head, end, generalisation) = match (next, after_next) {
                (Some(b'>'), _) => (Ending::Arrow, end + 1, false),
                (Some(b'|'), Some(b'>')) => (Ending::Hollow, end + 2, true),
                (Some(b'o'), next) if next.is_none_or(|b| b.is_ascii_whitespace()) => {
                    (Ending::Circle, end + 1, false)
                }
                (Some(b'x'), next) if next.is_none_or(|b| b.is_ascii_whitespace()) => {
                    (Ending::Cross, end + 1, false)
                }
                _ => (Ending::None, end, false),
            };
            return Some(Found {
                start,
                end,
                kind: Operator::Line { head, tail, generalisation },
                span,
            });
        }
        at += 1;
    }
    None
}

/// The dashes that close a labelled association, after its words: where they start, where they end,
/// and the ending they draw.
fn closing(text: &str) -> Option<(usize, usize, Ending, usize)> {
    let found = operator(text)?;
    match found.kind {
        Operator::Line { head, .. } => Some((found.start, found.end, head, found.span)),
        Operator::Dotted => None,
    }
}

/// `Customer opens@` with its edge id taken off.
fn without_edge_id(text: &str) -> &str {
    match text.strip_suffix('@') {
        Some(before) => match before.rfind(char::is_whitespace) {
            Some(at) => before[..at].trim(),
            None => before,
        },
        None => text,
    }
}

/// An element's words and its size.
struct Measured {
    label: Label,
    stereotype: Option<Label>,
    rows: Vec<(Label, Label)>,
    size: Size,
}

fn measure(element: &Element, options: &Options) -> Measured {
    let style = options.style(0.95, false);
    let small = options.style(0.75, false);
    let label = text::measure(&element.label, &style, options.metrics, 170.0);
    let stereotype = element
        .stereotype
        .as_ref()
        .map(|words| text::measure_unwrapped(&format!("«{words}»"), &small, options.metrics));
    let stereo_height = stereotype.as_ref().map_or(0.0, |label| label.height);
    let stereo_width = stereotype.as_ref().map_or(0.0, |label| label.width);
    let words_width = label.width.max(stereo_width);
    let mut rows = Vec::new();
    let size = match &element.kind {
        Kind::Actor { .. } => Size::new(
            words_width.max(FIGURE_WIDTH) + 16.0,
            FIGURE + 6.0 + stereo_height + label.height,
        ),
        Kind::UseCase { rectangle: true } => {
            Size::new(words_width + 30.0, label.height + stereo_height + 22.0)
        }
        // A block of words `w` by `h` fits in an ellipse `W` by `H` when both are scaled by the square
        // root of two, which puts the corners of the block on the ellipse.
        Kind::UseCase { rectangle: false } => Size::new(
            (words_width * 1.42 + 28.0).max(90.0),
            ((label.height + stereo_height) * 1.42 + 20.0).max(46.0),
        ),
        Kind::Note => Size::new(label.width + 34.0, label.height + 18.0),
        Kind::Json { rows: data } => {
            let mono = options.style(0.8, false);
            for (path, value) in data {
                rows.push((
                    text::measure(path, &mono, options.metrics, 220.0),
                    text::measure(value, &mono, options.metrics, 220.0),
                ));
            }
            let path_width = rows.iter().map(|(path, _)| path.width).fold(0.0, f32::max);
            let value_width = rows.iter().map(|(_, value)| value.width).fold(0.0, f32::max);
            let height: f32 =
                rows.iter().map(|(path, value)| path.height.max(value.height) + 6.0).sum();
            Size::new(
                (path_width + value_width + 40.0).max(label.width + 24.0),
                label.height + 12.0 + height + 6.0,
            )
        }
    };
    Measured { label, stereotype, rows, size }
}

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let measured: Vec<Measured> =
        diagram.elements.iter().map(|element| measure(element, options)).collect();
    let title_style = options.style(0.95, true);
    let mut graph = layered::Graph { direction: diagram.direction, ..layered::Graph::default() };
    for boundary in &diagram.boundaries {
        let title = text::measure_unwrapped(&boundary.title, &title_style, options.metrics);
        let tab = if boundary.package { 8.0 } else { 0.0 };
        graph.groups.push(GroupSpec {
            title: Size::new(title.width, title.height + 6.0 + tab),
            parent: None,
        });
    }
    for (index, element) in diagram.elements.iter().enumerate() {
        graph.add_node(measured[index].size, element.boundary);
    }
    let link_style = options.style(0.82, false);
    let labels: Vec<Label> = diagram
        .relationships
        .iter()
        .map(|relationship| {
            text::measure(&relationship.label, &link_style, options.metrics, text::EDGE_WRAP)
        })
        .collect();
    for (index, relationship) in diagram.relationships.iter().enumerate() {
        graph.edges.push(EdgeSpec {
            label: labels[index].size(),
            span: relationship.span,
            ..EdgeSpec::new(relationship.from, relationship.to)
        });
    }
    let placed = layered::layout(&graph);
    let mut scene = Scene::new();
    let top = parts::title(&mut scene, source, options, placed.size.width);
    let origin = Point::new(parts::MARGIN, top + parts::MARGIN);
    draw_boundaries(&mut scene, diagram, &placed, origin, options, false);
    draw_relationships(&mut scene, diagram, &placed, origin, &labels, options);
    // The titles go on after the lines, on a panel, because a line into a boundary crosses the top of
    // its frame where the title is.
    draw_boundaries(&mut scene, diagram, &placed, origin, options, true);
    for (index, element) in diagram.elements.iter().enumerate() {
        let rect = placed.nodes[index].moved(origin.x, origin.y);
        draw_element(&mut scene, element, &measured[index], rect, options);
    }
    parts::finish(&mut scene);
    scene
}

/// Each boundary's frame, or with `titles` each boundary's title on a panel of its own.
fn draw_boundaries(
    scene: &mut Scene,
    diagram: &Diagram,
    placed: &layered::Placed,
    origin: Point,
    options: &Options,
    titles: bool,
) {
    let theme = &options.theme;
    for (index, boundary) in diagram.boundaries.iter().enumerate() {
        let frame = placed.groups[index].moved(origin.x, origin.y);
        if frame.width <= 0.0 {
            continue;
        }
        let colour = theme.series(index);
        let title_style = options.style(0.95, true);
        let title = text::measure_unwrapped(&boundary.title, &title_style, options.metrics);
        let words = parts::text_style(options, 0.95, true, theme.text);
        let tab = Rect::new(frame.left(), frame.top(), title.width + 24.0, title.height + 8.0);
        match (boundary.package, titles) {
            (true, false) => {
                // A package: a tab on the top left holding the title.
                scene.add(Item::Rect {
                    rect: tab,
                    radius: 3.0,
                    fill: Some(Paint::faded(colour, 60)),
                    stroke: Some(Stroke::new(colour, parts::LINE)),
                });
                let body =
                    Rect::new(frame.left(), tab.bottom(), frame.width, frame.height - tab.height);
                scene.add(Item::Rect {
                    rect: body,
                    radius: 3.0,
                    fill: Some(Paint::faded(colour, 22)),
                    stroke: Some(Stroke::new(colour, parts::LINE)),
                });
            }
            (true, true) => {
                scene.add(Item::Rect {
                    rect: tab.grown(-1.0),
                    radius: 3.0,
                    fill: Some(Paint::solid(theme.node_fill.color)),
                    stroke: None,
                });
                parts::one_line(
                    scene,
                    &boundary.title,
                    Point::new(tab.left() + 12.0, tab.top() + 4.0),
                    &words,
                    Anchor::Start,
                    title.width,
                );
            }
            (false, false) => {
                scene.add(Item::Rect {
                    rect: frame,
                    radius: parts::CORNER,
                    fill: Some(Paint::faded(colour, 22)),
                    stroke: Some(Stroke::new(colour, parts::LINE)),
                });
            }
            (false, true) => {
                let at = Point::new(frame.centre().x, frame.top() + 6.0);
                let panel = Rect::new(
                    at.x - title.width / 2.0 - 5.0,
                    at.y - 1.0,
                    title.width + 10.0,
                    title.height + 2.0,
                );
                scene.add(Item::Rect {
                    rect: panel,
                    radius: 3.0,
                    fill: Some(Paint::solid(theme.node_fill.color)),
                    stroke: None,
                });
                parts::one_line(scene, &boundary.title, at, &words, Anchor::Middle, title.width);
            }
        }
    }
}

/// The shape a line to an element is cut back to.
fn outline(element: &Element, rect: Rect) -> Outline {
    match element.kind {
        Kind::UseCase { rectangle: false } => Outline::Polygon(ellipse(rect)),
        _ => Outline::Rect(rect),
    }
}

/// An ellipse filling `rect`, as points.
fn ellipse(rect: Rect) -> Vec<Point> {
    let centre = rect.centre();
    (0..48)
        .map(|step| {
            let angle = step as f32 / 48.0 * std::f32::consts::TAU;
            Point::new(
                centre.x + rect.width / 2.0 * angle.cos(),
                centre.y + rect.height / 2.0 * angle.sin(),
            )
        })
        .collect()
}

fn draw_relationships(
    scene: &mut Scene,
    diagram: &Diagram,
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
        let from = placed.nodes[relationship.from].moved(origin.x, origin.y);
        let to = placed.nodes[relationship.to].moved(origin.x, origin.y);
        let path = parts::edge_path(
            &path,
            &outline(&diagram.elements[relationship.from], from),
            &outline(&diagram.elements[relationship.to], to),
        );
        if path.len() < 2 {
            continue;
        }
        let dash = match relationship.meaning {
            Meaning::Include | Meaning::Extend => parts::DASH,
            Meaning::NoteLink => Dash::Dashed(2.0, 3.0),
            _ => Dash::Solid,
        };
        let drawn = parts::trimmed(
            &path,
            parts::ending_inset(relationship.tail),
            parts::ending_inset(relationship.head),
        );
        scene.add(Item::Line { points: drawn, stroke: Stroke::new(theme.line, parts::LINE), dash });
        parts::ending(
            scene,
            relationship.head,
            path[path.len() - 1],
            parts::heading(&path),
            theme.line,
            theme.node_fill,
        );
        parts::ending(
            scene,
            relationship.tail,
            path[0],
            parts::tail_heading(&path),
            theme.line,
            theme.node_fill,
        );
        let label = &labels[index];
        if !label.is_empty() {
            let at = placed.labels[index];
            let panel = Rect::around(
                Point::new(at.x + origin.x, at.y + origin.y),
                Size::new(label.width + 8.0, label.height + 2.0),
            );
            scene.add(Item::Rect {
                rect: panel,
                radius: 3.0,
                fill: Some(Paint::solid(theme.node_fill.color)),
                stroke: None,
            });
            parts::centred_label(
                scene,
                label,
                panel,
                &parts::text_style(options, 0.82, false, theme.dim),
            );
        }
    }
}

fn draw_element(
    scene: &mut Scene,
    element: &Element,
    measured: &Measured,
    rect: Rect,
    options: &Options,
) {
    let theme = &options.theme;
    let stroke = Stroke::new(theme.node_stroke, parts::LINE);
    let words = parts::text_style(options, 0.95, false, theme.text);
    let small = parts::text_style(options, 0.75, false, theme.dim);
    let stereo_height = measured.stereotype.as_ref().map_or(0.0, |label| label.height);
    match &element.kind {
        Kind::Actor { hollow } => {
            draw_figure(
                scene,
                Point::new(rect.centre().x, rect.top()),
                *hollow,
                element.business,
                options,
            );
            let mut y = rect.top() + FIGURE + 6.0;
            if let Some(stereotype) = &measured.stereotype {
                parts::label_at(
                    scene,
                    stereotype,
                    Point::new(rect.centre().x, y),
                    &small,
                    Anchor::Middle,
                );
                y += stereotype.height;
            }
            parts::label_at(
                scene,
                &measured.label,
                Point::new(rect.centre().x, y),
                &words,
                Anchor::Middle,
            );
        }
        Kind::UseCase { rectangle } => {
            match rectangle {
                true => Shape::Round.draw(
                    scene,
                    rect,
                    theme.node_fill,
                    Stroke::new(theme.accent, parts::LINE),
                ),
                false => scene.add(Item::Polygon {
                    points: ellipse(rect),
                    fill: Some(theme.node_fill),
                    stroke: Some(Stroke::new(theme.accent, parts::LINE)),
                }),
            }
            if element.business && !rectangle {
                // The business slash: a short line across the left of the ellipse.
                let x = rect.left() + rect.width * 0.2;
                scene.add(Item::Line {
                    points: vec![
                        Point::new(x - 6.0, rect.top() + rect.height * 0.12),
                        Point::new(x + 6.0, rect.bottom() - rect.height * 0.12),
                    ],
                    stroke,
                    dash: Dash::Solid,
                });
            }
            let top = rect.centre().y - (measured.label.height + stereo_height) / 2.0;
            if let Some(stereotype) = &measured.stereotype {
                parts::label_at(
                    scene,
                    stereotype,
                    Point::new(rect.centre().x, top),
                    &small,
                    Anchor::Middle,
                );
            }
            parts::label_at(
                scene,
                &measured.label,
                Point::new(rect.centre().x, top + stereo_height),
                &words,
                Anchor::Middle,
            );
        }
        Kind::Note => {
            Shape::NotchedRect.draw(
                scene,
                rect,
                Paint::faded(theme.series(2), 40),
                Stroke::new(theme.series(2), parts::LINE),
            );
            parts::centred_label(scene, &measured.label, rect, &words);
        }
        Kind::Json { .. } => draw_table(scene, measured, rect, options),
    }
}

/// A stick figure whose head's top is at `top`.
fn draw_figure(scene: &mut Scene, top: Point, hollow: bool, business: bool, options: &Options) {
    let theme = &options.theme;
    let stroke = Stroke::new(theme.text, parts::LINE + 0.3);
    let head = 6.5;
    let centre = Point::new(top.x, top.y + head);
    scene.add(Item::Circle {
        centre,
        radius: head,
        fill: if hollow { None } else { Some(theme.node_fill) },
        stroke: Some(stroke),
    });
    let neck = top.y + head * 2.0;
    let hip = neck + 16.0;
    let line = |scene: &mut Scene, a: Point, b: Point| {
        scene.add(Item::Line { points: vec![a, b], stroke, dash: Dash::Solid })
    };
    line(scene, Point::new(top.x, neck), Point::new(top.x, hip));
    line(
        scene,
        Point::new(top.x - FIGURE_WIDTH / 2.0 + 2.0, neck + 6.0),
        Point::new(top.x + FIGURE_WIDTH / 2.0 - 2.0, neck + 6.0),
    );
    line(scene, Point::new(top.x, hip), Point::new(top.x - 9.0, top.y + FIGURE));
    line(scene, Point::new(top.x, hip), Point::new(top.x + 9.0, top.y + FIGURE));
    if business {
        line(
            scene,
            Point::new(top.x - 10.0, top.y + 2.0),
            Point::new(top.x + 10.0, top.y + head * 2.0),
        );
    }
}

/// A JSON table: the name in a header row, then a row for each path and value.
fn draw_table(scene: &mut Scene, measured: &Measured, rect: Rect, options: &Options) {
    let theme = &options.theme;
    let stroke = Stroke::new(theme.node_stroke, parts::LINE);
    scene.add(Item::Rect { rect, radius: 3.0, fill: Some(theme.node_fill), stroke: Some(stroke) });
    let header = measured.label.height + 12.0;
    scene.add(Item::Rect {
        rect: Rect::new(rect.x, rect.y, rect.width, header),
        radius: 3.0,
        fill: Some(Paint::faded(theme.accent, 50)),
        stroke: None,
    });
    parts::centred_label(
        scene,
        &measured.label,
        Rect::new(rect.x, rect.y, rect.width, header),
        &parts::text_style(options, 0.95, true, theme.text),
    );
    let path_width = measured.rows.iter().map(|(path, _)| path.width).fold(0.0, f32::max);
    let divider = rect.x + path_width + 20.0;
    let mut y = rect.y + header + 3.0;
    let style = parts::text_style(options, 0.8, false, theme.text);
    let dim = parts::text_style(options, 0.8, false, theme.dim);
    for (path, value) in &measured.rows {
        parts::label_at(scene, path, Point::new(rect.x + 10.0, y), &dim, Anchor::Start);
        parts::label_at(scene, value, Point::new(divider + 10.0, y), &style, Anchor::Start);
        y += path.height.max(value.height) + 6.0;
    }
    if !measured.rows.is_empty() {
        scene.add(Item::Line {
            points: vec![Point::new(divider, rect.y + header), Point::new(divider, rect.bottom())],
            stroke,
            dash: Dash::Solid,
        });
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
    fn actors_use_cases_and_boundaries_are_read() {
        let diagram = diagram(
            "usecase-beta\ndirection LR\nactor Customer\nactor Admin(\"Main administrator\")\nsystemBoundary Storefront\n  Browse(\"Browse catalogue\")\n  Report[Generate report]\nend\n\"Reset password\"\nCustomer --> Browse\n",
        );
        assert_eq!(diagram.direction, Direction::Right);
        let names: Vec<&str> =
            diagram.elements.iter().map(|element| element.label.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Customer",
                "Main administrator",
                "Browse catalogue",
                "Generate report",
                "Reset password"
            ]
        );
        assert!(matches!(diagram.elements[0].kind, Kind::Actor { .. }));
        assert_eq!(diagram.elements[3].kind, Kind::UseCase { rectangle: true });
        assert_eq!(diagram.elements[2].boundary, Some(0));
        assert_eq!(diagram.elements[4].id, "Reset_password");
    }

    #[test]
    fn an_endpoint_nobody_declared_is_a_use_case_and_a_later_declaration_wins() {
        let diagram =
            diagram("usecase-beta\nCustomer --> Login\nactor Customer\nLogin(\"Sign in\")\n");
        assert!(
            matches!(diagram.elements[0].kind, Kind::Actor { .. }),
            "declared after its first use"
        );
        assert_eq!(diagram.elements[1].label, "Sign in");
    }

    #[test]
    fn every_relationship_form_is_read() {
        let diagram = diagram(
            "usecase-beta\nactor User\nactor Support\nUser --> Start\nStart <-- Support\nStart -- Finish\nUser --o Start\nStart o-- Support\nUser --x Finish\nCheckout ..> : include Payment\nApplyCoupon ..> : extend Checkout\nApplyCoupon --|> Checkout\nUser -- \"include account details\" --> Login\nCustomer opens@-- \"starts checkout\" ---> Checkout\n",
        );
        let relationships = &diagram.relationships;
        assert_eq!(relationships[0].head, Ending::Arrow);
        assert_eq!(relationships[1].tail, Ending::Arrow);
        assert_eq!((relationships[2].head, relationships[2].tail), (Ending::None, Ending::None));
        assert_eq!(relationships[3].head, Ending::Circle);
        assert_eq!(relationships[4].tail, Ending::Circle);
        assert_eq!(relationships[5].head, Ending::Cross);
        assert_eq!(relationships[6].meaning, Meaning::Include);
        assert_eq!(relationships[7].meaning, Meaning::Extend);
        assert_eq!(relationships[8].meaning, Meaning::Generalisation);
        assert_eq!(relationships[9].label, "include account details");
        assert_eq!(
            relationships[9].meaning,
            Meaning::Association,
            "the word include in a label is still a label"
        );
        assert_eq!(relationships[10].label, "starts checkout");
        assert_eq!(
            diagram.elements[diagram.relationships[10].from].label, "Customer",
            "the edge id is not a node"
        );
        assert_eq!(relationships[10].span, 2);
    }

    #[test]
    fn stereotypes_metadata_and_classes_are_read_off_a_declaration() {
        let diagram = diagram(
            "usecase-beta\nactor SalesAgent(\"Sales agent\")@{ business: true } <<Employee>>\nactor Broker@{ type: hollow, business: true }\nQuote(\"Prepare quote\")@{ business: true } <<Core>>:::critical\n",
        );
        assert_eq!(diagram.elements[0].stereotype.as_deref(), Some("Employee"));
        assert!(diagram.elements[0].business);
        assert_eq!(diagram.elements[1].kind, Kind::Actor { hollow: true });
        assert_eq!(diagram.elements[2].label, "Prepare quote");
        assert_eq!(diagram.elements[2].stereotype.as_deref(), Some("Core"));
    }

    #[test]
    fn a_json_table_is_flattened_into_paths() {
        let diagram = diagram(
            "usecase-beta\njson Payload@{\n  \"2\": \"second in source\",\n  \"enabled\": true,\n  \"address\": { \"city\": \"Oslo\" },\n  \"items\": [{ \"name\": \"Book\" }],\n  \"emptyObject\": {}\n}:::data\n",
        );
        let Kind::Json { rows } = &diagram.elements[0].kind else { panic!("a table") };
        let paths: Vec<&str> = rows.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(paths, vec!["2", "enabled", "address.city", "items[0].name", "emptyObject"]);
        assert_eq!(rows[0].1, "second in source");
        assert_eq!(rows[4].1, "{}");
    }

    #[test]
    fn a_note_hangs_off_its_target_on_a_dotted_line() {
        let diagram = diagram("usecase-beta\nnote for Login \"Requires an active session\"\nactor User\nUser --> Login\n");
        let note =
            diagram.elements.iter().position(|element| element.kind == Kind::Note).expect("a note");
        assert_eq!(diagram.relationships[0].from, note);
        assert_eq!(diagram.relationships[0].meaning, Meaning::NoteLink);
    }

    #[test]
    fn the_complete_example_draws_with_every_word() {
        let scene = check::drawn(
            "usecase-beta\ndirection LR\naccTitle: Online ordering use cases\naccDescr {\n  A customer places an order through the storefront.\n  Staff review the order and inspect its data.\n}\nactor Customer(\"Customer\")\nactor Staff(\"Order staff\")@{ type: hollow, business: true } <<Employee>>\nsystemBoundary ordering[\"Ordering System\"]@{ type: package }:::system\n  Browse(\"Browse products\")\n  Checkout(\"Checkout\") <<Core>>:::critical\n  Payment(\"Process payment\")\n  Review[Review order]\nend\njson OrderData@{\n  \"status\": \"pending\",\n  \"total\": 29.95\n}:::data\nnote for Checkout \"`Validates the **cart** before payment`\"\nCustomer starts@-- \"places order\" ---> Checkout\nCustomer --> Browse\nCheckout pays@..> : include Payment\nStaff --> Review\nReview --> OrderData\nclassDef system stroke:#c8a02a,stroke-width:2px\nstarts@{ animation: fast }\nstyle pays stroke:#6b46c1,stroke-width:2px\n",
            &options(),
            &["Online ordering use cases", "Customer", "Order staff", "«Employee»", "Ordering System", "Checkout", "«include»", "places order", "OrderData", "status", "pending"],
        );
        let texts = scene.texts();
        assert!(
            !texts.iter().any(|text| text.contains("A customer places")),
            "the description block is not drawn"
        );
    }

    #[test]
    fn a_line_that_is_not_a_use_case_statement_says_what_one_looks_like() {
        let problem = check::refused("usecase-beta\nactor User\nwhat is this ???\n", &options());
        assert_eq!(problem.line, Some(3));
        assert!(problem.reason.contains("Customer --> Login"));
    }
}
