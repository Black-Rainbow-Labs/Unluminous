//! The layout cache (task-2359, TDD 5.2): pages laid out with it are the pages laid out without it, on
//! every fixture Word made, and a keystroke lays out again only the paragraph it changed.

mod common;

use std::cell::RefCell;
use std::path::Path;

use unluminous_word::layout::{self, LayoutCache, Options};
use unluminous_word::position::Pos;
use unluminous_word::session::Session;

fn fixtures() -> Vec<std::path::PathBuf> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut out: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "docx"))
        .collect();
    out.sort();
    out
}

#[test]
fn a_cached_layout_is_the_layout() {
    let fonts = common::RealFonts::new();
    for path in fixtures() {
        let document = unluminous_word::read::open(&std::fs::read(&path).unwrap()).unwrap();
        let fresh = layout::lay_out(&document, &fonts, Options::default());
        let cache = RefCell::new(LayoutCache::new());
        let first = layout::lay_out_cached(&document, &fonts, Options::default(), &cache, 1);
        let second = layout::lay_out_cached(&document, &fonts, Options::default(), &cache, 1);
        assert!(first == fresh, "{} laid out with an empty cache differs", path.display());
        assert!(second == fresh, "{} laid out from the cache differs", path.display());
        assert_eq!(cache.borrow().misses, 0, "{}: the second layout took every paragraph again", path.display());
    }
}

#[test]
fn a_keystroke_lays_out_only_its_paragraph_and_is_quick() {
    let fonts = common::RealFonts::new();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/long.docx");
    let document = unluminous_word::read::open(&std::fs::read(path).unwrap()).unwrap();
    let mut session = Session::new(document);
    let cache = RefCell::new(LayoutCache::new());
    layout::lay_out_cached(&session.document, &fonts, Options::default(), &cache, 1);
    let at = Pos::new(vec![40], 3);
    session.select(at.clone(), at);
    session.type_text("x");
    let started = std::time::Instant::now();
    let pages = layout::lay_out_cached(&session.document, &fonts, Options::default(), &cache, 1);
    let took = started.elapsed();
    let (hits, misses) = (cache.borrow().hits, cache.borrow().misses);
    println!("a keystroke on long.docx: {took:?}, {hits} paragraphs from the cache, {misses} laid out");
    assert!(misses <= 2, "only the changed paragraph is laid out again: {misses}");
    assert!(pages == layout::lay_out(&session.document, &fonts, Options::default()), "and the pages are right");
    // The budget is a frame, measured in an optimised build; a debug build is several times slower.
    if !cfg!(debug_assertions) {
        assert!(took.as_millis() < 16, "a keystroke took {took:?}");
    }
}

/// A 200 page document opens in under two seconds.
#[test]
fn a_long_document_opens_quickly() {
    let fonts = common::RealFonts::new();
    let mut session = Session::new(unluminous_word::new::blank());
    let sentence = "Every paragraph of this generated document is long enough to wrap across a few lines of the page, so the layout has real work to do. ";
    let paragraph = sentence.repeat(4);
    let text: Vec<String> = (0..1400).map(|_| paragraph.clone()).collect();
    session.type_text(&text.join("\n"));
    let started = std::time::Instant::now();
    let pages = layout::lay_out(&session.document, &fonts, Options::default());
    let took = started.elapsed();
    println!("{} pages in {took:?}", pages.pages.len());
    assert!(pages.pages.len() >= 200, "{} pages", pages.pages.len());
    if !cfg!(debug_assertions) {
        assert!(took.as_secs_f32() < 2.0, "200 pages took {took:?}");
    }
}
