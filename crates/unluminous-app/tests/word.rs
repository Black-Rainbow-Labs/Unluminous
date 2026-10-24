//! Word documents in a real window: a `.docx` drawn as pages, typed into with the keyboard and formatted
//! with Word's chords, the format bar, the drill down panel, the `/` menu and the command search, saved,
//! and switched off. `task-2341`.
//!
//! The documents are the fixtures `unluminous-word` keeps, which Microsoft Word itself made through
//! `tools/word-oracle/make-fixtures.ps1`.

mod common;

use common::*;

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::UnluminousApp;

/// A folder with copies of some of Word's fixtures in it.
fn a_word_folder(name: &str, documents: &[&str]) -> std::path::PathBuf {
    let folder = fixture(name, &[("readme.md", "# A project with Word documents\n")]);
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../unluminous-word/tests/fixtures");
    for document in documents {
        std::fs::copy(source.join(document), folder.join(document)).expect("copy a fixture");
    }
    folder
}

/// A window with `document` open in its Word tab.
fn a_word_window(
    name: &str,
    document: &str,
) -> (std::path::PathBuf, Harness<'static, UnluminousApp>) {
    let folder = a_word_folder(name, &[document]);
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("tab open {document}"));
    steady(&mut harness);
    (folder, harness)
}

fn view(harness: &mut Harness<'static, UnluminousApp>) -> serde_json::Value {
    did(harness, "plugins view word")
}

fn paragraphs(harness: &mut Harness<'static, UnluminousApp>) -> Vec<serde_json::Value> {
    did(harness, "plugins run word read --runs").as_array().cloned().unwrap_or_default()
}

fn type_text(harness: &mut Harness<'static, UnluminousApp>, text: &str) {
    for character in text.chars() {
        harness.input_mut().events.push(egui::Event::Text(character.to_string()));
        harness.step();
    }
    steady(harness);
}

fn chord(harness: &mut Harness<'static, UnluminousApp>, key: Key) {
    harness.key_press_modifiers(Modifiers::COMMAND, key);
    steady(harness);
}

#[test]
fn a_document_word_made_opens_as_its_pages() {
    let (_, mut harness) = a_word_window("unluminous-word-pages", "kitchen-sink.docx");
    let current = view(&mut harness)["current"].clone();
    assert_eq!(current["pages"], 2, "Word made it two pages");
    assert_eq!(current["laidOutWith"], "window");
    let first = &paragraphs(&mut harness)[0];
    assert_eq!(first["style"], "Title");
    assert_eq!(first["text"], "Quarterly Report");
    harness.snapshot(shot("word_kitchen_sink"));
}

#[test]
fn typing_formats_with_words_chords_and_undoes() {
    let (_, mut harness) = a_word_window("unluminous-word-typing", "blank.docx");
    assert_eq!(view(&mut harness)["current"]["keyboard"], true, "the new tab holds the keyboard");
    type_text(&mut harness, "# A heading ");
    harness.key_press(Key::Enter);
    steady(&mut harness);
    type_text(&mut harness, "plain ");
    chord(&mut harness, Key::B);
    type_text(&mut harness, "bold");
    chord(&mut harness, Key::B);
    type_text(&mut harness, " plain");
    let read = paragraphs(&mut harness);
    assert_eq!(read[0]["style"], "Heading 1", "# and a space make a heading");
    assert_eq!(read[0]["text"], "A heading");
    assert_eq!(read[1]["text"], "plain bold plain");
    let bold: Vec<&str> = read[1]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["bold"] == true)
        .map(|r| r["text"].as_str().unwrap())
        .collect();
    assert_eq!(bold, ["bold"]);
    chord(&mut harness, Key::Z);
    assert_eq!(
        paragraphs(&mut harness)[1]["text"],
        "plain bold",
        "Ctrl+Z takes the last word back"
    );
    chord(&mut harness, Key::Y);
    assert_eq!(paragraphs(&mut harness)[1]["text"], "plain bold plain");
    harness.snapshot(shot("word_typed"));
}

