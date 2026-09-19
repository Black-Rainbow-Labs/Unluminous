//! What one frame of the editing area costs, measured with real fonts on this machine.
//!
//! `task-1666` began as a report that selecting text, scrolling and dragging the window were jagged
//! on Windows with a few tabs open. This is how that was turned into numbers rather than an
//! impression: it lays a real file out, collects the glyphs the painter collects, and prints the
//! milliseconds each part of a frame takes. Run it before and after a change and the difference is
//! visible.
//!
//! `cargo run --release -p unluminous-app --example frame_cost -- <file> [width]`
//!
//! The three lines at the end are the three things the ticket reported. Each is the work the window
//! really does for one frame of that gesture, and each has a comment beside it saying which.

use std::time::Instant;

use unluminous_app::services::text_renderer::TextRenderer;
use unluminous_core::{layout, relayout, Command, Document, Layout, LayoutTextView, Rope};

/// Run `body` `runs` times and give back the mean in milliseconds.
fn timed(runs: usize, mut body: impl FnMut()) -> f64 {
    // One untimed pass first, so a cache that fills on first use is not charged to the measurement.
    body();
    let start = Instant::now();
    for _ in 0..runs {
        body();
    }
    start.elapsed().as_secs_f64() * 1000.0 / runs as f64
}

/// Every glyph the painter would collect for the whole of a layout, which is what it used to do.
fn collect_every_glyph(renderer: &TextRenderer, text: &Rope, laid: &Layout) -> usize {
    collect_glyphs(renderer, text, laid, &laid.lines)
}

/// The same, for the lines that fall inside a window `height` points tall at `scroll`.
fn collect_visible_glyphs(
    renderer: &TextRenderer,
    text: &Rope,
    laid: &Layout,
    scroll: f32,
    height: f32,
) -> usize {
    let visible = laid.visible_lines(scroll, scroll + height);
    collect_glyphs(renderer, text, laid, &laid.lines[visible])
}

