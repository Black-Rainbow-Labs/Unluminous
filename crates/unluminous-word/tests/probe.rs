//! A diagnostic: line counts and line heights against Word's statistics. Run with `--nocapture`.

mod common;

use std::path::Path;
use unluminous_core::FontMetrics;

#[test]
#[ignore]
fn probe() {
    let fonts = common::RealFonts::new();
    for name in ["long", "paragraphs", "styles"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(format!("{name}.docx"));
        let document = unluminous_word::read::open(&std::fs::read(&path).unwrap()).unwrap();
        let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
        let lines: usize = pages.pages.iter().map(|p| p.carets.iter().filter(|l| l.story == unluminous_word::position::StoryId::Body).count()).sum();
        let report: serde_json::Value = serde_json::from_str(std::fs::read_to_string(path.with_extension("word.json")).unwrap().trim_start_matches('\u{feff}')).unwrap();
        println!("{name}: {lines} lines against Word's {}", report["lines"]);
        let first = &pages.pages[0];
        for line in first.carets.iter().take(6) {
            println!("   line {:?} top {:.2} height {:.2} {}..{}", line.path, line.top, line.height, line.start, line.end);
        }
    }
    for family in ["Aptos", "Calibri", "Times New Roman", "Aptos Display"] {
        let style = unluminous_core::CharStyle { family: family.into(), size: 12.0, ..Default::default() };
        let m = fonts.line_metrics(&style);
        println!("{family} 12pt: ascent {:.3} descent {:.3} gap {:.3} total {:.3}; 'abc xyz' {:.3}", m.ascent, m.descent, m.line_gap, m.height(), "abc xyz".chars().map(|c| fonts.advance(&c.to_string(), &style)).sum::<f32>());
    }
}
