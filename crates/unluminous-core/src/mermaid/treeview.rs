//! Tree views: `treeView-beta`.
//!
//! A directory listing. One row a line, each row indented by its depth, with an elbow from the line
//! running down its parent's name to the row's own name.
//!
//! The structure is read two ways, chosen line by line, so a listing pasted from a terminal works
//! as it is:
//!
//! - **by indentation**, where a line belongs under the nearest line above it that is indented less;
//! - **by box characters**, `├──`, `└──`, `│` and the heavy forms, where the depth is the column of the
//!   branch character. `Source` trims each line, so that column is the line's indent plus the number
//!   of characters before the branch in what is left, counted in characters and not bytes.
//!
//! What follows a name is read and then dropped or kept:
//!
//! - a trailing `/` makes a folder, drawn bold;
//! - `## words` is a description, drawn after the name in the dim colour and italic;
//! - `:::class` and `icon(pack:name)` are read and dropped, because a document does not choose the
//!   window's colours and the icon packs are not bundled.
//!
//! **A branch character is recognised only in the prefix of a line**, the run of rails, branches and
//! spaces before the name, so a file whose name contains `├` is not misread.

use super::parts;
use super::scene::{Anchor, Dash, Item, Point, Rect, Scene, Stroke};
use super::source::{self, Source};
use super::text;
use super::{Options, Problem};

/// How far each level of depth moves a name to the right.
const INDENT: f32 = 24.0;
/// How far from the left of a parent's name its vertical line runs.
const RAIL: f32 = 6.0;
/// Space between the end of an elbow and the name it points at.
const ELBOW_GAP: f32 = 4.0;
/// Space between a name and its description.
const DESCRIPTION_GAP: f32 = 10.0;
/// The most rows a tree may have, so a typing mistake cannot ask for a mile of listing.
const LIMIT: usize = 5000;

/// The characters that draw a branch to a name.
const BRANCHES: [char; 8] = ['├', '└', '┣', '┗', '┠', '┖', '┝', '┕'];
/// The characters that, with spaces, may stand in front of a name without being part of it.
const RAILS: [char; 12] = ['│', '┃', '─', '━', '┆', '┇', '┊', '┋', '╎', '╏', '┌', '┏'];

/// One row.
#[derive(Debug, Clone, PartialEq)]
struct Node {
    name: String,
    folder: bool,
    description: Option<String>,
    depth: usize,
    /// The row this one hangs from.
    parent: Option<usize>,
}

pub fn render(source: &Source, options: &Options) -> Result<Scene, Problem> {
    let nodes = read(source)?;
    Ok(draw(&nodes, source, options))
}

/// A line's prefix: where its branch character was, how many characters the prefix took, and the rest.
struct Split<'a> {
    branch: Option<usize>,
    prefix: usize,
    rest: &'a str,
}

/// Separate the box drawing and spaces at the front of `text` from the name after them.
fn split_prefix(text: &str) -> Split<'_> {
    let mut branch = None;
    for (offset, (at, character)) in text.char_indices().enumerate() {
        if BRANCHES.contains(&character) {
            branch.get_or_insert(offset);
        } else if !character.is_whitespace() && !RAILS.contains(&character) {
            return Split { branch, prefix: offset, rest: &text[at..] };
        }
    }
    Split { branch, prefix: text.chars().count(), rest: "" }
}

/// Read every row, and refuse a line that has a branch and no name.
fn read(source: &Source) -> Result<Vec<Node>, Problem> {
    let mut nodes: Vec<Node> = Vec::new();
    // The key of each open ancestor, with the row it belongs to.
    let mut open: Vec<(usize, usize)> = Vec::new();
    for line in source.statements() {
        let split = split_prefix(&line.text);
        if split.rest.is_empty() {
            if split.branch.is_some() {
                return Err(Problem::at(line, "a branch needs a name after it, like `├── src/`."));
            }
            // A line of nothing but rails joins two branches and holds no row.
            continue;
        }
        // A box line is one level deeper than a plain line at the same column, so that a plain
        // `project/` at column zero is the parent of a branch at column zero.
        let key = match split.branch {
            Some(offset) => line.indent + offset + 1,
            None => line.indent + split.prefix,
        };
        let (name, folder, description) = parse_label(split.rest).ok_or_else(|| {
            Problem::at(line, "a row is a name, then `## words` if it needs a description.")
        })?;
        while open.last().is_some_and(|(top, _)| *top >= key) {
            open.pop();
        }
        if nodes.len() >= LIMIT {
            return Err(Problem::at(
                line,
                format!("Unluminous draws {LIMIT} rows of a tree at most."),
            ));
        }
        let parent = open.last().map(|(_, row)| *row);
        open.push((key, nodes.len()));
        nodes.push(Node { name, folder, description, depth: open.len() - 1, parent });
    }
    Ok(nodes)
}