fn collect_glyphs(
    renderer: &TextRenderer,
    text: &Rope,
    laid: &Layout,
    lines: &[unluminous_core::PlacedLine],
) -> usize {
    let view = LayoutTextView::new(laid, text);
    let mut placed = 0usize;
    for line in lines {
        for run in &line.runs {
            for cluster in line.run_clusters(run) {
                view.for_each_character(cluster, |character| {
                    if renderer.glyph(character, &run.style).is_some() {
                        placed += 1;
                    }
                });
            }
        }
    }
    placed
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let path =
        arguments.next().unwrap_or_else(|| "crates/unluminous-app/src/app/mod.rs".to_owned());
    let width: f32 = arguments.next().and_then(|w| w.parse().ok()).unwrap_or(900.0);
    let view_height = 720.0_f32;

    let source = std::fs::read_to_string(&path).expect("the file to measure against");
    let renderer = TextRenderer::new();
    let mut document = Document::from_text(&source);

    // Coloured, because that is how a source file is really shown and the colouring is what turns one
    // style span into thousands. Measuring against a plain document flatters every number here.
    let (plugins, _) = unluminous_app::services::plugins::Plugins::load(None);
    let coloured = plugins.for_path(std::path::Path::new(&path)).map(|plugin| {
        let base = unluminous_core::Color::rgb(0xF2, 0xF2, 0xF2);
        let start = Instant::now();
        let tokens = unluminous_core::syntax::highlight(&source, &plugin.grammar);
        let tokenised = start.elapsed().as_secs_f64() * 1000.0;
        let spans: Vec<(std::ops::Range<usize>, unluminous_core::Color)> = tokens
            .into_iter()
            .filter_map(|(range, token)| plugin.theme.colour(token).map(|colour| (range, colour)))
            .collect();
        let count = spans.len();
        let start = Instant::now();
        document.set_syntax(base, &spans);
        let applied = start.elapsed().as_secs_f64() * 1000.0;
        (tokenised, applied, count)
    });

    println!(
        "{path}: {} bytes, {} lines, laid out at {width} points wide",
        source.len(),
        document.text().len_lines()
    );
    if let Some((tokenised, applied, count)) = coloured {
        println!("  syntax highlight:        {tokenised:8.2} ms  ({count} coloured spans)");
        println!(
            "  set_syntax:              {applied:8.2} ms  ({} style spans after)",
            document.chars().span_count()
        );
    }

    let laid = layout(document.text(), document.chars(), document.paragraphs(), &renderer, width);
    println!("            lines laid out: {}", laid.lines.len());

    let ms = timed(5, || {
        let _ = layout(document.text(), document.chars(), document.paragraphs(), &renderer, width);
    });
    println!("  layout, whole document:  {ms:8.2} ms");

    let every = collect_every_glyph(&renderer, document.text(), &laid);
    let ms = timed(10, || {
        std::hint::black_box(collect_every_glyph(&renderer, document.text(), &laid));
    });
    println!("  glyphs, whole document:  {ms:8.2} ms  ({every} glyphs)");

    let visible = collect_visible_glyphs(&renderer, document.text(), &laid, 0.0, view_height);
    let ms = timed(200, || {
        std::hint::black_box(collect_visible_glyphs(
            &renderer,
            document.text(),
            &laid,
            0.0,
            view_height,
        ));
    });
    println!("  glyphs, one screenful:   {ms:8.2} ms  ({visible} glyphs)");

    let end = document.text().len_bytes();
    let ms = timed(20, || {
        std::hint::black_box(laid.selection_rects(0..end));
    });
    println!("  selection_rects, all:    {ms:8.2} ms");

    let onscreen = laid.visible_lines(0.0, view_height);
    let ms = timed(2000, || {
        std::hint::black_box(laid.selection_rects_in(onscreen.clone(), 0..end));
    });
    println!("  selection_rects, shown:  {ms:8.2} ms");

    let ms = timed(20, || {
        std::hint::black_box(laid.decorations(&renderer));
    });
    println!("  decorations, whole doc:  {ms:8.2} ms");

    let style = unluminous_core::CharStyle::default();
    let ms = timed(200_000, || {
        std::hint::black_box(unluminous_core::FontMetrics::advance(&renderer, "a", &style));
    });
    println!("  one advance:             {:8.1} ns", ms * 1_000_000.0);
    let ms = timed(200_000, || {
        std::hint::black_box(renderer.glyph('a', &style));
    });
    println!("  one glyph lookup:        {:8.1} ns", ms * 1_000_000.0);

    let ms = timed(2000, || {
        std::hint::black_box(laid.line_of_offset(end));
    });
    println!("  line_of_offset, last:    {ms:8.4} ms");

    let ms = timed(2000, || {
        std::hint::black_box(laid.line_at_y(laid.height));
    });
    println!("  line_at_y, bottom:       {ms:8.4} ms");

    println!();
    println!("  and the three gestures the ticket reported:");

    // Dragging a selection. The caret moves, so the text revision does not, so nothing is laid out
    // and nothing is coloured: the frame is the selection rectangles and a screenful of glyphs.
    let ms = timed(500, || {
        std::hint::black_box(laid.selection_rects_in(onscreen.clone(), 0..end / 2));
        std::hint::black_box(collect_visible_glyphs(
            &renderer,
            document.text(),
            &laid,
            0.0,
            view_height,
        ));
    });
    println!("  dragging a selection:    {ms:8.2} ms  ({:.0} frames a second)", 1000.0 / ms);

    // Scrolling, and dragging the window: nothing about the document changes at all, so the frame is
    // a screenful of glyphs and nothing else.
    let ms = timed(500, || {
        std::hint::black_box(collect_visible_glyphs(
            &renderer,
            document.text(),
            &laid,
            4000.0,
            view_height,
        ));
    });
    println!("  scrolling or dragging:   {ms:8.2} ms  ({:.0} frames a second)", 1000.0 / ms);

    // Typing a letter. The text really did change, so it is laid out again — but only the paragraph
    // that changed — and then a screenful of glyphs is collected.
    document.apply(Command::PlaceCaret { offset: end / 2, extend: false });
    let mut carried =
        layout(document.text(), document.chars(), document.paragraphs(), &renderer, width);
    let ms = timed(50, || {
        document.apply(Command::Insert("x".to_owned()));
        carried = relayout(
            std::mem::take(&mut carried),
            document.text(),
            document.chars(),
            document.paragraphs(),
            &renderer,
            width,
            &unluminous_core::folding::Hidden::none(),
        );
        std::hint::black_box(collect_visible_glyphs(
            &renderer,
            document.text(),
            &carried,
            0.0,
            view_height,
        ));
    });
    println!("  typing a letter:         {ms:8.2} ms  ({:.0} frames a second)", 1000.0 / ms);

    // **The same keystroke, with the document saying which paragraph it was typed into**
    // (`task-1984` C7). The reading above is what the window did until then: `relayout` found the
    // paragraphs to lay out again by fingerprinting every paragraph in the document and comparing
    // each against the previous layout's, which reads and hashes the whole file on every key press.
    // Measured on a 2 MB file with nothing else changed, that pass was 17.2 ms of a 21.7 ms
    // keystroke. `Document::touched_since` answers it from what `splice` recorded instead.
    let ms = timed(50, || {
        let was = document.text_revision();
        document.apply(Command::Insert("x".to_owned()));
        carried = unluminous_core::relayout_touching(
            std::mem::take(&mut carried),
            document.text(),
            document.chars(),
            document.paragraphs(),
            &renderer,
            width,
            &unluminous_core::folding::Hidden::none(),
            document.touched_since(was),
        );
        std::hint::black_box(collect_visible_glyphs(
            &renderer,
            document.text(),
            &carried,
            0.0,
            view_height,
        ));
    });
    println!("  typing, told where:      {ms:8.2} ms  ({:.0} frames a second)", 1000.0 / ms);

    // The same, as the window really does it: a source file is coloured again after every edit.
    //
    // **Both readings, in one run**, because the point of `task-1804` §5.2 is the difference
    // between them and a number with nothing beside it is a number nobody can judge. The first is
    // what the window did until then -- read the whole file, lay every span back over it -- and the
    // second is what `UnluminousApp::colour_the_file` does now.
    if let Some(plugin) = plugins.for_path(std::path::Path::new(&path)) {
        let base = unluminous_core::Color::rgb(0xF2, 0xF2, 0xF2);
        let ms = timed(20, || {
            document.apply(Command::Insert("y".to_owned()));
            let text = document.text().to_string();
            let spans: Vec<(std::ops::Range<usize>, unluminous_core::Color)> =
                unluminous_core::syntax::highlight(&text, &plugin.grammar)
                    .into_iter()
                    .filter_map(|(range, token)| plugin.theme.colour(token).map(|c| (range, c)))
                    .collect();
            document.set_syntax(base, &spans);
            carried = relayout(
                std::mem::take(&mut carried),
                document.text(),
                document.chars(),
                document.paragraphs(),
                &renderer,
                width,
                &unluminous_core::folding::Hidden::none(),
            );
            std::hint::black_box(collect_visible_glyphs(
                &renderer,
                document.text(),
                &carried,
                0.0,
                view_height,
            ));
        });
        println!("  typing, whole file read: {ms:8.2} ms  ({:.0} frames a second)", 1000.0 / ms);

        let mut cache = unluminous_core::IncrementalTokens::default();
        // One reading first, so what is timed is the incremental case rather than the first one.
        {
            let text = document.text().to_string();
            let mut spans = Vec::new();
            cache.update(&text, &plugin.grammar, document.syntax_dirt(), |range, token| {
                if let Some(colour) = plugin.theme.colour(token) {
                    spans.push((range, colour));
                }
            });
            let _: &Vec<(std::ops::Range<usize>, unluminous_core::Color)> = &spans;
            document.set_syntax(base, &spans);
        }
        let mut scanned = 0usize;
        let ms = timed(20, || {
            // The window's own path end to end, hint and all, which is what `task-1984` C7 changed.
            let was = document.text_revision();
            document.apply(Command::Insert("z".to_owned()));
            let text = document.text().to_string();
            let mut spans: Vec<(std::ops::Range<usize>, unluminous_core::Color)> = Vec::new();
            let update =
                cache.update(&text, &plugin.grammar, document.syntax_dirt(), |range, token| {
                    if let Some(colour) = plugin.theme.colour(token) {
                        spans.push((range, colour));
                    }
                });
            scanned = update.scanned;
            document.set_syntax_in(base, &spans, update.changed);
            carried = unluminous_core::relayout_touching(
                std::mem::take(&mut carried),
                document.text(),
                document.chars(),
                document.paragraphs(),
                &renderer,
                width,
                &unluminous_core::folding::Hidden::none(),
                document.touched_since(was),
            );
            std::hint::black_box(collect_visible_glyphs(
                &renderer,
                document.text(),
                &carried,
                0.0,
                view_height,
            ));
        });
        println!(
            "  typing, coloured again:  {ms:8.2} ms  ({:.0} frames a second, {scanned} tokens read)",
            1000.0 / ms
        );

        // **The tokeniser's own share of a keystroke** (`task-1984` C8). `Tokens::update` allocated a
        // fresh vector the size of the whole token list and copied the untouched prefix and the tail
        // into it, and `safe_start` walked every token in front of the edit -- 5.25 ms a keystroke on
        // a 2 MB file while reading fourteen tokens. It is measured on its own here because the
        // reading above has a relayout and a screenful of glyphs in it, which are larger and moved
        // for their own reasons.
        const KEYSTROKES: usize = 20;
        let mut spent = std::time::Duration::ZERO;
        for _ in 0..KEYSTROKES {
            document.apply(Command::Insert("w".to_owned()));
            // Outside the clock: reading the rope into a `String` is 2 MB of its own and is a cost
            // the reading above already carries.
            let text = document.text().to_string();
            let dirt = document.syntax_dirt();
            let began = Instant::now();
            let update = cache.update(&text, &plugin.grammar, dirt, |_, _| {});
            spent += began.elapsed();
            document.set_syntax_in(base, &[], update.changed);
            std::hint::black_box(update.scanned);
        }
        let ms = spent.as_secs_f64() * 1000.0 / KEYSTROKES as f64;
        println!("  the tokeniser alone:     {ms:8.2} ms  ({} tokens held)", cache.all().len());
    }

    measure_the_caret(&source);
    measure_replace_all(&path, &source, &plugins);
}

