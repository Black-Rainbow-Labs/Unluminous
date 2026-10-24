//! The Word provider with the window's own fonts, and its commands as an agent sends them.

use std::path::{Path, PathBuf};

use unluminous_core::{CharStyle, FontMetrics};

use super::fonts::WordFonts;
use super::*;
use crate::services::text_renderer::TextRenderer;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../unluminous-word/tests/fixtures").join(name)
}

fn style(family: &str, size: f32) -> CharStyle {
    CharStyle {
        family: family.into(),
        size,
        color: unluminous_core::Color::rgb(0, 0, 0),
        ..CharStyle::default()
    }
}

/// **The window measures a 12 point face as 12 points to the em**, which is what Word means by the size.
/// The renderer sizes a face by its ascent to descent, so without `WordFonts` a 12 point Aptos was measured
/// at 9.8 points and every page held a fifth more text than Word's.
#[test]
fn the_window_measures_text_at_words_sizes() {
    let renderer = TextRenderer::new();
    if !renderer.families().iter().any(|f| f == "Calibri") {
        println!("Calibri is not installed, so this test checked nothing");
        return;
    }
    let fonts = WordFonts::new(&renderer);
    let calibri = style("Calibri", 12.0);
    // Calibri's 'a' is 981 units of 2048 wide: 5.748 points at 12.
    let a = fonts.advance("a", &calibri);
    assert!((a - 981.0 / 2048.0 * 12.0).abs() < 0.02, "a is {a}");
    let metrics = fonts.line_metrics(&calibri);
    assert!(
        (metrics.height() - 14.648).abs() < 0.05,
        "a line of Calibri 12 is {}",
        metrics.height()
    );
}

/// The lines of a paragraph in Calibri, Times New Roman and Arial break where Word breaks them, measured
/// with the window's fonts. `unluminous-word`'s `tests/pagination.rs` checks the same with fonts read
/// directly; this is the window's half.
#[test]
fn the_window_breaks_lines_where_word_does() {
    let renderer = TextRenderer::new();
    let has = |family: &str| renderer.families().iter().any(|f| f == family);
    if !has("Calibri") || !has("Times New Roman") || !has("Arial") {
        println!("the fonts are not installed, so this test checked nothing");
        return;
    }
    let fonts = WordFonts::new(&renderer);
    let document =
        unluminous_word::read::open(&std::fs::read(fixture("line-breaks.docx")).unwrap()).unwrap();
    let expected = std::fs::read_to_string(fixture("line-breaks.word-lines.txt")).unwrap();
    let mut words: Vec<(String, Vec<String>)> = Vec::new();
    for line in expected.lines() {
        if let Some(font) = line.strip_prefix("== ") {
            words.push((font.trim().to_owned(), Vec::new()));
        } else if let Some((_, lines)) = words.last_mut() {
            lines.push(line.trim().to_owned());
        }
    }
    let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
    for ((path, paragraph), (font, word_lines)) in
        unluminous_word::position::paragraphs(&document.body).into_iter().zip(&words)
    {
        if font == "Aptos" {
            continue;
        }
        let text = paragraph.flat_text();
        let ours: Vec<String> = pages
            .pages
            .iter()
            .flat_map(|p| p.carets.iter())
            .filter(|l| l.path == path)
            .map(|l| text[l.start..l.end.min(text.len())].trim().to_owned())
            .collect();
        assert_eq!(&ours, word_lines, "{font}");
    }
}

fn word_with(name: &str) -> (Word, String) {
    let mut word = Word::new();
    word.open(&crate::services::plugin_ui::Context::default()).unwrap();
    let path = fixture(name);
    let key = tab_key(&path);
    word.command("load", &[key.clone(), path.display().to_string()]).unwrap();
    (word, key)
}

fn run(word: &mut Word, line: &str) -> crate::services::plugin_ui::Answer {
    let words: Vec<String> = line.split(' ').map(str::to_owned).collect();
    word.command(&words[0], &words[1..]).unwrap_or_else(|e| panic!("{line}: {e}"))
}

#[test]
fn an_agent_reads_a_document_as_paragraphs() {
    let (mut word, _) = word_with("styles.docx");
    let answer = run(&mut word, "read 1 3");
    let paragraphs = answer.value.as_array().unwrap();
    assert_eq!(paragraphs.len(), 3);
    assert_eq!(paragraphs[0]["style"], "Title");
    let outline = run(&mut word, "outline");
    assert!(outline.value.as_array().unwrap().iter().any(|h| h["level"] == 1));
}

