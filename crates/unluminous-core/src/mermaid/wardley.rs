//! Wardley maps: `wardley-beta`.
//!
//! A map of a value chain. The vertical axis is how visible a component is to the user and the
//! horizontal axis is how far it has evolved, from genesis on the left to commodity on the right.
//! Components are written `[visibility, evolution]`, which is the order OnlineWardleyMaps uses and the
//! opposite of an `(x, y)` pair.
//!
//! Decisions a reader might disagree with:
//!
//! - **The plot has a fixed natural size.** `size [1100, 600]` is Mermaid's default in pixels. The
//!   editor is narrower than that, so the width is scaled by 0.82 and the height by 0.93, which
//!   makes the default map about 900 by 560 points. A larger `size` makes a larger map, within limits.
//! - **A stage width `@w` is the position where the stage ends**, counted from the left edge of the
//!   plot, so `Genesis@0.2 -> Custom@0.4` ends the first stage at a fifth of the width and the second
//!   at two fifths. A stage with no width shares what is left equally. The last stage always ends at
//!   the right edge.
//! - **A name goes to the right of its circle** unless something is there. The other places tried are
//!   the left, above, below and the four corners. Each place is given a penalty for every rectangle
//!   it overlaps, every link line that crosses it and every part of it outside the plot, and the first
//!   place with the lowest penalty wins, so the result does not depend on anything but the source.
//! - **A pipeline's components sit a fixed distance under their parent**, with their names under
//!   them, inside a frame. A pipeline names no visibility of its own.
//! - **`annotations [v, e]` is read as `[visibility, evolution]`** like every other coordinate pair,
//!   and is where the top left corner of the key box goes. With no `annotations` line the key goes
//!   under the plot.
//! - **Trends** (`Component -.- (x, y)`) are skipped.
//! - Colours come from the theme. The red of an evolution arrow is the theme's sixth series colour.

use super::parts;
use super::scene::{Anchor, Dash, Item, Paint, Point, Rect, Scene, Size, Stroke, TextStyle};
use super::source::{self, Line, Source};
use super::text::{self, Label};
use super::{Options, Problem};

/// How far a component's coordinates are kept from the edge of the plot, so its circle is whole.
const EDGE: f32 = 12.0;
/// The radius of a component's circle.
const CIRCLE: f32 = 6.0;
/// The radius of the shape a sourcing decorator draws behind the circle.
const MARKER: f32 = 13.0;
/// How far under its parent a pipeline's components are drawn.
const PIPE_DROP: f32 = 40.0;
/// The gap between a circle and its name.
const GAP: f32 = 4.0;
/// The most stages an `evolution` line may name.
const MOST_STAGES: usize = 8;
/// The width of the numbered circle an annotation draws.
const BADGE: f32 = 9.0;
/// How wide the key box of numbered annotations is when it is placed on the plot.
const KEY_WIDTH: f32 = 280.0;
/// Added to the penalty of a name for each link line that crosses it.
const CROSSING: f32 = 300.0;

/// The sourcing decision a decorator names, drawn as a shape behind the circle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Build,
    Buy,
    Outsource,
    Market,
}

/// One column of the evolution axis.
#[derive(Debug, Clone, PartialEq)]
struct Stage {
    /// One label, or two for a stage written `Genesis / Concept`.
    labels: Vec<String>,
    /// Where the stage ends, from nought to one.
    end: f32,
}

/// A component, an anchor, or a member of a pipeline.
#[derive(Debug, Clone, PartialEq)]
struct Node {
    name: String,
    vis: f32,
    evo: f32,
    anchor: bool,
    strategy: Option<Strategy>,
    inertia: bool,
    /// Where the author moved the name to, relative to its usual place on the right.
    offset: Option<(f32, f32)>,
    /// Where an `evolve` line says this component is heading.
    evolve: Option<f32>,
    /// The pipeline's parent, for a member of a pipeline.
    parent: Option<String>,
    parent_at: Option<usize>,
    line: Line,
}

/// What a link's line looks like at each end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    None,
    Forward,
    Backward,
    Both,
}

#[derive(Debug, Clone, PartialEq)]
struct Link {
    from: String,
    to: String,
    dashed: bool,
    flow: Flow,
    label: Option<String>,
    line: Line,
}

#[derive(Debug, Clone, PartialEq)]
struct Note {
    text: String,
    vis: f32,
    evo: f32,
}

/// An accelerator, which points right, or a deaccelerator, which points left.
#[derive(Debug, Clone, PartialEq)]
struct Force {
    text: String,
    vis: f32,
    evo: f32,
    accelerates: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct Annotation {
    number: u32,
    vis: f32,
    evo: f32,
    text: String,
}

#[derive(Debug, Clone, Default)]
struct Diagram {
    title: Option<String>,
    size: Option<(f32, f32)>,
    stages: Vec<Stage>,
    nodes: Vec<Node>,
    links: Vec<Link>,
    evolves: Vec<(String, f32, Line)>,
    notes: Vec<Note>,
    forces: Vec<Force>,
    annotations: Vec<Annotation>,
    key_at: Option<(f32, f32)>,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let diagram = read(source)?;
    Ok(draw(&diagram, source, options))
}

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

/// Read every statement, then check that every name a statement uses has been defined.
fn read(source: &Source) -> Result<Diagram, Problem> {
    let mut diagram = Diagram::default();
    let mut pipeline: Option<String> = None;
    for line in source.statements() {
        let text = strip_comment(&line.text);
        if text.is_empty() {
            continue;
        }
        read_statement(&mut diagram, &mut pipeline, line, &text)?;
    }
    resolve(&mut diagram)?;
    Ok(diagram)
}

/// The line without a trailing `%%` comment. A `%%` inside quotes is part of the text.
fn strip_comment(text: &str) -> String {
    let mut quoted = false;
    for (index, character) in text.char_indices() {
        if character == '"' {
            quoted = !quoted;
        } else if !quoted && text[index..].starts_with("%%") {
            return text[..index].trim().to_owned();
        }
    }
    text.trim().to_owned()
}

/// What follows `word` when `text` begins with it as a whole word, ignoring case.
fn word_after<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let head = text.get(..word.len())?;
    if !head.eq_ignore_ascii_case(word) {
        return None;
    }
    let rest = &text[word.len()..];
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

/// Dispatch one statement to the function that reads it.
fn read_statement(
    diagram: &mut Diagram,
    pipeline: &mut Option<String>,
    line: &Line,
    text: &str,
) -> Result<(), Problem> {
    if let Some(rest) = word_after(text, "title") {
        diagram.title = Some(source::label(rest));
    } else if let Some(rest) = word_after(text, "size") {
        diagram.size = Some(read_size(line, rest)?);
    } else if let Some(rest) = word_after(text, "evolution") {
        diagram.stages = read_stages(line, rest)?;
    } else if let Some(rest) = word_after(text, "anchor") {
        read_node(diagram, &None, line, rest, true)?;
    } else if let Some(rest) = word_after(text, "component") {
        read_node(diagram, pipeline, line, rest, false)?;
    } else if let Some(rest) = word_after(text, "pipeline") {
        let name = rest.trim_end_matches('{').trim();
        if name.is_empty() {
            return Err(Problem::at(
                line,
                "a pipeline looks like `pipeline Database {`, with the name of a component.",
            ));
        }
        *pipeline = Some(source::unquote(name));
    } else if text == "{" {
        // The brace of a pipeline written on its own line.
    } else if text == "}" {
        *pipeline = None;
    } else if let Some(rest) = word_after(text, "evolve") {
        read_evolve(diagram, line, rest)?;
    } else if let Some(rest) = word_after(text, "note") {
        let (text, vis, evo) = read_text_and_point(line, rest, "note")?;
        diagram.notes.push(Note { text, vis, evo });
    } else if let Some(rest) = word_after(text, "annotations") {
        match bracketed(rest) {
            Some((numbers, "", "")) if numbers.len() == 2 => {
                diagram.key_at = Some((numbers[0].clamp(0.0, 1.0), numbers[1].clamp(0.0, 1.0)));
            }
            _ => return Err(Problem::at(
                line,
                "`annotations` looks like `annotations [0.10, 0.90]`, the corner of the key box.",
            )),
        }
    } else if let Some(rest) = word_after(text, "annotation") {
        read_annotation(diagram, line, rest)?;
    } else if let Some(rest) = word_after(text, "accelerator") {
        let (text, vis, evo) = read_text_and_point(line, rest, "accelerator")?;
        diagram.forces.push(Force { text, vis, evo, accelerates: true });
    } else if let Some(rest) = word_after(text, "deaccelerator") {
        let (text, vis, evo) = read_text_and_point(line, rest, "deaccelerator")?;
        diagram.forces.push(Force { text, vis, evo, accelerates: false });
    } else if let Some(link) = read_link(line, text) {
        diagram.links.push(link);
    } else if text.contains("-.-") {
        // A trend: `Component -.- (x, y)`. It has no picture here.
    } else {
        return Err(Problem::at(
            line,
            "this is not a Wardley statement. Try `component Name [visibility, evolution]`, `A -> B`, `evolve Name 0.8` or `note \"text\" [0.5, 0.5]`.",
        ));
    }
    Ok(())
}

/// `size [1100, 600]`.
fn read_size(line: &Line, rest: &str) -> Result<(f32, f32), Problem> {
    match bracketed(rest) {
        Some((numbers, "", "")) if numbers.len() == 2 && numbers[0] > 0.0 && numbers[1] > 0.0 => {
            Ok((numbers[0], numbers[1]))
        }
        _ => Err(Problem::at(line, "`size` looks like `size [1100, 600]`, a width and a height.")),
    }
}

/// The numbers in the first `[a, b]` on the line, the text before it and the text after it.
fn bracketed(text: &str) -> Option<(Vec<f32>, &str, &str)> {
    let open = text.find('[')?;
    let close = open + text.get(open..)?.find(']')?;
    let inside = text.get(open + 1..close)?;
    let numbers: Option<Vec<f32>> = inside
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok().filter(|value| value.is_finite()))
        .collect();
    Some((numbers?, text.get(..open)?.trim(), text.get(close + 1..)?.trim()))
}

