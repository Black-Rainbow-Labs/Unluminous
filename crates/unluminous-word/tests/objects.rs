//! Text boxes, shapes and pictures laid out on the page (task-2359, TDD 5.3), against fixtures Word made.

use unluminous_core::FixedMetrics;
use unluminous_word::layout::{self, Item, Options};

fn fixture(name: &str) -> unluminous_word::model::Document {
    let path = format!("{}/tests/fixtures/{name}.docx", env!("CARGO_MANIFEST_DIR"));
    unluminous_word::read::open(&std::fs::read(path).expect("the fixture is there")).expect("it opens")
}

/// The words drawn inside a rectangle of the first page, in reading order.
fn words_in(page: &layout::Page, left: f32, right: f32, top: f32) -> String {
    let mut lines: Vec<(f32, f32, String)> = page
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Text(text) if text.xs.first().is_some_and(|x| *x >= left && *x < right) && text.baseline > top => {
                Some((text.baseline, text.xs[0], text.text.clone()))
            }
            _ => None,
        })
        .collect();
    lines.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    lines.into_iter().map(|l| l.2).collect::<Vec<_>>().join(" ")
}

#[test]
fn linked_text_boxes_share_one_story() {
    let document = fixture("linked-textboxes");
    let pages = layout::lay_out(&document, &FixedMetrics::default(), Options::default());
    let page = &pages.pages[0];
    // The body text starts at 72 and runs on behind the boxes; a box's text starts inside its inset.
    let first = words_in(page, 76.0, 240.0, 200.0);
    let second = words_in(page, 324.0, 490.0, 200.0);
    let story: String = unluminous_word::position::paragraphs(&document.textboxes[0].blocks)
        .iter()
        .map(|(_, p)| p.flat_text())
        .collect::<Vec<_>>()
        .join(" ");
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(!first.is_empty() && !second.is_empty(), "both boxes hold text: {first:?} / {second:?}");
    assert!(
        squash(&story).starts_with(&squash(&format!("{first} {second}"))),
        "the second box goes on where the first stops: {first:?} then {second:?}"
    );
    // Each box keeps only the lines that fit in it, and what the last box has no room for is not drawn.
    let lowest = |left: f32, right: f32| {
        page.items
            .iter()
            .filter_map(|i| match i {
                Item::Text(t) if t.xs.first().is_some_and(|x| *x >= left && *x < right) && t.baseline > 200.0 => Some(t.baseline),
                _ => None,
            })
            .fold(0.0f32, f32::max)
    };
    assert!(lowest(76.0, 240.0) <= 270.0, "nothing of the first box is drawn below it");
    assert!(lowest(324.0, 490.0) <= 320.0, "nothing of the second box is drawn below it");
}

#[test]
fn every_object_has_its_place_for_the_pointer() {
    let document = fixture("wrapping");
    let pages = layout::lay_out(&document, &FixedMetrics::default(), Options::default());
    let objects = pages.pages.iter().flat_map(|p| &p.items).filter(|i| matches!(i, Item::Object { .. })).count();
    assert!(objects >= 5, "every picture and the text box is an object a press selects, found {objects}");
}
