//! Fields, tables of contents, captions and cross references through the session (task-2359, TDD 5.4):
//! each built in a blank document, laid out with fixed metrics, updated, written and read back.

use unluminous_core::FixedMetrics;
use unluminous_word::layout::{self, Options, Pages};
use unluminous_word::model::{Block, Content};
use unluminous_word::position::{self, Pos, Selection};
use unluminous_word::references::{field_list, find_tocs, TocOptions};
use unluminous_word::session::Session;

fn pages(session: &Session) -> Pages {
    layout::lay_out(&session.document, &FixedMetrics::default(), Options::default())
}

/// A document of three headings with body text between them, the caret at its start.
fn headings() -> Session {
    let mut session = Session::new(unluminous_word::new::blank());
    for (style, text) in [
        ("Heading1", "Introduction"),
        ("Normal", "Some words."),
        ("Heading2", "Background"),
        ("Normal", "More words."),
        ("Heading1", "Results"),
    ] {
        session.type_text(text);
        session.apply_style(style).expect("a style");
        session.enter();
    }
    session.selection = Selection::caret(Pos::new(vec![0], 0));
    session
}

fn reread(session: &Session) -> Session {
    let bytes = unluminous_word::write::save(&session.document).expect("saved");
    Session::new(unluminous_word::read::open(&bytes).expect("read back"))
}

#[test]
fn a_table_of_contents_is_a_content_control_listing_every_heading() {
    let mut session = headings();
    let laid = pages(&session);
    session.insert_toc(3, &laid);
    let places = find_tocs(&session.document);
    assert_eq!(places.len(), 1, "one table of contents");
    let sdt = places[0].sdt.clone().expect("in a content control");
    let Some(Block::Sdt(control)) = position::block(&session.document.body, &sdt) else {
        panic!("no content control")
    };
    let text: Vec<String> =
        position::paragraphs(&control.blocks).iter().map(|(_, p)| p.visible_text()).collect();
    assert_eq!(text[0], "Contents");
    assert!(text[1].starts_with("Introduction\t"), "{text:?}");
    assert!(text[2].starts_with("Background\t"));
    assert!(text[3].starts_with("Results\t"));
    // Each entry's page is a PAGEREF to a _Toc bookmark on its heading.
    let back = reread(&session);
    let back_places = find_tocs(&back.document);
    assert_eq!(back_places.len(), 1, "the table survives a save");
    assert!(back_places[0].code.contains("TOC \\o \"1-3\""), "{}", back_places[0].code);
    let bookmarks = position::paragraphs(&back.document.body)
        .iter()
        .flat_map(|(_, p)| p.marks.iter())
        .filter(|m| m.el.attr("w:name").is_some_and(|n| n.starts_with("_Toc")))
        .count();
    assert_eq!(bookmarks, 3, "a _Toc bookmark on each heading");
}

#[test]
fn updating_a_table_of_contents_picks_up_a_new_heading_and_keeps_its_levels() {
    let mut session = headings();
    let laid = pages(&session);
    session.insert_toc_with(&TocOptions { levels: 1, ..TocOptions::default() }, &laid);
    let last = position::paragraphs(&session.document.body).last().expect("a paragraph").0.clone();
    session.selection = Selection::caret(Pos::new(last, 0));
    session.type_text("Discussion");
    session.apply_style("Heading1").expect("a style");
    let laid = pages(&session);
    assert!(session.update_fields(&laid) >= 1);
    let place = &find_tocs(&session.document)[0];
    let Some(Block::Sdt(control)) =
        position::block(&session.document.body, place.sdt.as_ref().unwrap())
    else {
        panic!()
    };
    let entries: Vec<String> = position::paragraphs(&control.blocks)
        .iter()
        .map(|(_, p)| p.visible_text())
        .filter(|t| t.contains('\t'))
        .collect();
    assert_eq!(entries.len(), 3, "levels 1 only: {entries:?}");
    assert!(entries[2].starts_with("Discussion"));
    assert!(session.remove_toc());
    assert!(find_tocs(&session.document).is_empty());
}

#[test]
fn captions_number_themselves_and_a_table_of_figures_lists_them() {
    let mut session = headings();
    let laid = pages(&session);
    session.selection = Selection::caret(Pos::new(vec![1], 0));
    session.insert_caption("Figure", "A chart", true, &laid);
    session.selection = Selection::caret(Pos::new(vec![4], 0));
    session.insert_caption("Figure", "A photo", true, &laid);
    let captions: Vec<String> = position::paragraphs(&session.document.body)
        .iter()
        .map(|(_, p)| p.visible_text())
        .filter(|t| t.starts_with("Figure"))
        .collect();
    assert_eq!(captions, ["Figure 1 A chart", "Figure 2 A photo"]);
    session.selection = Selection::caret(Pos::new(vec![0], 0));
    let laid = pages(&session);
    session.insert_toc_with(
        &TocOptions { caption: Some("Figure".into()), ..TocOptions::default() },
        &laid,
    );
    let place = &find_tocs(&session.document)[0];
    assert!(place.code.contains("\\c \"Figure\""));
}

