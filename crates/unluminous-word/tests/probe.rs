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
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/headers.docx");
    let document = unluminous_word::read::open(&std::fs::read(&path).unwrap()).unwrap();
    let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
    let target = vec![4u32];
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