#[test]
fn the_arrows_and_shift_select_and_backspace_deletes_the_selection() {
    let (_, mut harness) = a_word_window("unluminous-word-arrows", "blank.docx");
    type_text(&mut harness, "one two three");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::ArrowLeft);
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins run word selection")["text"], "three");
    harness.key_press(Key::Backspace);
    steady(&mut harness);
    harness.key_press(Key::Home);
    steady(&mut harness);
    type_text(&mut harness, ">");
    assert_eq!(paragraphs(&mut harness)[0]["text"], ">one two ");
}

#[test]
fn the_format_panel_drills_down_and_its_controls_change_the_document() {
    let (_, mut harness) = a_word_window("unluminous-word-panel", "styles.docx");
    harness.get_by_label("Format: text, paragraph, styles, page and more").click();
    steady(&mut harness);
    harness.snapshot(shot("word_panel_root"));
    harness.get_by_label("Page").click();
    steady(&mut harness);
    assert_eq!(view(&mut harness)["current"]["panel"], serde_json::json!(["Format", "Page"]));
    harness.snapshot(shot("word_panel_page"));
    harness.get_by_label("Landscape").click();
    steady(&mut harness);
    let pages = did(&mut harness, "plugins run word pages");
    assert!(
        pages[0]["width"].as_f64().unwrap() > pages[0]["height"].as_f64().unwrap(),
        "the page turned"
    );
    did(&mut harness, "plugins run word panel text");
    steady(&mut harness);
    harness.snapshot(shot("word_panel_text"));
}

#[test]
fn the_slash_menu_and_the_command_search_insert_by_name() {
    let (_, mut harness) = a_word_window("unluminous-word-slash", "blank.docx");
    type_text(&mut harness, "/tab");
    assert_eq!(view(&mut harness)["current"]["slash"], "tab");
    harness.snapshot(shot("word_slash"));
    harness.key_press(Key::Enter);
    steady(&mut harness);
    let read = paragraphs(&mut harness);
    assert!(read.iter().any(|p| p["cell"]["row"] == 1), "the / menu inserted a table: {read:?}");
    assert!(
        read.iter().all(|p| !p["text"].as_str().unwrap_or("").contains("/tab")),
        "and took what was typed away"
    );
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Slash);
    steady(&mut harness);
    type_text(&mut harness, "landsc");
    harness.snapshot(shot("word_command_search"));
    harness.key_press(Key::Enter);
    steady(&mut harness);
    let pages = did(&mut harness, "plugins run word pages");
    assert!(pages[0]["width"].as_f64().unwrap() > pages[0]["height"].as_f64().unwrap());
}

#[test]
fn a_document_is_saved_with_ctrl_s_and_on_closing_and_reads_back() {
    let (folder, mut harness) = a_word_window("unluminous-word-save", "blank.docx");
    type_text(&mut harness, "Saved by Unluminous");
    let tab = view(&mut harness)["current"]["key"].as_str().unwrap().to_owned();
    assert!(view(&mut harness)["docs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["key"] == tab.as_str() && d["modified"] == true));
    did(&mut harness, "action run save");
    let again =
        unluminous_word::read::open(&std::fs::read(folder.join("blank.docx")).unwrap()).unwrap();
    let text = unluminous_word::export::to_text(&again);
    assert!(text.starts_with("Saved by Unluminous"), "{text}");
    type_text(&mut harness, " again");
    did(&mut harness, "tab close");
    let closed =
        unluminous_word::read::open(&std::fs::read(folder.join("blank.docx")).unwrap()).unwrap();
    assert!(
        unluminous_word::export::to_text(&closed).starts_with("Saved by Unluminous again"),
        "closing saved it"
    );
    assert!(
        view(&mut harness)["docs"].as_array().unwrap().is_empty(),
        "the closed document is let go"
    );
}

#[test]
fn switched_off_a_docx_is_a_file_that_cannot_be_shown() {
    let folder = a_word_folder("unluminous-word-off", &["blank.docx"]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "plugins disable word");
    did(&mut harness, "tab open blank.docx");
    steady(&mut harness);
    assert!(
        harness.state().files.active().unrenderable.is_some(),
        "a .docx with the plugin off is not drawn"
    );
    did(&mut harness, "plugins enable word");
}