#[test]
fn an_agent_types_formats_and_undoes() {
    let (mut word, key) = word_with("blank.docx");
    run(&mut word, "type Hello agent\\nSecond paragraph");
    run(&mut word, "select 1 0 1 5");
    run(&mut word, "format bold on");
    run(&mut word, "paragraph align center");
    run(&mut word, "style Heading 1");
    let read = run(&mut word, "read --runs");
    let first = &read.value[0];
    assert_eq!(first["text"], "Hello agent");
    assert_eq!(first["style"], "Heading 1");
    assert_eq!(first["align"], "center");
    assert_eq!(first["runs"][0]["bold"], true);
    assert_eq!(read.value[1]["text"], "Second paragraph");
    assert!(word.modified(&key));
    run(&mut word, "undo");
    let read = run(&mut word, "read");
    assert_eq!(read.value[0]["style"], "Normal");
}

#[test]
fn an_agent_builds_lists_tables_comments_and_page_settings_and_saves() {
    let (mut word, key) = word_with("blank.docx");
    run(&mut word, "type one\\ntwo\\nthree");
    run(&mut word, "select all");
    run(&mut word, "list number");
    let read = run(&mut word, "read");
    assert_eq!(read.value[0]["list"]["label"], "1.");
    assert_eq!(read.value[2]["list"]["label"], "3.");
    run(&mut word, "select 3 5");
    run(&mut word, "insert table 2 2");
    run(&mut word, "type cell");
    run(&mut word, "table row-below");
    let read = run(&mut word, "read");
    assert!(read
        .value
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["text"] == "cell" && p["cell"]["row"] == 1));
    run(&mut word, "select 1");
    let comment = run(&mut word, "comment add Check this");
    assert_eq!(comment.value["id"], 0);
    run(&mut word, "section orientation landscape");
    run(&mut word, "section footer Page {PAGE} of {NUMPAGES}");
    run(&mut word, "track on");
    run(&mut word, "select text two");
    run(&mut word, "type 2");
    let changes = run(&mut word, "track list");
    assert!(changes.value["changes"].as_array().unwrap().len() >= 2);
    let out = std::env::temp_dir().join("unluminous-word-agent.docx");
    run(&mut word, &format!("save {}", out.display()));
    assert!(!word.modified(&key));
    let again = unluminous_word::read::open(&std::fs::read(&out).unwrap()).unwrap();
    assert!(unluminous_word::layout::section::Geometry::of(&again.section).landscape());
    assert_eq!(again.comments.as_ref().map(|c| c.notes.len()), Some(1));
    for kind in ["pdf", "html", "md", "txt"] {
        let path = out.with_extension(kind);
        run(&mut word, &format!("export {kind} {}", path.display()));
        assert!(std::fs::metadata(&path).unwrap().len() > 20, "{kind}");
    }
}

#[test]
fn every_action_runs_on_a_document() {
    let (mut word, _) = word_with("kitchen-sink.docx");
    for action in actions::ACTIONS {
        // These two ask the operating system for a file or write one.
        if matches!(action.id, "picture") || action.id.starts_with("export") {
            continue;
        }
        let doc = word.current_doc().unwrap();
        doc.ensure_pages();
        let _ = (action.run)(doc);
    }
    let doc = word.current_doc().unwrap();
    assert!(unluminous_word::write::save(&doc.session.document).is_ok());
}

#[test]
fn a_refusal_names_the_verbs_there_are() {
    let (mut word, _) = word_with("blank.docx");
    let refused = word.command("frobnicate", &[]).unwrap_err();
    assert!(refused.contains("read") && refused.contains("format"), "{refused}");
}

#[test]
fn the_markdown_habits_make_headings_and_lists() {
    let (mut word, _) = word_with("blank.docx");
    let doc = word.current_doc().unwrap();
    for c in "# Title".chars() {
        crate::components::word::keys::type_for_test(doc, &c.to_string());
    }
    let read = unluminous_word::export::paragraphs_value(&doc.session.document, 1, 1, false);
    assert_eq!(read[0]["style"], "Heading 1");
    assert_eq!(read[0]["text"], "Title");
}