/// `evolution Genesis / Concept@0.2 -> Custom -> Product -> Commodity`.
fn read_stages(line: &Line, rest: &str) -> Result<Vec<Stage>, Problem> {
    let parts: Vec<&str> = rest.split("->").collect();
    if parts.len() < 2 || parts.len() > MOST_STAGES {
        return Err(Problem::at(
            line,
            "`evolution` names between two and eight stages joined by `->`, such as `evolution Genesis -> Custom -> Product`.",
        ));
    }
    let mut labels = Vec::new();
    let mut widths = Vec::new();
    for part in parts {
        let (words, width) = match part.trim().rsplit_once('@') {
            Some((words, width)) => {
                let Some(width) =
                    width.trim().parse::<f32>().ok().filter(|v| v.is_finite() && *v > 0.0)
                else {
                    return Err(Problem::at(
                        line,
                        format!(
                            "`{}` is not a stage width. Write one like `Genesis@0.25`.",
                            width.trim()
                        ),
                    ));
                };
                (words, Some(width))
            }
            None => (part.trim(), None),
        };
        let names: Vec<String> = words
            .split('/')
            .map(|name| source::label(name.trim()))
            .filter(|name| !name.is_empty())
            .collect();
        if names.is_empty() {
            return Err(Problem::at(line, "a stage on an `evolution` line needs a name."));
        }
        labels.push(names);
        widths.push(width);
    }
    let ends = stage_ends(&widths);
    Ok(labels.into_iter().zip(ends).map(|(labels, end)| Stage { labels, end }).collect())
}

/// Where each stage ends, from the widths written. A stage with none shares what is left.
fn stage_ends(widths: &[Option<f32>]) -> Vec<f32> {
    let count = widths.len();
    let mut ends = Vec::with_capacity(count);
    let mut previous = 0.0_f32;
    for (index, width) in widths.iter().enumerate() {
        let left = (count - index) as f32;
        let wanted = width.unwrap_or(previous + (1.0 - previous) / left);
        let floor = previous + 0.04;
        let ceiling = (1.0 - 0.04 * (left - 1.0)).max(floor);
        let end = if index + 1 == count { 1.0 } else { wanted.max(floor).min(ceiling) };
        ends.push(end);
        previous = end;
    }
    ends
}

/// The stages a map has when it names none.
fn default_stages() -> Vec<Stage> {
    let names = ["Genesis", "Custom Built", "Product (+rental)", "Commodity (+utility)"];
    let ends = [0.17, 0.4, 0.7, 1.0];
    names
        .iter()
        .zip(ends)
        .map(|(name, end)| Stage { labels: vec![(*name).to_owned()], end })
        .collect()
}

/// `component Name [v, e] label [x, y] (build) (inertia)`, and the same for an anchor.
fn read_node(
    diagram: &mut Diagram,
    pipeline: &Option<String>,
    line: &Line,
    rest: &str,
    anchor: bool,
) -> Result<(), Problem> {
    let keyword = if anchor { "anchor" } else { "component" };
    let Some((numbers, name, after)) = bracketed(rest) else {
        return Err(Problem::at(
            line,
            format!("a {keyword} looks like `{keyword} Name [visibility, evolution]`, with both numbers between 0 and 1."),
        ));
    };
    let name = source::unquote(name);
    if name.is_empty() {
        return Err(Problem::at(line, format!("a {keyword} needs a name before its coordinates.")));
    }
    let (vis, evo) = match (pipeline.is_some(), numbers.as_slice()) {
        (true, [.., evo]) => (0.0, *evo),
        (false, [vis, evo]) => (*vis, *evo),
        _ => {
            return Err(Problem::at(
                line,
                format!("a {keyword} takes two numbers, `[visibility, evolution]`; a member of a pipeline takes one, `[evolution]`."),
            ))
        }
    };
    let mut node = Node {
        name,
        vis: vis.clamp(0.0, 1.0),
        evo: evo.clamp(0.0, 1.0),
        anchor,
        strategy: None,
        inertia: false,
        offset: None,
        evolve: None,
        parent: pipeline.clone(),
        parent_at: None,
        line: line.clone(),
    };
    read_node_options(&mut node, line, after)?;
    diagram.nodes.push(node);
    Ok(())
}

/// What follows a node's coordinates: `label [x, y]` and decorators in any order.
fn read_node_options(node: &mut Node, line: &Line, after: &str) -> Result<(), Problem> {
    let mut tail = after.trim();
    while !tail.is_empty() {
        if let Some(rest) = word_after(tail, "label") {
            let Some((offset, "", next)) = bracketed(rest).filter(|(n, _, _)| n.len() == 2) else {
                return Err(Problem::at(
                    line,
                    "`label` looks like `label [-50, 10]`, a move to the right and down.",
                ));
            };
            node.offset = Some((offset[0].clamp(-400.0, 400.0), offset[1].clamp(-400.0, 400.0)));
            tail = next;
        } else if let Some(inner) = tail.strip_prefix('(') {
            let Some(close) = inner.find(')') else {
                return Err(Problem::at(
                    line,
                    "a decorator looks like `(build)`; this one has no closing bracket.",
                ));
            };
            read_decorator(node, line, inner.get(..close).unwrap_or(""))?;
            tail = inner.get(close + 1..).unwrap_or("").trim();
        } else {
            return Err(Problem::at(
                line,
                "after the coordinates a component takes `label [x, y]` and decorators such as `(build)`.",
            ));
        }
    }
    Ok(())
}

fn read_decorator(node: &mut Node, line: &Line, word: &str) -> Result<(), Problem> {
    match word.trim().to_ascii_lowercase().as_str() {
        "inertia" => node.inertia = true,
        "build" => node.strategy = Some(Strategy::Build),
        "buy" => node.strategy = Some(Strategy::Buy),
        "outsource" => node.strategy = Some(Strategy::Outsource),
        "market" => node.strategy = Some(Strategy::Market),
        other => {
            return Err(Problem::at(
                line,
                format!(
                    "`({other})` is not a decorator. Use inertia, build, buy, outsource or market."
                ),
            ))
        }
    }
    Ok(())
}

/// `evolve Name 0.62`.
fn read_evolve(diagram: &mut Diagram, line: &Line, rest: &str) -> Result<(), Problem> {
    let parsed = rest.rsplit_once(char::is_whitespace).and_then(|(name, evo)| {
        let evo = evo.trim().parse::<f32>().ok().filter(|v| v.is_finite())?;
        Some((source::unquote(name), evo.clamp(0.0, 1.0)))
    });
    let Some((name, evo)) = parsed.filter(|(name, _)| !name.is_empty()) else {
        return Err(Problem::at(
            line,
            "`evolve` looks like `evolve Kettle 0.62`, a component and where it is heading.",
        ));
    };
    diagram.evolves.push((name, evo, line.clone()));
    Ok(())
}