/// Split a row's text into its name, whether it is a folder, and its description.
///
/// `None` when nothing is left once the annotations are taken away.
fn parse_label(rest: &str) -> Option<(String, bool, Option<String>)> {
    let (name, tail) = take_name(rest);
    if name.is_empty() {
        return None;
    }
    let description = find_marker(tail, "##")
        .map(|at| tail[at + 2..].trim().to_owned())
        .filter(|words| !words.is_empty());
    let folder = name.ends_with('/');
    Some((name, folder, description))
}

/// The name at the front of `rest`, quoted or running up to the first annotation, and what follows it.
fn take_name(rest: &str) -> (String, &str) {
    let rest = rest.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = rest.strip_prefix(quote) {
            if let Some(end) = inner.find(quote) {
                let stop = quote.len_utf8() + end + quote.len_utf8();
                return (source::label(&rest[..stop]), &rest[stop..]);
            }
        }
    }
    let end = ["##", ":::", "icon("]
        .iter()
        .filter_map(|marker| find_marker(rest, marker))
        .min()
        .unwrap_or(rest.len());
    (rest[..end].trim().to_owned(), &rest[end..])
}

/// Where `marker` first appears with a space or the start of the text in front of it.
fn find_marker(text: &str, marker: &str) -> Option<usize> {
    text.match_indices(marker)
        .map(|(at, _)| at)
        .find(|at| *at == 0 || text[..*at].ends_with(char::is_whitespace))
}

fn draw(nodes: &[Node], source: &Source, options: &Options) -> Scene {
    let mut scene = Scene::new();
    let plain = options.style(1.0, false);
    let bold = options.style(1.0, true);
    let line = text::measure_unwrapped("X", &plain, options.metrics).line_height;
    let row = line + 8.0;

    let lefts: Vec<f32> =
        nodes.iter().map(|node| parts::MARGIN + INDENT * node.depth as f32).collect();
    let name_widths: Vec<f32> = nodes
        .iter()
        .map(|node| {
            text::width_of(&node.name, if node.folder { &bold } else { &plain }, options.metrics)
        })
        .collect();
    let description_widths: Vec<f32> = nodes
        .iter()
        .map(|node| {
            node.description
                .as_ref()
                .map_or(0.0, |words| text::width_of(words, &plain, options.metrics))
        })
        .collect();
    let width = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let extra = if node.description.is_some() {
                DESCRIPTION_GAP + description_widths[index]
            } else {
                0.0
            };
            lefts[index] + name_widths[index] + extra
        })
        .fold(0.0_f32, f32::max)
        + parts::MARGIN;

    let top = parts::title(&mut scene, source, options, width) + parts::MARGIN;
    let tops: Vec<f32> = (0..nodes.len()).map(|index| top + row * index as f32).collect();
    draw_elbows(&mut scene, nodes, &lefts, &tops, line, options);
    draw_names(&mut scene, nodes, &lefts, &tops, &name_widths, &description_widths, options);
    scene.claim(Rect::new(0.0, 0.0, width - parts::MARGIN, top + row * nodes.len() as f32));
    parts::finish(&mut scene);
    scene
}

/// The elbow into every row: down its parent's line from the previous sibling, then across.
fn draw_elbows(
    scene: &mut Scene,
    nodes: &[Node],
    lefts: &[f32],
    tops: &[f32],
    line: f32,
    options: &Options,
) {
    // Where the line down each parent has got to so far.
    let mut reached: Vec<f32> = tops.iter().map(|top| top + 4.0 + line + 1.0).collect();
    let stroke = Stroke::new(options.theme.dim, parts::LINE);
    for (index, node) in nodes.iter().enumerate() {
        let Some(parent) = node.parent else {
            continue;
        };
        let rail = lefts[parent] + RAIL;
        let middle = tops[index] + 4.0 + line / 2.0;
        scene.add(Item::Line {
            points: vec![
                Point::new(rail, reached[parent]),
                Point::new(rail, middle),
                Point::new(lefts[index] - ELBOW_GAP, middle),
            ],
            stroke,
            dash: Dash::Solid,
        });
        reached[parent] = middle;
    }
}

