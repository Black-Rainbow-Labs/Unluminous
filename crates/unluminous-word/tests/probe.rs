//! A diagnostic: where each paragraph starts against where Word starts it. Run with
//! `cargo test -p unluminous-word --test probe -- --ignored --nocapture`.

mod common;

use std::path::Path;
use unluminous_core::FontMetrics;

#[test]
#[ignore]
fn probe() {
    let fonts = common::RealFonts::new();
    let name = std::env::var("PROBE").unwrap_or_else(|_| "long".into());
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format!("{name}.docx"));
    let document = unluminous_word::read::open(&std::fs::read(&path).unwrap()).unwrap();
    let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
    let report: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(path.with_extension("word.json"))
            .unwrap()
            .trim_start_matches('\u{feff}'),
    )
    .unwrap();
    let words = report["paragraphs"].as_array().unwrap();
    let ours = unluminous_word::position::paragraphs(&document.body);
    let mut word_index = 0;
    for (path, paragraph) in ours.iter().take(40) {
        // Word's list has a row end mark after each row; skip those to stay in step.
        while words.get(word_index).is_some_and(|w| {
            w["inTable"].as_bool() == Some(true)
                && w["text"].as_str().is_some_and(|t| t.trim().is_empty())
                && !paragraph.flat_text().is_empty()
        }) {
            word_index += 1;
        }
        let Some(word) = words.get(word_index) else { break };
        word_index += 1;
        let mut ours_at = None;
        let mut lines = 0;
        for (index, page) in pages.pages.iter().enumerate() {
            for line in page
                .carets
                .iter()
                .filter(|l| l.path == *path && l.story == unluminous_word::position::StoryId::Body)
            {
                if ours_at.is_none() {
                    ours_at = Some((index + 1, line.top, line.height));
                }
                lines += 1;
            }
        }
        let (page, top, height) = ours_at.unwrap_or((0, 0.0, 0.0));
        println!(
            "{:?} p{} top {:.1} (Word p{} {:.1}) lines {} (Word {}) h {:.2} {:?}",
            path,
            page,
            top,
            word["page"],
            word["top"].as_f64().unwrap_or(0.0),
            lines,
            word["lineCount"],
            height,
            paragraph.flat_text().chars().take(30).collect::<String>()
        );
    }
    for family in ["Aptos", "Aptos Display", "Calibri", "Times New Roman"] {
        let style =
            unluminous_core::CharStyle { family: family.into(), size: 12.0, ..Default::default() };
        let m = fonts.line_metrics(&style);
        println!(
            "{family} 12pt: ascent {:.3} descent {:.3} gap {:.3} total {:.3}",
            m.ascent,
            m.descent,
            m.line_gap,
            m.height()
        );
    }
}

#[test]
#[ignore]
fn kerning_probe() {
    let fonts = common::RealFonts::new();
    let style =
        unluminous_core::CharStyle { family: "Aptos".into(), size: 12.0, ..Default::default() };
    for (a, b) in [('T', 'o'), ('A', 'V'), ('e', ','), ('r', '.'), ('a', 'y'), ('y', '.')] {
        println!("kern {a}{b} = {:.3}", fonts.kerning(a, b, &style));
    }
    let text =
        "Through plain wraps margin the a shows with between the through plain wraps margin the";
    let width: f32 = text.chars().map(|c| fonts.advance(&c.to_string(), &style)).sum();
    let kerned: f32 =
        text.chars().zip(text.chars().skip(1)).map(|(a, b)| fonts.kerning(a, b, &style)).sum();
    println!("width {width:.2} kerned {kerned:.2}");
}