/// `note "text" [v, e]`, and the same shape for an accelerator.
fn read_text_and_point(
    line: &Line,
    rest: &str,
    keyword: &str,
) -> Result<(String, f32, f32), Problem> {
    let wrong = || {
        Problem::at(
            line,
            format!("`{keyword}` looks like `{keyword} \"words\" [0.5, 0.5]`, with the words in quotes."),
        )
    };
    let inner = rest.strip_prefix('"').ok_or_else(wrong)?;
    let close = inner.find('"').ok_or_else(wrong)?;
    let words = source::label(inner.get(..close).ok_or_else(wrong)?);
    match bracketed(inner.get(close + 1..).ok_or_else(wrong)?) {
        Some((numbers, "", "")) if numbers.len() == 2 => {
            Ok((words, numbers[0].clamp(0.0, 1.0), numbers[1].clamp(0.0, 1.0)))
        }
        _ => Err(wrong()),
    }
}

/// `annotation 1,[0.5, 0.6] "text"`.
fn read_annotation(diagram: &mut Diagram, line: &Line, rest: &str) -> Result<(), Problem> {
    let wrong = || {
        Problem::at(line, "`annotation` looks like `annotation 1,[0.5, 0.6] \"words\"`, a number, a point and words in quotes.")
    };
    let (number, point) = rest.split_once(',').ok_or_else(wrong)?;
    let number = number.trim().parse::<u32>().map_err(|_| wrong())?;
    let (numbers, before, after) = bracketed(point).ok_or_else(wrong)?;
    if !before.is_empty() || numbers.len() != 2 {
        return Err(wrong());
    }
    let inner = after.strip_prefix('"').ok_or_else(wrong)?;
    let close = inner.find('"').ok_or_else(wrong)?;
    let text = source::label(inner.get(..close).ok_or_else(wrong)?);
    diagram.annotations.push(Annotation {
        number,
        vis: numbers[0].clamp(0.0, 1.0),
        evo: numbers[1].clamp(0.0, 1.0),
        text,
    });
    Ok(())
}

/// A statement written `A -> B`, in any of its forms, or `None` when it has no link operator.
fn read_link(line: &Line, text: &str) -> Option<Link> {
    let (from, operator, rest) = split_at_operator(text)?;
    let parts = source::split_outside_quotes(rest, ';');
    let target = parts.first().map(String::as_str).unwrap_or("");
    let note = parts.get(1).map(|note| source::label(note.trim())).filter(|note| !note.is_empty());
    let (dashed, flow, label) = operator;
    Some(Link {
        from: source::unquote(from),
        to: source::unquote(target.trim()),
        dashed,
        flow,
        label: label.or(note),
        line: line.clone(),
    })
}

type Operator = (bool, Flow, Option<String>);

