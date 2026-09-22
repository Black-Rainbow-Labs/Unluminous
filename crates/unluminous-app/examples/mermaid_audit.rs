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

use unluminous_core::mermaid::{self, Anchor, Item, Options};
use unluminous_core::metrics::FixedMetrics;

fn main() {
    let folder = std::env::args().nth(1).expect("a folder of Markdown files");
    let metrics = FixedMetrics::default();
    let options = Options::new(&metrics);
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&folder)
        .expect("the folder")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    files.sort();
    let (mut blocks, mut refused, mut markup, mut overlapping) = (0, 0, 0, 0);
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
        "\n{blocks} blocks: {refused} refused, {markup} with markup left in, {overlapping} overlapping pairs of text"
    );
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