#[test]
#[ignore]
fn lines_probe() {
    let fonts = common::RealFonts::new();
    let name = std::env::var("PROBE_FIXTURE").unwrap_or_else(|_| "headers".into());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}.docx"));
    let document = unluminous_word::read::open(&std::fs::read(&path).unwrap()).unwrap();
    let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
    let target: Vec<u32> = std::env::var("PROBE_PATH")
        .map(|p| p.split(',').filter_map(|n| n.parse().ok()).collect())
        .unwrap_or_else(|_| vec![4]);
    let p = unluminous_word::position::paragraph(
        &document,
        &unluminous_word::position::StoryId::Body,
        &target,
    )
    .unwrap();
    let text = p.flat_text();
    println!("props {}", p.props.to_xml());
    for page in &pages.pages {
        for line in page.carets.iter().filter(|l| l.path == target) {
            let first = line.stops.first().map_or(0.0, |s| s.1);
            let last = line.stops.last().map_or(0.0, |s| s.1);
            println!(
                "{:.1}..{:.1} ({:.1}) {:?}",
                first,
                last,
                last - first,
                &text[line.start..line.end.min(text.len())]
            );
        }
        println!("body {:?}", page.body);
    }
}

#[test]
#[ignore]
fn fonts_probe() {
    let fonts = common::RealFonts::new();
    let path = std::env::var("PROBE_FILE").unwrap();
    let document = unluminous_word::read::open(&std::fs::read(&path).unwrap()).unwrap();
    let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
    for (path, p) in unluminous_word::position::paragraphs(&document.body) {
        let text = p.flat_text();
        let resolver = unluminous_word::styles::Resolver::new(&document);
        let look = resolver.run(&p.props, &p.runs[0].props, None);
        println!("== {} {} kern {:?}", look.font, look.size, look.kern);
        for page in &pages.pages {
            for line in page.carets.iter().filter(|l| l.path == path) {
                println!("  {}", text[line.start..line.end.min(text.len())].trim());
            }
        }
    }
}

/// Our advance for each printable character in `PROBE_FAMILY` at `PROBE_SIZE`, as JSON, to compare with
/// `tools/word-oracle/advances.ps1`.
#[test]
#[ignore]
fn advances_probe() {
    let fonts = common::RealFonts::new();
    let family = std::env::var("PROBE_FAMILY").unwrap_or_else(|_| "Aptos".into());
    let size: f32 = std::env::var("PROBE_SIZE").ok().and_then(|s| s.parse().ok()).unwrap_or(12.0);
    let style =
        unluminous_core::CharStyle { family: family.as_str().into(), size, ..Default::default() };
    let mut map = serde_json::Map::new();
    for code in (32u32..=126).chain([0x2019, 0x201C, 0x201D, 0x2013, 0x2014, 0x2026]) {
        let c = char::from_u32(code).unwrap();
        map.insert(c.to_string(), serde_json::json!(fonts.advance(&c.to_string(), &style)));
    }
    println!("ADVANCES {}", serde_json::Value::Object(map));
}

/// How wide each of `PROBE_TEXT`'s `|` separated pieces is in Aptos 12 with the real fonts, laid out as
/// a paragraph of its own on a Letter page with Word's one inch margins.
#[test]
#[ignore]
fn width_probe() {
    let fonts = common::RealFonts::new();
    for text in std::env::var("PROBE_TEXT").unwrap().split('|') {
        let mut session = unluminous_word::session::Session::new(unluminous_word::new::blank());
        session.type_text(text);
        let pages = unluminous_word::layout::lay_out(&session.document, &fonts, Default::default());
        for line in &pages.pages[0].carets {
            let first = line.stops.first().map_or(0.0, |s| s.1);
            let last = line.stops.last().map_or(0.0, |s| s.1);
            println!("{:.2} {:?}", last - first, &text[line.start..line.end.min(text.len())]);
        }
    }
}

/// The footnote lines on each page of `PROBE_FIXTURE`: where each starts and how tall it is.
#[test]
#[ignore]
fn notes_probe() {
    let fonts = common::RealFonts::new();
    let name = std::env::var("PROBE_FIXTURE").unwrap_or_else(|_| "notes".into());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}.docx"));
    let document = unluminous_word::read::open(&std::fs::read(&path).unwrap()).unwrap();
    let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
    for (n, page) in pages.pages.iter().enumerate() {
        println!("page {} notes {:?}", n + 1, page.notes);
        for line in &page.carets {
            if matches!(line.story, unluminous_word::position::StoryId::Footnote(_)) {
                println!("  {:.2} +{:.2}", line.top, line.height);
            }
        }
    }
}