/// Find the first link operator outside quotes and say what it is, with what lies on either side.
fn split_at_operator(text: &str) -> Option<(&str, Operator, &str)> {
    let mut quoted = false;
    for (index, character) in text.char_indices() {
        if character == '"' {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        let rest = &text[index..];
        let found: Option<(usize, Operator)> = if rest.starts_with("-.->") {
            Some((4, (true, Flow::None, None)))
        } else if rest.starts_with("-->") {
            Some((3, (false, Flow::None, None)))
        } else if rest.starts_with("->") {
            Some((2, (false, Flow::None, None)))
        } else if rest.starts_with("+<>") {
            Some((3, (false, Flow::Both, None)))
        } else if rest.starts_with("+<") {
            Some((2, (false, Flow::Backward, None)))
        } else if rest.starts_with("+>") {
            Some((2, (false, Flow::Forward, None)))
        } else if rest.starts_with("+'") {
            rest.get(2..).and_then(|inner| inner.find("'>")).map(|end| {
                let words = rest.get(2..2 + end).unwrap_or("");
                (end + 4, (false, Flow::Forward, Some(source::label(words))))
            })
        } else {
            None
        };
        if let Some((length, operator)) = found {
            return Some((text.get(..index)?.trim(), operator, text.get(index + length..)?));
        }
    }
    None
}

/// Check that every name used has been defined, and attach what refers to nodes.
fn resolve(diagram: &mut Diagram) -> Result<(), Problem> {
    let names: Vec<String> = diagram.nodes.iter().map(|node| node.name.clone()).collect();
    let find = |name: &str| names.iter().position(|known| known == name);
    let unknown = |line: &Line, name: &str| {
        Problem::at(
            line,
            format!("`{name}` is not a component or an anchor. Define it with `component {name} [visibility, evolution]`."),
        )
    };
    for index in 0..diagram.nodes.len() {
        let Some(parent) = diagram.nodes[index].parent.clone() else {
            continue;
        };
        let line = diagram.nodes[index].line.clone();
        let Some(at) = find(&parent) else {
            return Err(unknown(&line, &parent));
        };
        if diagram.nodes[at].parent.is_some() {
            return Err(Problem::at(
                &line,
                "a pipeline's parent must be a component outside any pipeline.",
            ));
        }
        diagram.nodes[index].parent_at = Some(at);
    }
    for link in &diagram.links {
        for name in [&link.from, &link.to] {
            if find(name).is_none() {
                return Err(unknown(&link.line, name));
            }
        }
    }
    for (name, evo, line) in &diagram.evolves {
        let Some(at) = find(name) else {
            return Err(unknown(line, name));
        };
        diagram.nodes[at].evolve = Some(*evo);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------

/// Everything the drawing steps share.
struct Ctx<'a, 'b> {
    diagram: &'a Diagram,
    options: &'a Options<'b>,
    plot: Rect,
    /// Where each node's centre is.
    points: Vec<Point>,
    /// Each node's name, measured.
    names: Vec<Label>,
}

/// What a link meets at each end.
enum Shape {
    Circle(f32),
    Box(Rect),
}

/// Keep `value` between `low` and `high`, and never panic when `high` is below `low`.
fn fit(value: f32, low: f32, high: f32) -> f32 {
    if high < low {
        low
    } else {
        value.max(low).min(high)
    }
}

/// `rect`, moved the least it takes to lie inside `bounds` (or against its top left if it is too big).
fn keep_inside(rect: Rect, bounds: &Rect) -> Rect {
    let x = fit(rect.x, bounds.left() + 2.0, bounds.right() - 2.0 - rect.width);
    let y = fit(rect.y, bounds.top() + 2.0, bounds.bottom() - 2.0 - rect.height);
    Rect::new(x, y, rect.width, rect.height)
}

/// Where a coordinate pair falls on the plot.
fn place(plot: &Rect, vis: f32, evo: f32) -> Point {
    let x = plot.x + evo.clamp(0.0, 1.0) * plot.width;
    let y = plot.y + (1.0 - vis.clamp(0.0, 1.0)) * plot.height;
    Point::new(
        fit(x, plot.left() + EDGE, plot.right() - EDGE),
        fit(y, plot.top() + EDGE, plot.bottom() - EDGE),
    )
}

fn draw(diagram: &Diagram, source: &Source, options: &Options) -> Scene {
    let mut scene = Scene::new();
    let mut titled = source.clone();
    if titled.title.is_none() {
        titled.title.clone_from(&diagram.title);
    }
    let axis = options.style(0.85, false);
    let gutter = ["Invisible", "Visible"]
        .iter()
        .map(|word| text::width_of(word, &axis, options.metrics))
        .fold(0.0_f32, f32::max)
        + 12.0;
    let (width, height) = diagram.size.unwrap_or((1100.0, 600.0));
    let plot_width = (width.clamp(300.0, 3000.0) * 0.82).min(1300.0);
    let plot_height = (height.clamp(200.0, 3000.0) * 0.93).min(1000.0);
    let total = parts::MARGIN * 2.0 + gutter + plot_width;
    let top = parts::title(&mut scene, &titled, options, total);
    let plot = Rect::new(parts::MARGIN + gutter, top + parts::MARGIN, plot_width, plot_height);
    let stages = if diagram.stages.is_empty() { default_stages() } else { diagram.stages.clone() };

    draw_frame(&mut scene, &plot, &stages, options);
    let below = draw_stage_labels(&mut scene, &plot, &stages, options);

    let ctx = build_context(diagram, options, plot);
    let key = KeyBox::new(&ctx, below + parts::MARGIN);
    let mut taken: Vec<Rect> = Vec::new();
    let mut segments: Vec<(Point, Point)> = Vec::new();
    let mut name_rects: Vec<Option<Rect>> = vec![None; diagram.nodes.len()];

    taken.extend(key.placed_rect().map(|rect| rect.grown(2.0)));
    for annotation in &diagram.annotations {
        let at = place(&plot, annotation.vis, annotation.evo);
        taken.push(Rect::around(at, Size::new(BADGE * 2.0, BADGE * 2.0)).grown(2.0));
    }
    fix_the_certain_names(&ctx, &mut name_rects, &mut taken);
    let mut forces = force_rects(&ctx);
    for force in &mut forces {
        let moved = settle(force.whole, &ctx.plot, &taken);
        force.arrow = force.arrow.moved(moved.x - force.whole.x, moved.y - force.whole.y);
        force.whole = moved;
        taken.push(moved.grown(2.0));
    }
    let mut notes = note_rects(&ctx);
    for (_, rect) in &mut notes {
        *rect = settle(*rect, &ctx.plot, &taken);
        taken.push(rect.grown(2.0));
    }
    let frames = pipeline_frames(&ctx, &name_rects);
    taken.extend(frames.iter().map(|(_, rect)| rect.grown(2.0)));
    let pills = draw_links(&mut scene, &ctx, &name_rects, &mut taken, &mut segments);
    draw_evolutions(&mut scene, &ctx, &mut taken);
    place_the_other_names(&ctx, &mut name_rects, &mut taken, &segments);

    draw_pipeline_frames(&mut scene, &frames, options);
    draw_link_pills(&mut scene, &pills, options);
    draw_forces(&mut scene, &forces, options);
    draw_notes(&mut scene, &notes, options);
    draw_nodes(&mut scene, &ctx, &name_rects);
    draw_annotations(&mut scene, &ctx);
    key.draw(&mut scene, &ctx);

    scene.claim(Rect::new(0.0, 0.0, total, key.bottom(below)));
    parts::finish(&mut scene);
    scene
}

/// Measure every name and work out where every node's centre is.
fn build_context<'a, 'b>(
    diagram: &'a Diagram,
    options: &'a Options<'b>,
    plot: Rect,
) -> Ctx<'a, 'b> {
    let names: Vec<Label> = diagram
        .nodes
        .iter()
        .map(|node| {
            let style = options.style(if node.anchor { 1.0 } else { 0.9 }, node.anchor);
            text::measure_unwrapped(&node.name, &style, options.metrics)
        })
        .collect();
    let mut points: Vec<Point> =
        diagram.nodes.iter().map(|node| place(&plot, node.vis, node.evo)).collect();
    for (index, node) in diagram.nodes.iter().enumerate() {
        let Some(parent) = node.parent_at else {
            continue;
        };
        let lowest = plot.bottom() - EDGE - CIRCLE - GAP - names[index].height - 6.0;
        let x = place(&plot, 0.0, node.evo).x;
        points[index] = Point::new(x, fit(points[parent].y + PIPE_DROP, plot.top() + EDGE, lowest));
    }
    Ctx { diagram, options, plot, points, names }
}

/// The plot's outline, the dashed line between stages and the labels of the vertical axis.
fn draw_frame(scene: &mut Scene, plot: &Rect, stages: &[Stage], options: &Options) {
    scene.add(Item::Rect {
        rect: *plot,
        radius: 0.0,
        fill: None,
        stroke: Some(Stroke::new(options.theme.grid, 1.0)),
    });
    for stage in stages.iter().take(stages.len().saturating_sub(1)) {
        let x = plot.x + stage.end * plot.width;
        scene.add(Item::Line {
            points: vec![Point::new(x, plot.top()), Point::new(x, plot.bottom())],
            stroke: Stroke::new(options.theme.grid, 1.0),
            dash: parts::DASH,
        });
    }
    let style = parts::text_style(options, 0.85, false, options.theme.dim);
    let measure = options.style(0.85, false);
    let lh = text::measure_unwrapped("Visible", &measure, options.metrics).line_height;
    for (words, y) in [("Visible", plot.top() + 4.0), ("Invisible", plot.bottom() - lh - 4.0)] {
        let width = text::width_of(words, &measure, options.metrics);
        parts::one_line(scene, words, Point::new(plot.x - 8.0, y), &style, Anchor::End, width);
    }
}

/// The stage names under the plot, each centred in its column. Returns where the last one ends.
fn draw_stage_labels(scene: &mut Scene, plot: &Rect, stages: &[Stage], options: &Options) -> f32 {
    let style = options.style(0.85, false);
    let drawn = parts::text_style(options, 0.85, false, options.theme.dim);
    let mut start = 0.0_f32;
    let mut bottom = plot.bottom();
    for stage in stages {
        let column = (stage.end - start) * plot.width;
        let label = text::measure(
            &stage.labels.join("\n"),
            &style,
            options.metrics,
            (column - 8.0).max(20.0),
        );
        let centre = plot.x + (start + stage.end) / 2.0 * plot.width;
        parts::label_at(
            scene,
            &label,
            Point::new(centre, plot.bottom() + 6.0),
            &drawn,
            Anchor::Middle,
        );
        bottom = bottom.max(plot.bottom() + 6.0 + label.height);
        start = stage.end;
    }
    bottom
}

// --- notes and forces -------------------------------------------------------------------------

/// Every note, measured and placed. A note is text with its left edge at the point.
fn note_rects(ctx: &Ctx) -> Vec<(Label, Rect)> {
    let style = ctx.options.style(0.85, false);
    ctx.diagram
        .notes
        .iter()
        .map(|note| {
            let label = text::measure(&note.text, &style, ctx.options.metrics, text::WRAP);
            let at = place(&ctx.plot, note.vis, note.evo);
            let rect = Rect::new(at.x, at.y - label.height / 2.0, label.width, label.height);
            let rect = keep_inside(rect, &ctx.plot);
            (label, rect)
        })
        .collect()
}

fn draw_notes(scene: &mut Scene, notes: &[(Label, Rect)], options: &Options) {
    let style =
        TextStyle { italic: true, ..parts::text_style(options, 0.85, false, options.theme.dim) };
    for (label, rect) in notes {
        // On a panel, because a dependency line can run under a note (`task-2339`).
        scene.add(Item::Rect {
            rect: Rect::new(rect.x - 3.0, rect.y - 2.0, label.width + 6.0, label.height + 4.0),
            radius: 3.0,
            fill: Some(Paint::faded(options.theme.node_fill.color, 230)),
            stroke: None,
        });
        parts::label_at(scene, label, Point::new(rect.x, rect.y), &style, Anchor::Start);
    }
}

/// An accelerator or deaccelerator, measured and placed.
struct ForceBox {
    label: Label,
    /// The arrow and its label together.
    whole: Rect,
    /// Just the arrow.
    arrow: Rect,
    accelerates: bool,
}

/// Every accelerator and deaccelerator, placed with its arrow centred on the point.
fn force_rects(ctx: &Ctx) -> Vec<ForceBox> {
    let style = ctx.options.style(0.85, false);
    ctx.diagram
        .forces
        .iter()
        .map(|force| {
            let label = text::measure_unwrapped(&force.text, &style, ctx.options.metrics);
            let length = label.width.max(70.0);
            let at = place(&ctx.plot, force.vis, force.evo);
            let whole = Rect::new(at.x - length / 2.0, at.y - 11.0, length, 25.0 + label.height);
            let whole = keep_inside(whole, &ctx.plot);
            let arrow = Rect::new(whole.x, whole.y, length, 22.0);
            ForceBox { label, whole, arrow, accelerates: force.accelerates }
        })
        .collect()
}

/// A block arrow pointing right for an accelerator and left for a deaccelerator, with its words under it.
fn draw_forces(scene: &mut Scene, forces: &[ForceBox], options: &Options) {
    let style = parts::text_style(options, 0.85, false, options.theme.text);
    for force in forces {
        let colour = options.theme.series(if force.accelerates { 1 } else { 5 });
        let (a, head) = (force.arrow, 16.0_f32.min(force.arrow.width / 2.0));
        let (mid, top, bottom) = (a.centre().y, a.top(), a.bottom());
        let (shaft_top, shaft_bottom) = (mid - 5.0, mid + 5.0);
        let points = if force.accelerates {
            let neck = a.right() - head;
            vec![
                Point::new(a.left(), shaft_top),
                Point::new(neck, shaft_top),
                Point::new(neck, top),
                Point::new(a.right(), mid),
                Point::new(neck, bottom),
                Point::new(neck, shaft_bottom),
                Point::new(a.left(), shaft_bottom),
            ]
        } else {
            let neck = a.left() + head;
            vec![
                Point::new(a.right(), shaft_top),
                Point::new(neck, shaft_top),
                Point::new(neck, top),
                Point::new(a.left(), mid),
                Point::new(neck, bottom),
                Point::new(neck, shaft_bottom),
                Point::new(a.right(), shaft_bottom),
            ]
        };
        scene.add(Item::Polygon {
            points,
            fill: Some(Paint::faded(colour, 90)),
            stroke: Some(Stroke::new(colour, parts::LINE)),
        });
        let at = Point::new(a.centre().x, a.bottom() + 3.0);
        parts::label_at(scene, &force.label, at, &style, Anchor::Middle);
    }
}

/// Where an author placed `rect`, or the nearest place to it that leaves things clear.
///
/// The author chose the spot without knowing how wide the words would be, so a note can land on a
/// circle. Moves up and down by a line, then sideways, are tried in that order and the cheapest wins.
fn settle(rect: Rect, plot: &Rect, taken: &[Rect]) -> Rect {
    let step = rect.height + 6.0;
    let mut best: Option<(f32, Rect)> = None;
    for dx in [0.0, 60.0, -60.0, 120.0, -120.0] {
        for dy in [0.0, step, -step, step * 2.0, -step * 2.0, step * 3.0, -step * 3.0] {
            let moved = keep_inside(rect.moved(dx, dy), plot);
            let cost = penalty(&moved.grown(1.0), taken, plot, &[]);
            if best.is_none_or(|(lowest, _)| cost < lowest) {
                best = Some((cost, moved));
            }
            if cost <= 0.0 {
                return moved;
            }
        }
    }
    best.map_or(rect, |(_, moved)| moved)
}

// --- names ------------------------------------------------------------------------------------

/// How far from the centre the circle, its sourcing shape and its inertia bar reach.
fn reach(node: &Node) -> f32 {
    if node.anchor {
        0.0
    } else if node.strategy.is_some() {
        MARKER
    } else {
        CIRCLE
    }
}

/// How far a name must stay from the centre of its node, to the right.
fn clearance(node: &Node) -> f32 {
    reach(node) + if node.inertia { 10.0 } else { 0.0 }
}

/// Fix the names that have only one possible place: an anchor's, and a pipeline member's.
/// Their circles and the anchors' words are put in `taken` as well, for the others to avoid.
fn fix_the_certain_names(ctx: &Ctx, name_rects: &mut [Option<Rect>], taken: &mut Vec<Rect>) {
    for (index, node) in ctx.diagram.nodes.iter().enumerate() {
        let centre = ctx.points[index];
        let size = ctx.names[index].size();
        let rect = if node.anchor {
            Some(keep_inside(Rect::around(centre, size), &ctx.plot))
        } else if node.parent_at.is_some() {
            let below = Rect::new(
                centre.x - size.width / 2.0,
                centre.y + CIRCLE + GAP,
                size.width,
                size.height,
            );
            Some(keep_inside(below, &ctx.plot))
        } else {
            None
        };
        if let Some(rect) = rect {
            name_rects[index] = Some(rect);
            taken.push(rect.grown(2.0));
        }
        if !node.anchor {
            let half = clearance(node) + 2.0;
            taken.push(Rect::around(centre, Size::new(half * 2.0, half * 2.0)));
        }
    }
}

/// The places a name could go, best first.
fn candidates(centre: Point, node: &Node, size: Size) -> Vec<Rect> {
    let away = clearance(node) + GAP;
    let (w, h) = (size.width, size.height);
    let right = Rect::new(centre.x + away, centre.y - h / 2.0, w, h);
    if let Some((dx, dy)) = node.offset {
        return vec![right.moved(dx, dy)];
    }
    vec![
        right,
        Rect::new(centre.x - away - w, centre.y - h / 2.0, w, h),
        Rect::new(centre.x - w / 2.0, centre.y - reach(node) - GAP - h, w, h),
        Rect::new(centre.x - w / 2.0, centre.y + reach(node) + GAP, w, h),
        Rect::new(centre.x + away, centre.y + 2.0, w, h),
        Rect::new(centre.x + away, centre.y - h - 2.0, w, h),
        Rect::new(centre.x - away - w, centre.y + 2.0, w, h),
        Rect::new(centre.x - away - w, centre.y - h - 2.0, w, h),
        Rect::new(centre.x - 8.0, centre.y + reach(node) + GAP, w, h),
        Rect::new(centre.x + 8.0 - w, centre.y + reach(node) + GAP, w, h),
        Rect::new(centre.x - 8.0, centre.y - reach(node) - GAP - h, w, h),
        Rect::new(centre.x + 8.0 - w, centre.y - reach(node) - GAP - h, w, h),
        Rect::new(centre.x + away, centre.y + h + 4.0, w, h),
        Rect::new(centre.x + away, centre.y - h * 2.0 - 4.0, w, h),
        Rect::new(centre.x - away - w, centre.y + h + 4.0, w, h),
        Rect::new(centre.x - away - w, centre.y - h * 2.0 - 4.0, w, h),
    ]
}

/// The area two rectangles share.
fn shared_area(a: &Rect, b: &Rect) -> f32 {
    let width = a.right().min(b.right()) - a.left().max(b.left());
    let height = a.bottom().min(b.bottom()) - a.top().max(b.top());
    if width > 0.0 && height > 0.0 {
        width * height
    } else {
        0.0
    }
}

/// The area of `rect` that lies outside `bounds`.
fn outside_area(rect: &Rect, bounds: &Rect) -> f32 {
    rect.width * rect.height - shared_area(rect, bounds)
}

/// True when the line from `a` to `b` passes through `rect`.
fn crosses(rect: &Rect, a: Point, b: Point) -> bool {
    let steps = (a.distance(b) / 4.0).ceil().clamp(2.0, 400.0) as usize;
    (0..=steps).any(|step| rect.contains(a.towards(b, step as f32 / steps as f32)))
}

/// How bad it is to put a name at `rect`: nothing is free.
fn penalty(rect: &Rect, taken: &[Rect], plot: &Rect, segments: &[(Point, Point)]) -> f32 {
    let mut total: f32 = taken.iter().map(|other| shared_area(rect, other)).sum::<f32>() * 5.0;
    total += outside_area(rect, plot) * 2.0;
    total += segments.iter().filter(|(a, b)| crosses(rect, *a, *b)).count() as f32 * CROSSING;
    total
}

/// Place every name not yet placed, in the order written, each avoiding all that went before.
fn place_the_other_names(
    ctx: &Ctx,
    name_rects: &mut [Option<Rect>],
    taken: &mut Vec<Rect>,
    segments: &[(Point, Point)],
) {
    for (index, node) in ctx.diagram.nodes.iter().enumerate() {
        if name_rects[index].is_some() {
            continue;
        }
        let options = candidates(ctx.points[index], node, ctx.names[index].size());
        let mut best: Option<(f32, Rect)> = None;
        for rect in options {
            let cost = penalty(&rect.grown(1.0), taken, &ctx.plot, segments);
            if best.is_none_or(|(lowest, _)| cost < lowest) {
                best = Some((cost, rect));
            }
            if cost <= 0.0 {
                break;
            }
        }
        if let Some((_, rect)) = best {
            name_rects[index] = Some(rect);
            taken.push(rect.grown(2.0));
        }
    }
}

// --- pipelines --------------------------------------------------------------------------------

/// The frame round each pipeline's members and their names, with the index of the parent.
fn pipeline_frames(ctx: &Ctx, name_rects: &[Option<Rect>]) -> Vec<(usize, Rect)> {
    let mut frames: Vec<(usize, Rect)> = Vec::new();
    for (index, node) in ctx.diagram.nodes.iter().enumerate() {
        let Some(parent) = node.parent_at else {
            continue;
        };
        let centre = ctx.points[index];
        let mut rect = Rect::around(centre, Size::new(CIRCLE * 2.0, CIRCLE * 2.0));
        if let Some(name) = name_rects[index] {
            rect = rect.union(&name);
        }
        match frames.iter_mut().find(|(owner, _)| *owner == parent) {
            Some((_, frame)) => *frame = frame.union(&rect),
            None => frames.push((parent, rect)),
        }
    }
    for (_, frame) in &mut frames {
        *frame = frame.grown(8.0);
    }
    frames
}

fn draw_pipeline_frames(scene: &mut Scene, frames: &[(usize, Rect)], options: &Options) {
    for (_, rect) in frames {
        scene.add(Item::Rect {
            rect: *rect,
            radius: parts::CORNER,
            fill: Some(options.theme.group_fill),
            stroke: Some(Stroke::new(options.theme.group_stroke, 1.0)),
        });
    }
}

// --- links ------------------------------------------------------------------------------------

/// A label on a link, in a small pill over the line.
struct Pill {
    rect: Rect,
    label: Label,
}

/// What a link starts or ends against.
fn shape_of(ctx: &Ctx, name_rects: &[Option<Rect>], index: usize) -> Shape {
    let node = &ctx.diagram.nodes[index];
    match (node.anchor, name_rects[index]) {
        (true, Some(rect)) => Shape::Box(rect.grown(3.0)),
        _ => Shape::Circle(clearance_for_links(node)),
    }
}

fn clearance_for_links(node: &Node) -> f32 {
    reach(node) + 2.0
}

/// Where a line from inside `shape` towards `towards` leaves it.
fn leave(shape: &Shape, centre: Point, towards: Point) -> Point {
    let (dx, dy) = (towards.x - centre.x, towards.y - centre.y);
    let length = (dx * dx + dy * dy).sqrt();
    if length < f32::EPSILON {
        return centre;
    }
    match shape {
        Shape::Circle(radius) => {
            Point::new(centre.x + dx / length * radius, centre.y + dy / length * radius)
        }
        Shape::Box(rect) => {
            let across =
                if dx.abs() < f32::EPSILON { f32::INFINITY } else { rect.width / 2.0 / dx.abs() };
            let down =
                if dy.abs() < f32::EPSILON { f32::INFINITY } else { rect.height / 2.0 / dy.abs() };
            let t = across.min(down).min(1.0);
            Point::new(centre.x + dx * t, centre.y + dy * t)
        }
    }
}

/// Draw every link and return the pills that carry their words.
fn draw_links(
    scene: &mut Scene,
    ctx: &Ctx,
    name_rects: &[Option<Rect>],
    taken: &mut Vec<Rect>,
    segments: &mut Vec<(Point, Point)>,
) -> Vec<Pill> {
    let mut pills = Vec::new();
    let find = |name: &str| ctx.diagram.nodes.iter().position(|node| node.name == name);
    let label_style = ctx.options.style(0.8, false);
    for link in &ctx.diagram.links {
        let (Some(from), Some(to)) = (find(&link.from), find(&link.to)) else {
            continue;
        };
        let (centre_a, centre_b) = (ctx.points[from], ctx.points[to]);
        if centre_a.distance(centre_b) < 1.0 {
            continue;
        }
        let start = leave(&shape_of(ctx, name_rects, from), centre_a, centre_b);
        let end = leave(&shape_of(ctx, name_rects, to), centre_b, centre_a);
        segments.push((start, end));
        draw_one_link(scene, ctx.options, link, start, end);
        if let Some(words) = &link.label {
            let label = text::measure_unwrapped(words, &label_style, ctx.options.metrics);
            let middle = start.towards(end, 0.5);
            let size = Size::new(label.width + 8.0, label.height + 2.0);
            let rect = keep_inside(Rect::around(middle, size), &ctx.plot);
            taken.push(rect.grown(2.0));
            pills.push(Pill { rect, label });
        }
    }
    pills
}

/// One link's line, thicker and with arrowheads when it is a flow.
fn draw_one_link(scene: &mut Scene, options: &Options, link: &Link, start: Point, end: Point) {
    let flows = link.flow != Flow::None;
    let colour = options.theme.line;
    let width = if flows { 2.5 } else { 1.2 };
    let forward = matches!(link.flow, Flow::Forward | Flow::Both);
    let backward = matches!(link.flow, Flow::Backward | Flow::Both);
    let inset = parts::HEAD * 0.9;
    let from = if backward {
        start.towards(end, (inset / start.distance(end).max(1.0)).min(0.5))
    } else {
        start
    };
    let to = if forward {
        end.towards(start, (inset / start.distance(end).max(1.0)).min(0.5))
    } else {
        end
    };
    scene.add(Item::Line {
        points: vec![from, to],
        stroke: Stroke::new(colour, width),
        dash: if link.dashed { parts::DASH } else { Dash::Solid },
    });
    if forward {
        let direction = Point::new(end.x - start.x, end.y - start.y);
        scene.add(Item::Polygon {
            points: parts::arrow_head(end, direction, parts::HEAD),
            fill: Some(Paint::solid(colour)),
            stroke: None,
        });
    }
    if backward {
        let direction = Point::new(start.x - end.x, start.y - end.y);
        scene.add(Item::Polygon {
            points: parts::arrow_head(start, direction, parts::HEAD),
            fill: Some(Paint::solid(colour)),
            stroke: None,
        });
    }
}

fn draw_link_pills(scene: &mut Scene, pills: &[Pill], options: &Options) {
    let style = parts::text_style(options, 0.8, false, options.theme.dim);
    for pill in pills {
        scene.add(Item::Rect {
            rect: pill.rect,
            radius: parts::CORNER,
            fill: Some(options.theme.node_fill),
            stroke: Some(Stroke::new(options.theme.node_stroke, 1.0)),
        });
        parts::centred_label(scene, &pill.label, pill.rect, &style);
    }
}

// --- evolution --------------------------------------------------------------------------------

/// Draw the red dashed arrow and the ghost circle of every `evolve`, and keep names off them.
fn draw_evolutions(scene: &mut Scene, ctx: &Ctx, taken: &mut Vec<Rect>) {
    let colour = ctx.options.theme.series(5);
    for (index, node) in ctx.diagram.nodes.iter().enumerate() {
        let Some(target) = node.evolve else {
            continue;
        };
        let from = ctx.points[index];
        let goal = Point::new(place(&ctx.plot, 0.0, target).x, from.y);
        let span = goal.x - from.x;
        if span.abs() < clearance(node) + CIRCLE * 2.0 + 6.0 {
            continue;
        }
        let way = if span > 0.0 { 1.0 } else { -1.0 };
        let start = Point::new(from.x + way * (clearance(node) + 3.0), from.y);
        let tip = Point::new(goal.x - way * (CIRCLE + 2.0), from.y);
        let stem = Point::new(tip.x - way * parts::HEAD * 0.9, from.y);
        scene.add(Item::Circle {
            centre: goal,
            radius: CIRCLE,
            fill: None,
            stroke: Some(Stroke::new(colour, parts::LINE)),
        });
        scene.add(Item::Line {
            points: vec![start, stem],
            stroke: Stroke::new(colour, parts::LINE),
            dash: parts::DASH,
        });
        scene.add(Item::Polygon {
            points: parts::arrow_head(tip, Point::new(way, 0.0), parts::HEAD),
            fill: Some(Paint::solid(colour)),
            stroke: None,
        });
        let left = start.x.min(goal.x + way * CIRCLE).min(goal.x - CIRCLE);
        let right = start.x.max(goal.x + way * CIRCLE).max(goal.x + CIRCLE);
        taken.push(Rect::new(left, from.y - 8.0, right - left, 16.0));
    }
}

// --- nodes ------------------------------------------------------------------------------------

/// Draw each node's shapes and its name.
fn draw_nodes(scene: &mut Scene, ctx: &Ctx, name_rects: &[Option<Rect>]) {
    let theme = &ctx.options.theme;
    for (index, node) in ctx.diagram.nodes.iter().enumerate() {
        let centre = ctx.points[index];
        if !node.anchor {
            draw_strategy(scene, node.strategy, centre, ctx.options);
            scene.add(Item::Circle {
                centre,
                radius: CIRCLE,
                fill: Some(theme.node_fill),
                stroke: Some(Stroke::new(theme.node_stroke, parts::LINE)),
            });
            if node.inertia {
                let x = centre.x + reach(node) + 5.0;
                scene.add(Item::Line {
                    points: vec![Point::new(x, centre.y - 9.0), Point::new(x, centre.y + 9.0)],
                    stroke: Stroke::new(theme.text, 4.0),
                    dash: Dash::Solid,
                });
            }
        }
        if let Some(rect) = name_rects[index] {
            let style = parts::text_style(
                ctx.options,
                if node.anchor { 1.0 } else { 0.9 },
                node.anchor,
                theme.text,
            );
            parts::label_at(
                scene,
                &ctx.names[index],
                Point::new(rect.x, rect.y),
                &style,
                Anchor::Start,
            );
        }
    }
}

/// The shape behind a circle that says how the component is sourced.
fn draw_strategy(scene: &mut Scene, strategy: Option<Strategy>, c: Point, options: &Options) {
    let Some(strategy) = strategy else {
        return;
    };
    let colour = options.theme.accent;
    let fill = Some(Paint::faded(colour, 70));
    let stroke = Some(Stroke::new(colour, parts::LINE));
    let m = MARKER;
    match strategy {
        Strategy::Build => scene.add(Item::Polygon {
            points: vec![
                Point::new(c.x, c.y - m),
                Point::new(c.x + m, c.y + m * 0.8),
                Point::new(c.x - m, c.y + m * 0.8),
            ],
            fill,
            stroke,
        }),
        Strategy::Buy => scene.add(Item::Polygon {
            points: vec![
                Point::new(c.x, c.y - m),
                Point::new(c.x + m, c.y),
                Point::new(c.x, c.y + m),
                Point::new(c.x - m, c.y),
            ],
            fill,
            stroke,
        }),
        Strategy::Outsource => scene.add(Item::Rect {
            rect: Rect::around(c, Size::new(m * 1.6, m * 1.6)),
            radius: 0.0,
            fill,
            stroke,
        }),
        Strategy::Market => {
            scene.add(Item::Circle { centre: c, radius: m - 1.0, fill: None, stroke })
        }
    }
}

// --- annotations ------------------------------------------------------------------------------

/// The numbered circles.
fn draw_annotations(scene: &mut Scene, ctx: &Ctx) {
    let theme = &ctx.options.theme;
    let style = ctx.options.style(0.8, true);
    let drawn = parts::text_style(ctx.options, 0.8, true, theme.text);
    for annotation in &ctx.diagram.annotations {
        let at = place(&ctx.plot, annotation.vis, annotation.evo);
        scene.add(Item::Circle {
            centre: at,
            radius: BADGE,
            fill: Some(theme.node_fill),
            stroke: Some(Stroke::new(theme.accent, parts::LINE)),
        });
        let words = annotation.number.to_string();
        let label = text::measure_unwrapped(&words, &style, ctx.options.metrics);
        parts::label_at(
            scene,
            &label,
            Point::new(at.x, at.y - label.height / 2.0),
            &drawn,
            Anchor::Middle,
        );
    }
}

/// The box that explains the numbers.
struct KeyBox {
    /// One line of words, wrapped, for each annotation.
    entries: Vec<Label>,
    rect: Option<Rect>,
    /// True when the author placed it, false when it goes under the plot.
    placed: bool,
}

impl KeyBox {
    fn new(ctx: &Ctx, under: f32) -> Self {
        let diagram = ctx.diagram;
        if diagram.annotations.is_empty() {
            return Self { entries: Vec::new(), rect: None, placed: false };
        }
        let placed = diagram.key_at.is_some();
        let wrap =
            if placed { KEY_WIDTH.min(ctx.plot.width - 40.0) } else { ctx.plot.width - 24.0 };
        let style = ctx.options.style(0.85, false);
        let entries: Vec<Label> = diagram
            .annotations
            .iter()
            .map(|a| {
                text::measure(
                    &format!("{}. {}", a.number, a.text),
                    &style,
                    ctx.options.metrics,
                    wrap.max(60.0),
                )
            })
            .collect();
        let width = entries.iter().map(|e| e.width).fold(0.0_f32, f32::max) + 16.0;
        let height = entries.iter().map(|e| e.height).sum::<f32>() + 16.0;
        let rect = match diagram.key_at {
            Some((vis, evo)) => {
                let at = place(&ctx.plot, vis, evo);
                keep_inside(Rect::new(at.x, at.y, width, height), &ctx.plot)
            }
            None => Rect::new(ctx.plot.x, under, width, height),
        };
        Self { entries, rect: Some(rect), placed }
    }

    /// The rectangle, when it sits on the plot and other things must keep off it.
    fn placed_rect(&self) -> Option<Rect> {
        self.rect.filter(|_| self.placed)
    }

    /// How far down the diagram reaches: the key when it is under the plot, else `below`.
    fn bottom(&self, below: f32) -> f32 {
        match self.rect {
            Some(rect) if !self.placed => rect.bottom().max(below),
            _ => below,
        }
    }

    fn draw(&self, scene: &mut Scene, ctx: &Ctx) {
        let Some(rect) = self.rect else {
            return;
        };
        let theme = &ctx.options.theme;
        scene.add(Item::Rect {
            rect,
            radius: parts::CORNER,
            fill: Some(theme.group_fill),
            stroke: Some(Stroke::new(theme.group_stroke, 1.0)),
        });
        let style = parts::text_style(ctx.options, 0.85, false, theme.text);
        let mut y = rect.y + 8.0;
        for entry in &self.entries {
            parts::label_at(scene, entry, Point::new(rect.x + 8.0, y), &style, Anchor::Start);
            y += entry.height;
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

    const TEA: &str = "wardley-beta
title Tea Shop Value Chain

anchor Business [0.95, 0.63]
component Cup of Tea [0.79, 0.61]
component Tea [0.63, 0.81]
component Hot Water [0.52, 0.80]
component Kettle [0.43, 0.35]
component Power [0.10, 0.70]

Business -> Cup of Tea
Cup of Tea -> Tea
Cup of Tea -> Hot Water
Hot Water -> Kettle
Kettle -> Power

evolve Kettle 0.62
evolve Power 0.89

note \"Standardising power allows Kettles to evolve faster\" [0.30, 0.49]
";

    const COMPLETE: &str = "wardley-beta
title Software Platform Strategy
size [1100, 800]

evolution Genesis@0.25 -> Custom@0.5 -> Product@0.75 -> Commodity@1.0

anchor Customer [0.90, 0.95]

component \"Mobile App\" [0.80, 0.85] (build)
component \"Web App\" [0.75, 0.80] label [-60, 10] (build)
component \"API Gateway\" [0.70, 0.65] (buy)
component \"Auth Service\" [0.60, 0.55] (outsource)
component \"Database\" [0.50, 0.45] (buy) (inertia)
component \"Cloud Platform\" [0.30, 0.95] (market)

Customer -> \"Mobile App\"
Customer -> \"Web App\"
\"Mobile App\" -> \"API Gateway\"
\"Web App\" -> \"API Gateway\"
\"API Gateway\" -> \"Auth Service\"
\"API Gateway\" -> \"Database\"
\"Database\" -> \"Cloud Platform\"

evolve \"API Gateway\" 0.85
evolve \"Database\" 0.75

accelerator \"Cloud Native\" [0.20, 0.85]
deaccelerator \"Legacy Data\" [0.45, 0.35]

annotations [0.10, 0.20]
annotation 1,[0.78, 0.82] \"User touchpoints\"
annotation 2,[0.70, 0.60] \"Integration layer\"
annotation 3,[0.50, 0.40] \"Data persistence\"

note \"Build mobile-first experience\" [0.85, 0.90]
note \"Migrate to cloud-native database\" [0.60, 0.50]
";

    const LINKS: &str = "wardley-beta
component User [0.90, 0.95]
component App [0.75, 0.75]
component API [0.60, 0.60]
component Cache [0.65, 0.45]
component Database [0.15, 0.80]

User -> App
App +> API
API -.-> Cache
API +<> Cache
App +< Database
Cache +'backup'> Database
API --> Database; reads
";

    /// The rectangle each text item covers, with the fixed stub's ten points a character.
    fn text_rects(scene: &Scene) -> Vec<(String, Rect)> {
        scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Text { at, text, anchor, .. } => {
                    let width = text.chars().count() as f32 * 10.0;
                    let left = match anchor {
                        Anchor::Start => at.x,
                        Anchor::Middle => at.x - width / 2.0,
                        Anchor::End => at.x - width,
                    };
                    Some((text.clone(), Rect::new(left, at.y, width, 14.0)))
                }
                _ => None,
            })
            .collect()
    }

    fn assert_no_text_overlaps(scene: &Scene) {
        let rects = text_rects(scene);
        for first in 0..rects.len() {
            for second in first + 1..rects.len() {
                let (a, b) = (&rects[first], &rects[second]);
                assert!(!a.1.overlaps(&b.1), "{:?} overlaps {:?}: {:?} {:?}", a.0, b.0, a.1, b.1);
            }
        }
    }

    #[test]
    fn a_component_is_read_with_its_label_offset_and_decorators() {
        let d = diagram(
            "wardley-beta\n component \"Web App\" [0.75, 0.80] label [-60, 10] (build) (inertia)\n",
        );
        let node = &d.nodes[0];
        assert_eq!(node.name, "Web App");
        assert_eq!((node.vis, node.evo), (0.75, 0.80));
        assert_eq!(node.offset, Some((-60.0, 10.0)));
        assert_eq!(node.strategy, Some(Strategy::Build));
        assert!(node.inertia);
    }

    #[test]
    fn names_may_hold_spaces_and_hyphens() {
        let d = diagram(
            "wardley-beta\n component real-time processing [0.55, 0.40]\n component end-user [0.9, 0.95]\n end-user -> real-time processing\n",
        );
        assert_eq!(d.nodes[0].name, "real-time processing");
        assert_eq!(d.links[0].from, "end-user");
        assert_eq!(d.links[0].to, "real-time processing");
    }

    #[test]
    fn every_form_of_link_is_read() {
        let d = diagram(LINKS);
        let forms: Vec<(bool, Flow, Option<&str>)> =
            d.links.iter().map(|l| (l.dashed, l.flow, l.label.as_deref())).collect();
        assert_eq!(
            forms,
            vec![
                (false, Flow::None, None),
                (false, Flow::Forward, None),
                (true, Flow::None, None),
                (false, Flow::Both, None),
                (false, Flow::Backward, None),
                (false, Flow::Forward, Some("backup")),
                (false, Flow::None, Some("reads")),
            ]
        );
    }

    #[test]
    fn a_trailing_comment_is_ignored() {
        let d = diagram("wardley-beta\n component Infra [0.30, 0.20]        %% Low visibility\n");
        assert_eq!(d.nodes[0].name, "Infra");
    }

    #[test]
    fn stages_read_widths_and_dual_labels() {
        let d = diagram(
            "wardley-beta\n evolution Genesis / Concept@0.2 -> Custom -> Product@0.75 -> Commodity\n",
        );
        assert_eq!(d.stages[0].labels, vec!["Genesis", "Concept"]);
        assert_eq!(d.stages[0].end, 0.2);
        assert!((d.stages[1].end - 0.4667).abs() < 0.001, "{:?}", d.stages[1]);
        assert_eq!(d.stages[2].end, 0.75);
        assert_eq!(d.stages[3].end, 1.0);
    }

    #[test]
    fn a_pipeline_reads_its_members_under_a_parent() {
        let d = diagram(
            "wardley-beta\n component Database [0.40, 0.60]\n pipeline Database {\n  component \"File System\" [0.25]\n  component \"SQL DB\" [0.50]\n }\n",
        );
        assert_eq!(d.nodes.len(), 3);
        assert_eq!(d.nodes[1].parent_at, Some(0));
        assert_eq!(d.nodes[2].evo, 0.50);
    }

    #[test]
    fn notes_forces_and_annotations_are_read() {
        let d = diagram(COMPLETE);
        assert_eq!(d.notes.len(), 2);
        assert_eq!(d.forces.len(), 2);
        assert!(d.forces[0].accelerates && !d.forces[1].accelerates);
        assert_eq!(d.annotations[2].number, 3);
        assert_eq!(d.key_at, Some((0.10, 0.20)));
        assert_eq!(d.size, Some((1100.0, 800.0)));
    }

    #[test]
    fn bad_lines_are_refused_with_their_line_number() {
        let o = options();
        let cases = [
            ("wardley-beta\n component Tea [0.5]\n", 2, "two numbers"),
            ("wardley-beta\n component Tea [0.5, 0.5] (loud)\n", 2, "decorator"),
            ("wardley-beta\n component A [0.5, 0.5]\n A -> B\n", 3, "not a component"),
            ("wardley-beta\n evolve Ghost 0.5\n", 2, "not a component"),
            ("wardley-beta\n note Hello [0.5, 0.5]\n", 2, "quotes"),
            ("wardley-beta\n size [wide]\n", 2, "size"),
            ("wardley-beta\n evolution Only\n", 2, "two and eight"),
            ("wardley-beta\n something odd\n", 2, "not a Wardley statement"),
            ("wardley-beta\n annotation x,[0.1, 0.1] \"a\"\n", 2, "annotation"),
        ];
        for (text, line, reason) in cases {
            let problem = check::refused(text, &o);
            assert_eq!(problem.line, Some(line), "{text}");
            assert!(problem.reason.contains(reason), "{text}: {}", problem.reason);
        }
    }

    #[test]
    fn the_tea_shop_is_drawn_with_no_names_over_each_other() {
        let scene = check::drawn(
            TEA,
            &options(),
            &[
                "Tea Shop Value Chain",
                "Business",
                "Cup of Tea",
                "Kettle",
                "Power",
                "Genesis",
                "Visible",
            ],
        );
        assert_no_text_overlaps(&scene);
    }

    #[test]
    fn the_complete_example_is_drawn_inside_its_scene() {
        let scene = check::drawn(
            COMPLETE,
            &options(),
            &["Software Platform Strategy", "Mobile App", "Cloud Native", "1. User touchpoints"],
        );
        assert_no_text_overlaps(&scene);
        // A larger `size` makes a taller map.
        let tea = check::drawn(TEA, &options(), &["Kettle"]);
        assert!(scene.size.height > tea.size.height);
    }

    #[test]
    fn every_link_form_is_drawn() {
        let scene = check::drawn(LINKS, &options(), &["User", "backup", "reads"]);
        assert_no_text_overlaps(&scene);
    }

    #[test]
    fn a_pipeline_is_drawn_in_a_frame_under_its_parent() {
        let scene = check::drawn(
            "wardley-beta\n title Pipeline Evolution\n component Database [0.40, 0.60]\n pipeline Database {\n  component \"File System\" [0.25]\n  component \"SQL DB\" [0.50]\n  component \"NoSQL\" [0.70]\n  component \"Cloud DB\" [0.85]\n }\n",
            &options(),
            &["File System", "Cloud DB", "Database"],
        );
        assert_no_text_overlaps(&scene);
        let frames: Vec<Rect> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Rect { rect, radius, .. } if *radius > 0.0 => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(frames.len(), 1, "one frame for the pipeline");
        for (name, rect) in text_rects(&scene) {
            if ["File System", "SQL DB", "NoSQL", "Cloud DB"].contains(&name.as_str()) {
                assert!(
                    frames[0].contains(Point::new(rect.left(), rect.top()))
                        && frames[0].contains(Point::new(rect.right(), rect.bottom())),
                    "{name} sits inside the pipeline frame"
                );
            }
        }
    }

    #[test]
    fn crowded_names_move_to_another_side_of_their_circle() {
        let text = "wardley-beta\n component Alpha service [0.50, 0.50]\n component Beta service [0.50, 0.52]\n component Gamma service [0.50, 0.54]\n component Delta service [0.48, 0.51]\n";
        let scene = check::drawn(text, &options(), &["Alpha service", "Delta service"]);
        assert_no_text_overlaps(&scene);
    }

    #[test]
    fn stage_labels_sit_under_the_plot_in_their_columns() {
        let scene = check::drawn(
            "wardley-beta\n evolution Genesis@0.2 -> Custom@0.4 -> Product@0.75 -> Commodity@1.0\n component A [0.5, 0.5]\n",
            &options(),
            &["Genesis", "Commodity"],
        );
        let rects = text_rects(&scene);
        let find = |name: &str| rects.iter().find(|(text, _)| text == name).expect(name).1;
        assert!(find("Genesis").centre().x < find("Custom").centre().x);
        assert!(find("Custom").centre().x < find("Commodity").centre().x);
    }

    #[test]
    fn a_source_cut_off_anywhere_never_panics() {
        let o = options();
        for whole in [COMPLETE, LINKS] {
            for end in 0..=whole.len() {
                if let Some(part) = whole.get(..end) {
                    let _ = super::super::render(part, &o);
                }
            }
        }
    }

    #[test]
    fn an_empty_map_still_draws_its_frame() {
        check::drawn("wardley-beta\n", &options(), &["Visible", "Genesis"]);
    }
}
