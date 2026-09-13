//! Accounts for the retained heap-owned pieces of a full document layout.
//!
//! `cargo run --release -p unluminous-app --example layout_memory -- <file> [width]`

use std::mem::size_of;

use unluminous_app::services::text_renderer::TextRenderer;
use unluminous_core::{layout, Document, PlacedCluster, PlacedLine, PlacedRun};

/// Loads and colours one file exactly as the editor does before measuring its layout containers.
fn main() {
    let mut arguments = std::env::args().skip(1);
    let path = arguments.next().expect("a file path");
    let width = arguments.next().and_then(|value| value.parse().ok()).unwrap_or(900.0);
    let source = std::fs::read_to_string(&path).expect("read source");
    let renderer = TextRenderer::new();
    let mut document = Document::from_text(&source);
    colour_like_the_editor(&path, &source, &mut document);
    let mut laid =
        layout(document.text(), document.chars(), document.paragraphs(), &renderer, width);
    report_layout("working", &laid, source.len());
    laid.compact_capacity();
    report_layout("cached", &laid, source.len());
}

/// Applies the source file's plugin grammar and colours so the run shape matches the real editor.
fn colour_like_the_editor(path: &str, source: &str, document: &mut Document) {
    let (plugins, _) = unluminous_app::services::plugins::Plugins::load(None);
    let Some(plugin) = plugins.for_path(std::path::Path::new(path)) else { return };
    let base = unluminous_core::Color::rgb(0xF2, 0xF2, 0xF2);
    let spans: Vec<_> = unluminous_core::syntax::highlight(source, &plugin.grammar)
        .into_iter()
        .filter_map(|(range, token)| plugin.theme.colour(token).map(|colour| (range, colour)))
        .collect();
    document.set_syntax(base, &spans);
}

/// Prints aggregate lengths, capacities, element sizes, and accounted retained bytes without source text.
fn report_layout(state: &str, laid: &unluminous_core::Layout, source_bytes: usize) {
    let runs = laid.lines.iter().flat_map(|line| &line.runs);
    let run_count = runs.clone().count();
    let run_capacity: usize = laid.lines.iter().map(|line| line.runs.capacity()).sum();
    let cluster_count: usize = laid.lines.iter().map(|line| line.clusters().count()).sum();
    let cluster_capacity: usize = laid.lines.iter().map(|line| line.cluster_capacity()).sum();
    let accounted = laid.lines.capacity() * size_of::<PlacedLine>()
        + run_capacity * size_of::<PlacedRun>()
        + cluster_capacity * size_of::<PlacedCluster>();
    println!("state={state} source_bytes={source_bytes}");
    println!(
        "size_line={} size_run={} size_cluster={}",
        size_of::<PlacedLine>(),
        size_of::<PlacedRun>(),
        size_of::<PlacedCluster>()
    );
    println!(
        "lines={} line_capacity={} runs={} run_capacity={} clusters={} cluster_capacity={}",
        laid.lines.len(),
        laid.lines.capacity(),
        run_count,
        run_capacity,
        cluster_count,
        cluster_capacity
    );
    println!(
        "public_layout_bytes={accounted} public_layout_mb={:.2}",
        accounted as f64 / 1024.0 / 1024.0
    );
}
