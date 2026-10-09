//! How long laying a whole file out takes in this crate alone, with a measurer that does no work.
//!
//! `cargo run --release -p unluminous-core --example layout_cost -- <file>`
//!
//! `frame_cost` in `unluminous-app` measures layout with the machine's real fonts, which is the number a
//! person feels. This one measures the same layout with [`FixedMetrics`], so the difference between the two
//! is what the fonts cost and this number is what the layout's own bookkeeping costs. `task-2218` used the
//! pair to tell which half to work on. It is not a test and nothing fails it.

use std::time::Instant;
use unluminous_core::{layout, CharStyle, FixedMetrics, ParagraphStyles, Rope, StyleSpans};

fn main() {
    let path = std::env::args().nth(1).expect("give a file to lay out");
    let text = std::fs::read_to_string(&path).expect("read the file");
    let rope = Rope::from_str(&text);
    let chars = StyleSpans::new(rope.len_bytes(), CharStyle::default());
    let paragraphs = ParagraphStyles::new(rope.len_lines());
    let metrics = FixedMetrics::default();
    let runs = 20;
    let started = Instant::now();
    let mut lines = 0;
    for _ in 0..runs {
        lines = layout(&rope, &chars, &paragraphs, &metrics, 900.0).lines.len();
    }
    let each = started.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
    println!("{path}: {} bytes, {lines} lines, {each:.2} ms a layout", text.len());
}