/// Office's cloud fonts are added when a document opens, so a document in Aptos is measured in Aptos
/// rather than in whatever face the window falls back to.
#[test]
fn office_fonts_are_found_once_they_are_added() {
    let folders = super::fonts::office_font_folders();
    if folders.iter().all(|f| !f.join("Aptos").is_dir()) {
        println!("Office's Aptos is not on this machine, so this test checked nothing");
        return;
    }
    let renderer = TextRenderer::new();
    let before = WordFonts::new(&renderer).advance("w", &style("Aptos", 12.0));
    for folder in &folders {
        renderer.add_font_folder(folder);
    }
    let fonts = WordFonts::new(&renderer);
    let after = fonts.advance("w", &style("Aptos", 12.0));
    println!("Aptos w before {before} after {after}");
    let file = std::fs::read_dir(folders[0].join("Aptos")).unwrap().next().unwrap().unwrap().path();
    let bytes = std::fs::read(file).unwrap();
    let face = ttf_parser::Face::parse(&bytes, 0).unwrap();
    let w = face.glyph_index('w').unwrap();
    let expected =
        f32::from(face.glyph_hor_advance(w).unwrap()) / f32::from(face.units_per_em()) * 12.0;
    assert!((after - expected).abs() < 0.02, "Aptos w is {after}, the font says {expected}");
}

/// The long fixture with the window's fonts has as many pages as Word gave it.
#[test]
fn the_long_fixture_has_words_page_count_with_the_windows_fonts() {
    let folders = super::fonts::office_font_folders();
    if folders.iter().all(|f| !f.join("Aptos").is_dir()) {
        println!("Office's Aptos is not on this machine, so this test checked nothing");
        return;
    }
    let renderer = TextRenderer::new();
    for folder in &folders {
        renderer.add_font_folder(folder);
    }
    let fonts = WordFonts::new(&renderer);
    let document =
        unluminous_word::read::open(&std::fs::read(fixture("long.docx")).unwrap()).unwrap();
    let pages = unluminous_word::layout::lay_out(&document, &fonts, Default::default());
    let report: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(fixture("long.word.json")).unwrap().trim_start_matches('\u{feff}'),
    )
    .unwrap();
    assert_eq!(pages.pages.len() as u64, report["pages"].as_u64().unwrap());
    // The interface zoomed to 125%: the page and its letters grow together, so the pages do not change.
    renderer.follow_the_interface(1.25);
    let zoomed =
        unluminous_word::layout::lay_out(&document, &WordFonts::new(&renderer), Default::default());
    assert_eq!(zoomed.pages.len(), pages.pages.len(), "the interface zoom changed the pages");
}

/// **Copying inside a document and pasting keeps the formatting**: a bold word pasted into a new paragraph
/// is still bold, and a heading's style comes with its paragraph.
#[test]
fn a_formatted_copy_pastes_with_its_formatting() {
    let (mut word, _) = word_with("blank.docx");
    run(&mut word, "type Bold words here\\nA heading\\n");
    run(&mut word, "select 1 0 1 4");
    run(&mut word, "format bold on");
    run(&mut word, "select 2 0 2 9");
    run(&mut word, "style Heading 2");
    let doc = word.current_doc().unwrap();
    use unluminous_word::position::Pos;
    doc.session.select(Pos::new(vec![0], 0), Pos::new(vec![1], 9));
    let (text, fragment) = doc.session.copy();
    assert_eq!(text, "Bold words here\nA heading");
    // The empty third paragraph the typing ended in.
    doc.session.select(Pos::new(vec![2], 0), Pos::new(vec![2], 0));
    doc.session.paste_fragment(fragment);
    let read = unluminous_word::export::paragraphs_value(&doc.session.document, 1, 10, true);
    let texts: Vec<&str> = read.iter().map(|p| p["text"].as_str().unwrap_or("")).collect();
    assert_eq!(texts, ["Bold words here", "A heading", "Bold words here", "A heading"]);
    assert_eq!(read[2]["runs"][0]["text"], "Bold");
    assert_eq!(read[2]["runs"][0]["bold"], true);
    assert_eq!(read[3]["style"], "Heading 2");
}

