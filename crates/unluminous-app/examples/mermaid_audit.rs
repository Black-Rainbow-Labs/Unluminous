//! Lay out every Mermaid block in a folder of Markdown files and report what would look wrong.
//!
//! `cargo run --example mermaid_audit -- <folder>`
//!
//! `task-2063` asked for the Mermaid drawn in a Markdown preview to be looked at and improved: `<br/>`
//! shown as text, parts drawn over each other, bad formatting. Looking is the real test and this does
//! not replace it; it is how the faults were counted across several hundred diagrams before and after,
//! which a person paging through screenshots cannot do. Three things are reported for each block:
//!
//! - a block the reader refused, with its reason;
//! - text that still holds markup, a `<tag>` or an entity, which a reader should have turned into what
//!   it means;
//! - two pieces of text drawn over each other. Text is measured through the fixed width stub the layout
//!   tests use, ten points a character and twenty a line, so what counts as an overlap is measured the
//!   same way on every machine.
//!
//! `task-2194` added two more, because they are the other two things the ticket names:
//!
//! - text that runs outside the box it is drawn in, found by asking which filled box holds the middle
//!   of the words and whether the words fit inside it;
//! - a line that runs through a box that is not at either end of it, counted separately for the boxes
//!   that are things and the small panels that carry another line's label. Only in the five diagram
//!   types the layered layout draws, because a sequence diagram's lines cross its activation bars on
//!   purpose.
//!
//! The folder is searched to any depth, and several can be given separated by `;`.

use unluminous_core::mermaid::scene::{Point, Rect};
use unluminous_core::mermaid::{self, Anchor, Item, Options};
use unluminous_core::metrics::FixedMetrics;

fn main() {
    let folders = std::env::args().nth(1).expect("a folder of Markdown files");
    let metrics = FixedMetrics::default();
    let options = Options::new(&metrics);
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for folder in folders.split(';') {
        files.extend(markdown_files(std::path::Path::new(folder)));
    }
    files.sort();
    let (mut blocks, mut refused, mut markup, mut overlapping) = (0, 0, 0, 0);
    let (mut escaping, mut through, mut through_labels) = (0, 0, 0);
    for path in files {
        let text = std::fs::read_to_string(&path).expect("the file");
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        for (index, source) in fences(&text).into_iter().enumerate() {
            blocks += 1;
            let scene = match mermaid::render(&source, &options) {
                Ok(scene) => scene,
                Err(problem) => {
                    refused += 1;
                    println!("{name} #{index}: REFUSED {}", problem.message());
                    continue;
                }
            };
            let texts: Vec<(f32, f32, f32, f32, String)> = scene
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
                        Some((left, at.y, width, 20.0, text.clone()))
                    }
                    _ => None,
                })
                .collect();
            for (_, _, _, _, words) in &texts {
                if has_markup(words) {
                    markup += 1;
                    println!("{name} #{index}: MARKUP {words:?}");
                }
            }
            let boxes: Vec<(Rect, bool)> = scene
                .items
                .iter()
                .filter_map(|item| match item {
                    // A label's panel has a corner of three points, which nothing else does.
                    Item::Rect { rect, fill: Some(fill), radius, .. } if fill.alpha == 255 => {
                        Some((*rect, (*radius - 3.0).abs() < 0.01))
                    }
                    _ => None,
                })
                .collect();
            let layered = ["flowchart", "graph", "stateDiagram", "classDiagram", "erDiagram"]
                .iter()
                .any(|kind| source.trim_start().starts_with(kind));
            for (left, top, width, height, words) in &texts {
                let middle = Point::new(left + width / 2.0, top + height / 2.0);
                let holder = boxes
                    .iter()
                    .map(|(rect, _)| rect)
                    .filter(|rect| rect.contains(middle))
                    .min_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)));
                if let Some(rect) = holder {
                    // Across only: the stub's twenty point line is taller than a small label's real
                    // one, so a short tag would be reported for a height it does not have.
                    let out = rect.x - left > 1.0 || (left + width) - rect.right() > 1.0;
                    if out {
                        escaping += 1;
                        println!("{name} #{index}: ESCAPES {words:?} from {rect:?}");
                    }
                }
            }
            for item in scene.items.iter().filter(|_| layered) {
                let Item::Line { points, .. } = item else { continue };
                let (Some(first), Some(last)) = (points.first(), points.last()) else { continue };
                for (rect, label) in &boxes {
                    let near = |at: &Point| rect.grown(3.0).contains(*at);
                    if near(first) || near(last) {
                        continue;
                    }
                    let inner =
                        Rect::new(rect.x + 2.0, rect.y + 2.0, rect.width - 4.0, rect.height - 4.0);
                    let crosses = points.windows(2).any(|pair| {
                        (0..=20).any(|step| {
                            inner.contains(pair[0].towards(pair[1], step as f32 / 20.0))
                        })
                    });
                    // A line passes through the middle of its own label's panel, which is where the
                    // layout puts the label, so that one is not a fault.
                    let own = points.windows(2).any(|pair| {
                        (0..=40).any(|step| {
                            pair[0].towards(pair[1], step as f32 / 40.0).distance(rect.centre())
                                < 2.0
                        })
                    });
                    if crosses && *label && !own {
                        through_labels += 1;
                        println!("{name} #{index}: THROUGH a label at {rect:?}");
                    } else if crosses && !*label {
                        through += 1;
                        println!("{name} #{index}: THROUGH a box at {rect:?}");
                    }
                }
            }
            for (first, a) in texts.iter().enumerate() {
                for b in &texts[first + 1..] {
                    let across = (a.0 + a.2).min(b.0 + b.2) - a.0.max(b.0);
                    let down = (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1);
                    if across > 4.0 && down > 4.0 {
                        overlapping += 1;
                        println!("{name} #{index}: OVERLAP {:?} and {:?}", a.4, b.4);
                    }
                }
            }
        }
    }
    println!(
        "\n{blocks} blocks: {refused} refused, {markup} with markup left in, {overlapping} overlapping pairs of text, {escaping} pieces of text outside their box, {through} lines through a box, {through_labels} lines behind another line's label"
    );
}

/// Every `.md` file under `folder`, to any depth.
fn markdown_files(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(markdown_files(&path));
        } else if path.extension().is_some_and(|extension| extension == "md") {
            found.push(path);
        }
    }
    found
}

/// The body of every ```mermaid fence in a Markdown file.
fn fences(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut inside: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        match inside.as_mut() {
            None if trimmed.starts_with("```mermaid") => inside = Some(String::new()),
            Some(_) if trimmed.starts_with("```") => found.extend(inside.take()),
            Some(body) => {
                body.push_str(line);
                body.push('\n');
            }
            None => {}
        }
    }
    found
}

/// Whether text still holds a tag or an entity that should have become what it means.
fn has_markup(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let tag = lower.char_indices().any(|(at, c)| {
        c == '<'
            && lower[at + 1..]
                .chars()
                .next()
                .is_some_and(|next| next.is_ascii_alphabetic() || next == '/')
            && lower[at..].contains('>')
    });
    tag || ["&lt;", "&gt;", "&amp;", "&quot;", "&nbsp;", "#quot;", "#lt;", "#gt;"]
        .iter()
        .any(|entity| lower.contains(entity))
}