/// Every name, bold for a folder, and every description after its name.
fn draw_names(
    scene: &mut Scene,
    nodes: &[Node],
    lefts: &[f32],
    tops: &[f32],
    name_widths: &[f32],
    description_widths: &[f32],
    options: &Options,
) {
    let plain = parts::text_style(options, 1.0, false, options.theme.text);
    let bold = parts::text_style(options, 1.0, true, options.theme.text);
    let mut described = parts::text_style(options, 1.0, false, options.theme.dim);
    described.italic = true;
    for (index, node) in nodes.iter().enumerate() {
        let at = Point::new(lefts[index], tops[index] + 4.0);
        let style = if node.folder { &bold } else { &plain };
        parts::one_line(scene, &node.name, at, style, Anchor::Start, name_widths[index]);
        if let Some(words) = &node.description {
            let after = Point::new(at.x + name_widths[index] + DESCRIPTION_GAP, at.y);
            parts::one_line(
                scene,
                words,
                after,
                &described,
                Anchor::Start,
                description_widths[index],
            );
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

    fn nodes(text: &str) -> Vec<Node> {
        read(&Source::read(text).expect("a diagram")).expect("it should read")
    }

    fn shape(nodes: &[Node]) -> Vec<(&str, usize)> {
        nodes.iter().map(|node| (node.name.as_str(), node.depth)).collect()
    }

    #[test]
    fn indentation_nests_the_rows() {
        let nodes = nodes(
            "treeView-beta\n    my-project/\n        src/\n            index.js\n        package.json\n",
        );
        assert_eq!(
            shape(&nodes),
            vec![("my-project/", 0), ("src/", 1), ("index.js", 2), ("package.json", 1)]
        );
        assert!(nodes[0].folder && nodes[1].folder && !nodes[2].folder);
        assert_eq!(nodes[3].parent, Some(0));
    }

    #[test]
    fn quoted_labels_keep_their_spaces() {
        let nodes = nodes("treeView-beta\n    \"my project\"\n        \"folder with spaces\"\n");
        assert_eq!(shape(&nodes), vec![("my project", 0), ("folder with spaces", 1)]);
    }

    #[test]
    fn box_characters_give_the_depth_from_the_branch_column() {
        let nodes = nodes(
            "treeView-beta\n├── packages/\n│   ├── mermaid/\n│   │   ├── src/\n│   │   │   ├── parser.ts\n\
             │   │   │   └── renderer.ts\n│   │   └── package.json\n│   └── parser/\n│       └── src/\n└── README.md\n",
        );
        assert_eq!(
            shape(&nodes),
            vec![
                ("packages/", 0),
                ("mermaid/", 1),
                ("src/", 2),
                ("parser.ts", 3),
                ("renderer.ts", 3),
                ("package.json", 2),
                ("parser/", 1),
                ("src/", 2),
                ("README.md", 0),
            ]
        );
    }

    #[test]
    fn heavy_characters_and_a_root_above_the_branches_read_the_same() {
        let nodes = nodes("treeView-beta\nroot/\n┣━━ a\n┃   ┗━━ b\n┗━━ c\n");
        assert_eq!(shape(&nodes), vec![("root/", 0), ("a", 1), ("b", 2), ("c", 1)]);
    }

    #[test]
    fn a_line_of_rails_alone_holds_no_row() {
        let nodes = nodes("treeView-beta\n├── a\n│\n└── b\n");
        assert_eq!(shape(&nodes), vec![("a", 0), ("b", 0)]);
    }

    #[test]
    fn a_branch_with_no_name_is_refused_with_its_line() {
        let problem = check::refused("treeView-beta\n├── a\n└──\n", &options());
        assert_eq!(problem.line, Some(3));
        assert!(problem.reason.contains("name"));
    }

    #[test]
    fn annotations_are_read_dropped_or_kept() {
        let nodes = nodes(
            "treeView-beta\n├── App.tsx :::highlight icon(logos:react) ## main component\n\
             ├── index.ts ## entry point\n├── Plain.js icon(none)\n└── 🚀 launch/\n",
        );
        assert_eq!(nodes[0].name, "App.tsx");
        assert_eq!(nodes[0].description.as_deref(), Some("main component"));
        assert_eq!(nodes[1].description.as_deref(), Some("entry point"));
        assert_eq!(nodes[2].name, "Plain.js");
        assert_eq!(nodes[2].description, None);
        assert_eq!(nodes[3].name, "🚀 launch/");
        assert!(nodes[3].folder, "an emoji label can still be a folder");
    }

    #[test]
    fn a_hash_inside_a_name_is_not_a_description() {
        let nodes = nodes("treeView-beta\n    issue#42.md\n    \"a ## b\"\n");
        assert_eq!(nodes[0].name, "issue#42.md");
        assert_eq!(nodes[1].name, "a ## b");
        assert_eq!(nodes[1].description, None);
    }

    #[test]
    fn comments_are_skipped_and_a_tab_counts_as_indentation() {
        let nodes =
            nodes("treeView-beta\n    %% generated\n    src/\n\t\tgenerated/\n    index.js\n");
        assert_eq!(shape(&nodes), vec![("src/", 0), ("generated/", 1), ("index.js", 0)]);
    }

    #[test]
    fn the_documentation_examples_are_drawn() {
        let opts = options();
        let texts = [
            "treeView-beta\n    my-project/\n        src/\n            index.js\n        package.json\n        README.md\n",
            "treeView-beta\n├── src/\n│   ├── App.tsx :::highlight icon(logos:react) ## main component\n│   └── index.ts ## entry point\n├── .env ## environment variables\n├── Dockerfile\n└── package.json\n",
            "treeView-beta\n    🚀 rocket-app/\n        📦 packages/\n            🎨 ui/\n            🛠️ utils/\n        🧪 tests/\n        📝 README.md\n        ⚙️ config.yaml\n",
        ];
        for text in texts {
            let scene = check::drawn(text, &opts, &[]);
            check::no_two_rectangles_overlap(&scene.rects());
        }
        check::drawn(texts[1], &opts, &["src/", "main component", "package.json"]);
    }

    #[test]
    fn folders_are_bold_and_descriptions_are_dim_italic_after_the_name() {
        let scene = check::drawn(
            "treeView-beta\n    src/\n        index.js ## app entry point\n",
            &options(),
            &["src/", "index.js", "app entry point"],
        );
        let find = |words: &str| {
            scene.items.iter().find_map(|item| match item {
                Item::Text { text, style, at, .. } if text == words => Some((style.clone(), *at)),
                _ => None,
            })
        };
        let (folder, _) = find("src/").expect("the folder");
        assert!(folder.bold);
        let (name, name_at) = find("index.js").expect("the file");
        assert!(!name.bold);
        let (description, description_at) = find("app entry point").expect("the description");
        assert!(description.italic);
        assert_ne!(description.color, name.color);
        assert!(
            description_at.x >= name_at.x + 80.0,
            "it starts after the eight letters of the name"
        );
        assert!((description_at.y - name_at.y).abs() < 0.01);
    }

    #[test]
    fn an_elbow_joins_each_row_to_its_parents_line() {
        let scene = check::drawn(
            "treeView-beta\n    a/\n        b\n        c\n",
            &options(),
            &["a/", "b", "c"],
        );
        let elbows: Vec<&Vec<Point>> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Line { points, .. } if points.len() == 3 => Some(points),
                _ => None,
            })
            .collect();
        assert_eq!(elbows.len(), 2);
        assert!(
            (elbows[0][0].x - elbows[1][0].x).abs() < 0.01,
            "both leave the same vertical line"
        );
        assert!(
            (elbows[1][0].y - elbows[0][1].y).abs() < 0.01,
            "the second carries on from the first"
        );
    }

    #[test]
    fn no_row_overlaps_another_and_no_line_crosses_a_name() {
        let options = options();
        let scene = check::drawn(
            "treeView-beta\n├── src/\n│   ├── a.ts ## one\n│   └── b.ts\n└── c ## two\n",
            &options,
            &[],
        );
        let words: Vec<Rect> = scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Text { at, text, .. } => Some(Rect::new(
                    at.x,
                    at.y,
                    text::width_of(text, &options.style(1.0, false), options.metrics),
                    20.0,
                )),
                _ => None,
            })
            .collect();
        for first in 0..words.len() {
            for second in first + 1..words.len() {
                assert!(!words[first].overlaps(&words[second]));
            }
        }
        for item in &scene.items {
            if let Item::Line { points, .. } = item {
                for rect in &words {
                    let inner =
                        Rect::new(rect.x + 1.0, rect.y + 1.0, rect.width - 2.0, rect.height - 2.0);
                    for pair in points.windows(2) {
                        for step in 0..=40 {
                            let at = pair[0].towards(pair[1], step as f32 / 40.0);
                            assert!(!inner.contains(at), "a line crosses a name at {at:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_source_cut_off_anywhere_never_panics() {
        let text = "treeView-beta\n├── 🚀 src/ ## a note\n│   └── \"x y\" :::c icon(a:b)\n└── z\n";
        let options = options();
        for end in 0..=text.len() {
            if let Some(part) = text.get(..end) {
                let _ = super::super::render(part, &options);
            }
        }
    }
}