/// **A double press on a page's top margin moves into its header, and a press on the body leaves it**, as
/// Word does. A single press on the margin stays in the body.
#[test]
fn a_double_press_on_the_margin_edits_the_header() {
    use crate::components::word::page_view::clickable;
    use unluminous_word::position::{Pos, StoryId};
    let (mut word, _) = word_with("headers.docx");
    let renderer = TextRenderer::new();
    let doc = word.current_doc().unwrap();
    doc.pages = unluminous_word::layout::lay_out(
        &doc.session.document,
        &WordFonts::new(&renderer),
        Default::default(),
    );
    let top = doc.pages.pages[0].body.y / 2.0;
    assert_eq!(clickable(doc, 0, top, false), [StoryId::Body]);
    let stories = clickable(doc, 0, top, true);
    assert!(
        !stories.is_empty() && stories.iter().all(|s| matches!(s, StoryId::Header(_))),
        "{stories:?}"
    );
    let header = stories[0].clone();
    doc.session.select(
        Pos { story: header.clone(), path: vec![0], offset: 0 },
        Pos { story: header, path: vec![0], offset: 0 },
    );
    let middle = doc.pages.pages[0].body.y + 100.0;
    assert_eq!(clickable(doc, 0, middle, false), [StoryId::Body]);
}

/// **An agent lists the spelling errors with suggestions**, steps to the next one as F7 does, and a
/// suggestion replaces the word.
#[test]
fn an_agent_lists_spelling_errors_and_a_suggestion_replaces_one() {
    let (mut word, _) = word_with("blank.docx");
    run(&mut word, "type A sentance with an an error.");
    let found = run(&mut word, "spelling").value;
    let found = found.as_array().unwrap();
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0]["word"], "sentance");
    assert_eq!(found[0]["kind"], "spelling");
    assert!(found[0]["suggestions"].as_array().unwrap().iter().any(|s| s == "sentence"));
    assert_eq!(found[1]["word"], "an");
    assert_eq!(found[1]["kind"], "repeated");
    run(&mut word, "select 1 0");
    assert_eq!(run(&mut word, "spelling next").value["text"], "sentance");
    let doc = word.current_doc().unwrap();
    assert!(doc.view.spell_menu.is_some(), "F7 opens the card");
    super::spelling::replace(doc, "sentence");
    super::spelling::next(doc).unwrap();
    super::spelling::delete_repeated(doc);
    let read = unluminous_word::export::paragraphs_value(&doc.session.document, 1, 1, false);
    assert_eq!(read[0]["text"], "A sentence with an error.");
    assert_eq!(run(&mut word, "spelling").value.as_array().unwrap().len(), 0);
}

/// **A suggestion takes the formatting of the word it replaces**, which in Word is the formatting of its
/// first character. Found on the installed 1.14.1: in the kitchen sink document "su" of "summarises" is
/// italic and the word before it is bold, and the suggestion "summarizes" came out bold.
#[test]
fn a_suggestion_keeps_the_formatting_of_the_word_it_replaces() {
    use unluminous_word::position::Pos;
    let (mut word, _) = word_with("kitchen-sink.docx");
    let doc = word.current_doc().unwrap();
    doc.session.select(Pos::new(vec![8], 14), Pos::new(vec![8], 14));
    let before = doc.session.look_at_caret();
    crate::components::word::spelling::right_click(doc, Pos::new(vec![8], 14));
    let menu = doc.view.spell_menu.clone().expect("the card is open");
    assert_eq!(menu.word.word, "summarises");
    super::spelling::replace(doc, "summarizes");
    let story = doc.session.selection.head.story.clone();
    let text = unluminous_word::position::paragraph(&doc.session.document, &story, &[8])
        .unwrap()
        .flat_text();
    assert!(text.starts_with("This report summarizes the"), "{text}");
    let after = doc.session.look_at_caret();
    assert_eq!((after.bold, after.italic), (before.bold, before.italic));
    assert!(!after.bold, "the word is not bold");
}

/// **A right click on an underlined word selects it and opens its card**, and a right click elsewhere
/// moves the caret there.
#[test]
fn a_right_click_on_a_misspelled_word_opens_its_card() {
    use unluminous_word::position::Pos;
    let (mut word, _) = word_with("blank.docx");
    run(&mut word, "type Plain wrods here");
    let doc = word.current_doc().unwrap();
    crate::components::word::spelling::right_click(doc, Pos::new(vec![0], 8));
    let menu = doc.view.spell_menu.clone().expect("the card is open");
    assert_eq!(menu.word.word, "wrods");
    assert!(menu.suggestions.iter().any(|s| s == "words"), "{:?}", menu.suggestions);
    assert_eq!(doc.session.selected_text(), "wrods");
    crate::components::word::spelling::right_click(doc, Pos::new(vec![0], 2));
    assert!(doc.view.spell_menu.is_none());
    assert!(doc.session.selection.is_empty());
    assert_eq!(doc.session.selection.head.offset, 2);
}