/// What one arrow key costs on a file with no line breaks in it.
///
/// **`task-1984` C5.** `Document::line_window` handed the grapheme and word walks the whole line,
/// copied into a fresh `String` -- which is a few dozen bytes in ordinary source and is the whole
/// file in a minified `.js`, a one line `.json` or a single log line. The review measured **15.9 ms
/// for one `MoveRight`** on a megabyte of one line, against 0.002 ms on the same bytes with line
/// breaks in them, so holding an arrow key dropped every frame.
///
/// Both readings are printed in one run, for the reason the two typing readings above are: a number
/// with nothing beside it is a number nobody can judge. The same bytes are measured twice, once as
/// the file really is and once with every line break taken out of it.
fn measure_the_caret(source: &str) {
    println!();
    println!("  and one arrow key, on the same bytes read two ways:");
    let flat = source.replace('\n', " ");
    for (shape, text) in [("as it is", source), ("with no line breaks", flat.as_str())] {
        let mut document = Document::from_text(text);
        let middle = on_a_boundary(text, text.len() / 2);
        let ms = timed(200, || {
            document.apply(Command::PlaceCaret { offset: middle, extend: false });
            document.apply(Command::MoveRight { extend: false });
        });
        let lines = document.text().len_lines();
        println!("  MoveRight, {shape:<19} {ms:8.3} ms  ({} bytes, {lines} lines)", text.len());
        let ms = timed(200, || {
            document.apply(Command::PlaceCaret { offset: middle, extend: false });
            document.apply(Command::MoveWordRight { extend: false });
        });
        println!("  MoveWordRight, {shape:<15} {ms:8.3} ms");
    }
}