#[test]
fn a_cross_reference_shows_its_target_and_unlinking_keeps_the_text() {
    let mut session = headings();
    let target = vec![2];
    let end = position::paragraphs(&session.document.body).last().unwrap().0.clone();
    session.selection = Selection::caret(Pos::new(end.clone(), 0));
    let laid = pages(&session);
    session.insert_cross_reference(&target, "text", &laid).expect("a cross reference");
    let paragraph = position::paragraph(&session.document, &position::StoryId::Body, &end).unwrap();
    assert!(paragraph.visible_text().starts_with("Background"), "{}", paragraph.visible_text());
    // Unlinked, the result is plain text and no field is left.
    let length = paragraph.len();
    session.selection =
        Selection { anchor: Pos::new(end.clone(), 0), head: Pos::new(end.clone(), length) };
    assert_eq!(session.unlink_fields(), 1);
    let paragraph = position::paragraph(&session.document, &position::StoryId::Body, &end).unwrap();
    assert!(paragraph
        .runs
        .iter()
        .all(|r| !matches!(r.content, Content::Instr(_) | Content::FieldChar(_))));
    assert!(paragraph.visible_text().starts_with("Background"));
}

#[test]
fn a_locked_field_keeps_its_result_through_update() {
    let mut session = Session::new(unluminous_word::new::blank());
    session.insert_field("DATE \\@ \"yyyy\"", "1999");
    let length =
        position::paragraph(&session.document, &position::StoryId::Body, &[0]).unwrap().len();
    session.selection = Selection { anchor: Pos::new(vec![0], 0), head: Pos::new(vec![0], length) };
    assert_eq!(session.lock_fields(true), 1);
    let laid = pages(&session);
    session.update_fields(&laid);
    assert_eq!(field_list(&session.document)[0].2, "1999");
    session.lock_fields(false);
    session.update_fields(&laid);
    assert_ne!(field_list(&session.document)[0].2, "1999", "an unlocked date updates");
}

/// The text drawn on the first page, in order.
fn drawn(pages: &Pages) -> String {
    pages.pages[0]
        .items
        .iter()
        .filter_map(|item| match item {
            layout::Item::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn field_codes_draw_the_code_in_braces_instead_of_the_result() {
    let mut session = Session::new(unluminous_word::new::blank());
    session.type_text("Words: ");
    session.insert_field("NUMWORDS", "1");
    let metrics = FixedMetrics::default();
    let results = layout::lay_out(&session.document, &metrics, Options::default());
    assert!(drawn(&results).contains("Words: 1"), "{}", drawn(&results));
    let codes = layout::lay_out(
        &session.document,
        &metrics,
        Options { show_codes: true, ..Options::default() },
    );
    assert!(drawn(&codes).contains("{ NUMWORDS }"), "{}", drawn(&codes));
    assert!(!drawn(&codes).contains("Words: 1"));
}

/// Every field code and switch in `field-codes.docx`, updated by Unluminous, gives the result Word gave it
/// (TDD section 8). The fixture is made by `make-fixtures.ps1`, which has Word update every field before
/// it saves, and `field-codes.word.json` holds Word's results in document order.
#[test]
fn every_field_code_and_switch_gives_words_result() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let bytes = std::fs::read(folder.join("field-codes.docx")).unwrap();
    let report: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(folder.join("field-codes.word.json"))
            .unwrap()
            .trim_start_matches('\u{feff}'),
    )
    .unwrap();
    let mut session = Session::new(unluminous_word::read::open(&bytes).unwrap());
    session.file = Some(folder.join("field-codes.docx"));
    // What the evaluator works out, so a field it does not understand cannot pass on Word's stored result.
    let ours = session.evaluated_fields(&pages(&session));
    let words: Vec<(String, String)> = report["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let code = f["code"].as_str().unwrap_or_default().trim().to_owned();
            let result = f["result"].as_str().unwrap_or_default().trim_end_matches('\r').to_owned();
            (code, result)
        })
        .collect();
    assert_eq!(ours.len(), words.len(), "the same fields: ours {ours:?}");
    let mut wrong = Vec::new();
    for ((code, result), (word_code, word_result)) in ours.iter().zip(&words) {
        assert_eq!(
            code.split_whitespace().collect::<Vec<_>>(),
            word_code.split_whitespace().collect::<Vec<_>>()
        );
        match result {
            Some(result) if result.trim() == word_result.trim() => {}
            Some(result) => wrong.push(format!("{code}: ours {result:?}, Word's {word_result:?}")),
            None => wrong.push(format!("{code}: not worked out, Word's {word_result:?}")),
        }
    }
    assert!(wrong.is_empty(), "fields that disagree with Word:\n{}", wrong.join("\n"));
}