/// Where each body paragraph's first line starts, page and top, beside Word's report.
#[test]
#[ignore]
fn tops_probe() {
    let fonts = common::RealFonts::new();
    let name = std::env::var("PROBE_FIXTURE").unwrap_or_else(|_| "builtin-styles".into());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}.docx"));
    let document = unluminous_word::read::open(&std::fs::read(&path).unwrap()).unwrap();
    let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
    let report: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(path.with_extension("word.json")).unwrap().trim_start_matches('\u{feff}'),
    )
    .unwrap();
    let words = report["paragraphs"].as_array().unwrap();
    for (index, (path, p)) in unluminous_word::position::paragraphs(&document.body).iter().enumerate() {
        let found = pages.pages.iter().enumerate().find_map(|(n, page)| {
            page.carets.iter().find(|l| &l.path == path).map(|l| (n + 1, l.top))
        });
        let word = words.get(index);
        println!(
            "{:3} ours {:?} word {:?} {:?} {}",
            index + 1,
            found,
            word.map(|w| w["page"].clone()),
            word.map(|w| w["top"].clone()),
            p.flat_text().chars().take(30).collect::<String>()
        );
    }
}

/// How long laying out a fixture takes with the real fonts, for the performance work.
#[test]
#[ignore]
fn layout_time_probe() {
    let fonts = common::RealFonts::new();
    let name = std::env::var("PROBE_FIXTURE").unwrap_or_else(|_| "long".into());
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}.docx"));
    let document = unluminous_word::read::open(&std::fs::read(&path).unwrap()).unwrap();
    for _ in 0..3 {
        let started = std::time::Instant::now();
        let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
        println!("{} pages in {:?}", pages.pages.len(), started.elapsed());
    }
}

/// Write `PROBE_OUT` as a fixture encrypted with the password `s3cret`, or decrypt `PROBE_IN` with it.
#[test]
#[ignore]
fn crypto_probe() {
    if let Ok(out) = std::env::var("PROBE_OUT") {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kitchen-sink.docx");
        let package = std::fs::read(path).unwrap();
        std::fs::write(&out, unluminous_word::crypto::encrypt(&package, "s3cret").unwrap()).unwrap();
        println!("wrote {out}");
    }
    if let Ok(input) = std::env::var("PROBE_IN") {
        let bytes = std::fs::read(&input).unwrap();
        let package = unluminous_word::crypto::decrypt(&bytes, "s3cret").unwrap();
        let document = unluminous_word::read::open(&package).unwrap();
        println!("decrypted {} paragraphs", unluminous_word::position::paragraphs(&document.body).len());
    }
}

/// Write `PROBE_OUT` with Restrict Editing set to read only with the password `s3cret`.
#[test]
#[ignore]
fn protection_probe() {
    let out = std::env::var("PROBE_OUT").unwrap();
    let mut session = unluminous_word::session::Session::new(unluminous_word::new::blank());
    session.type_text("Protected text");
    session.lock_tracking("s3cret");
    std::fs::write(&out, unluminous_word::write::save(&session.document).unwrap()).unwrap();
    println!("wrote {out}");
}

/// Write `PROBE_OUT` with a password to modify, `s3cret`, and `PROBE_OUT2` restricted to read only.
#[test]
#[ignore]
fn modify_probe() {
    let mut session = unluminous_word::session::Session::new(unluminous_word::new::blank());
    session.type_text("Text");
    session.set_password_to_modify("s3cret");
    std::fs::write(std::env::var("PROBE_OUT").unwrap(), unluminous_word::write::save(&session.document).unwrap()).unwrap();
    let mut session = unluminous_word::session::Session::new(unluminous_word::new::blank());
    session.type_text("Text");
    session.restrict_editing("readOnly", "s3cret").unwrap();
    std::fs::write(std::env::var("PROBE_OUT2").unwrap(), unluminous_word::write::save(&session.document).unwrap()).unwrap();
}