/// `offset` moved forward to the nearest character boundary.
fn on_a_boundary(text: &str, mut offset: usize) -> usize {
    while offset < text.len() && !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

/// What Replace All costs on a coloured file.
///
/// **`task-1984` C6.** `Document::replace_many` coloured each replacement with `StyleSpans::set`,
/// which rebuilds the whole span list -- so a rename or a Find in Files Replace All over 210 matches
/// rebuilt it 210 times. The review measured **27.5 ms coloured against 0.24 ms uncoloured** on a
/// 117 KB file, which is the cost `task-1804` §5.2 took out of one keystroke, left behind on the one
/// path that pays it hundreds of times in a row.
///
/// Both readings are printed, because the difference between them is the finding: the same
/// replacements are applied to the same text, once with the file coloured and once without, and a
/// colour scheme is not supposed to change what an edit costs.
fn measure_replace_all(
    path: &str,
    source: &str,
    plugins: &unluminous_app::services::plugins::Plugins,
) {
    println!();
    println!("  and Replace All, on the same file read two ways:");
    // A word that really is all over the file, so the matches are in real places rather than planted
    // at regular intervals.
    let needle = "self";
    let found: Vec<std::ops::Range<usize>> =
        source.match_indices(needle).map(|(at, _)| at..at + needle.len()).take(210).collect();
    if found.is_empty() {
        println!("  nothing to replace in this file");
        return;
    }
    let count = found.len();
    for coloured in [false, true] {
        let mut document = Document::from_text(source);
        let mut spans = 1;
        if coloured {
            colour_like_the_editor(path, source, &mut document, plugins);
            spans = document.chars().spans().count();
        }
        let edits: Vec<(std::ops::Range<usize>, String)> =
            found.iter().map(|range| (range.clone(), "this".to_owned())).collect();
        let began = Instant::now();
        document.apply(Command::ReplaceMany(edits));
        let ms = began.elapsed().as_secs_f64() * 1000.0;
        let shape = match coloured {
            true => "coloured",
            false => "uncoloured",
        };
        println!("  {count} replacements, {shape:<10} {ms:8.2} ms  ({spans} spans)");
    }
}

/// Colour a document the way `UnluminousApp::colour_the_file` does, if a plugin claims the file.
fn colour_like_the_editor(
    path: &str,
    source: &str,
    document: &mut Document,
    plugins: &unluminous_app::services::plugins::Plugins,
) {
    let Some(plugin) = plugins.for_path(std::path::Path::new(path)) else { return };
    let base = unluminous_core::Color::rgb(0xF2, 0xF2, 0xF2);
    let spans: Vec<(std::ops::Range<usize>, unluminous_core::Color)> =
        unluminous_core::syntax::highlight(source, &plugin.grammar)
            .into_iter()
            .filter_map(|(range, token)| plugin.theme.colour(token).map(|colour| (range, colour)))
            .collect();
    document.set_syntax(base, &spans);
}
