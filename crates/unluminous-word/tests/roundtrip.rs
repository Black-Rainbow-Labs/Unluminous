//! A document Word wrote, opened and saved by Unluminous with no change, keeps everything.
//!
//! Every part but the main document comes back byte for byte, and the main document comes back as the
//! same tree of elements, compared after dropping the two things Word itself regenerates on opening:
//! `w:proofErr`, where the spelling checker stopped, and `w:lastRenderedPageBreak`.

use std::path::{Path, PathBuf};

use unluminous_word::xml::{self, El, Node};

pub fn fixtures() -> Vec<PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&folder)
        .expect("the fixtures folder")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|e| e == "docx"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures in {}", folder.display());
    files
}

/// The tree with the regenerated elements taken out and every run's text joined, so two trees that
/// differ only in how a run's text was split into `w:t` elements compare equal.
fn canonical(el: &El) -> El {
    let mut out = El { children: Vec::new(), ..el.clone() };
    for node in &el.children {
        match node {
            Node::El(child)
                if matches!(child.name.as_str(), "w:proofErr" | "w:lastRenderedPageBreak") => {}
            Node::El(child) => {
                let child = canonical(child);
                // Two `w:t` in a row are one stretch of text.
                if let (Some(Node::El(previous)), "w:t") =
                    (out.children.last_mut(), child.name.as_str())
                {
                    if previous.name == "w:t" {
                        let joined = format!("{}{}", previous.text(), child.text());
                        previous.children = vec![Node::Text(joined)];
                        continue;
                    }
                }
                out.children.push(Node::El(child));
            }
            Node::Text(text) => out.children.push(Node::Text(text.clone())),
        }
    }
    // An empty run (only properties) carries nothing, and Unluminous does not write one back.
    out.children.retain(|node| match node {
        Node::El(child) if child.name == "w:r" => child.elements().any(|c| c.name != "w:rPr"),
        _ => true,
    });
    // `xml:space` is written exactly where it is needed, which is not always where another program put it.
    out.attrs.retain(|(key, _)| key != "xml:space");
    out
}

/// Where two trees first differ, for the failure message.
fn first_difference(a: &El, b: &El, path: &str) -> Option<String> {
    let here = format!("{path}/{}", a.name);
    if a.name != b.name || a.attrs != b.attrs {
        return Some(format!(
            "{here}: {:?} {:?} against {:?} {:?}",
            a.name, a.attrs, b.name, b.attrs
        ));
    }
    if a.children.len() != b.children.len() {
        let names = |el: &El| {
            el.children
                .iter()
                .map(|n| match n {
                    Node::El(e) => e.name.clone(),
                    Node::Text(t) => format!("{t:?}"),
                })
                .collect::<Vec<_>>()
        };
        return Some(format!("{here}: children {:?} against {:?}", names(a), names(b)));
    }
    for (x, y) in a.children.iter().zip(&b.children) {
        match (x, y) {
            (Node::El(x), Node::El(y)) => {
                if let Some(found) = first_difference(x, y, &here) {
                    return Some(found);
                }
            }
            (Node::Text(x), Node::Text(y)) if x == y => {}
            _ => return Some(format!("{here}: {x:?} against {y:?}")),
        }
    }
    None
}

#[test]
fn every_fixture_saved_unchanged_keeps_every_part_and_element() {
    for file in fixtures() {
        let bytes = std::fs::read(&file).unwrap();
        let document = unluminous_word::read::open(&bytes)
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let saved = unluminous_word::write::save(&document).unwrap();
        let before = unluminous_word::zip::read(&bytes).unwrap();
        let after = unluminous_word::zip::read(&saved).unwrap();
        let names = |entries: &[unluminous_word::zip::Entry]| {
            entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
        };
        assert_eq!(names(&before), names(&after), "{}: the parts", file.display());
        for (a, b) in before.iter().zip(&after) {
            if a.name == document.main {
                let a_tree = canonical(&xml::parse_bytes(&a.data).unwrap());
                let b_tree = canonical(&xml::parse_bytes(&b.data).unwrap());
                if let Some(difference) = first_difference(&a_tree, &b_tree, "") {
                    panic!("{}: the main part differs at {difference}", file.display());
                }
            } else {
                assert!(a.data == b.data, "{}: {} changed", file.display(), a.name);
            }
        }
    }
}

#[test]
fn a_saved_fixture_reads_back_to_the_same_text() {
    for file in fixtures() {
        let document = unluminous_word::read::open(&std::fs::read(&file).unwrap()).unwrap();
        let again =
            unluminous_word::read::open(&unluminous_word::write::save(&document).unwrap()).unwrap();
        let text = |d: &unluminous_word::Document| -> Vec<String> {
            unluminous_word::position::paragraphs(&d.body)
                .into_iter()
                .map(|(_, p)| p.flat_text())
                .collect()
        };
        assert_eq!(text(&document), text(&again), "{}", file.display());
    }
}
