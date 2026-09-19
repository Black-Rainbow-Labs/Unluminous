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
    report_history(&path, &source);
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

/// What one undo step costs, which the layout numbers above say nothing about.
///
/// **`task-1984` C4.** Every edit that is not typing calls `Document::snapshot`, which clones the
/// text, the paragraphs and the whole `StyleSpans` — and this diagnostic, built by `task-1813` to say
/// what a window is holding, reported the layout containers and nothing about the history. A person
/// who has pressed Backspace two hundred times in a large file is carrying something larger than
/// anything else that ticket measured, and no number anywhere said so.
///
/// Accounted rather than sampled, which is what the layout numbers above already are: the process's
/// working set moves for reasons that have nothing to do with this, and an accounted number is the
/// same on every machine. What a snapshot holds is the text, one span per style run, and one
/// paragraph style per paragraph.
///
/// **The family is no longer counted per span**, and that is the change C4 is about:
/// `CharStyle::family` was a `String`, so a file coloured into 234,000 spans carried 234,000 heap
/// copies of the same word; it is an `Arc<str>` now and every span shares one.
fn report_history(path: &str, source: &str) {
    let mut document = Document::from_text(source);
    colour_like_the_editor(path, source, &mut document);
    let spans = document.chars().spans().count();
    let paragraphs = document.paragraphs().len();
    let text = document.text().len_bytes();

    let span_bytes = document.chars().accounted_bytes();
    let paragraph_bytes = paragraphs * size_of::<unluminous_core::ParagraphStyle>();
    let snapshot = text + span_bytes + paragraph_bytes;

    // And what one step really costs in time, measured rather than reasoned about. The caret is
    // moved between the deletes because a run of deleting is one undo step since `task-1984` C13.
    const STEPS: usize = 32;
    let caret = text / 2;
    let began = std::time::Instant::now();
    for _ in 0..STEPS {
        document.apply(unluminous_core::Command::PlaceCaret { offset: caret, extend: false });
        document.apply(unluminous_core::Command::DeleteBackward);
    }
    let took = began.elapsed().as_secs_f64() * 1000.0 / STEPS as f64;

    println!(
        "state=history spans={spans} paragraphs={paragraphs} span_bytes_each={}",
        unluminous_core::StyleSpans::SPAN_BYTES
    );
    println!(
        "snapshot_bytes={snapshot} snapshot_mb={:.2} text_mb={:.2} spans_mb={:.2} \
         whole_history_mb={:.1} ms_per_step={took:.2}",
        snapshot as f64 / 1024.0 / 1024.0,
        text as f64 / 1024.0 / 1024.0,
        span_bytes as f64 / 1024.0 / 1024.0,
        snapshot as f64 * unluminous_core::UNDO_LIMIT as f64 / 1024.0 / 1024.0,
    );
}
