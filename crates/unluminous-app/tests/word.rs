//! Word documents in a real window: a `.docx` drawn as pages, typed into with the keyboard and formatted
//! with Word's chords, the format bar, the drill down panel, the `/` menu and the command search, saved,
//! and switched off. `task-2341`.
//!
//! The documents are the fixtures `unluminous-word` keeps, which Microsoft Word itself made through
//! `tools/word-oracle/make-fixtures.ps1`.

mod common;

use common::*;

use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
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
    type_text(&mut harness, "# A heading");
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
    // Autocorrect capitalizes the first word of a sentence, as Word does.
    assert_eq!(read[1]["text"], "Plain bold plain");
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
        "Plain bold",
        "Ctrl+Z takes the last word back"
    );
    chord(&mut harness, Key::Y);
    assert_eq!(paragraphs(&mut harness)[1]["text"], "Plain bold plain");
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
    assert_eq!(
        paragraphs(&mut harness)[0]["text"],
        ">One two ",
        "autocorrect capitalized the sentence"
    );
}

/// **The bubble over a selection takes a click.** The page under the bubble used to take the press as
/// well, move the caret and close the bubble before its button saw the release.
#[test]
fn the_bubble_over_a_selection_formats_it() {
    let (_, mut harness) = a_word_window("unluminous-word-bubble", "blank.docx");
    type_text(&mut harness, "one two three");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::ArrowLeft);
    steady(&mut harness);
    harness.get_by_label("Bold").click();
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins run word selection")["text"], "three");
    harness.get_by_label("Italic").click();
    steady(&mut harness);
    let runs = paragraphs(&mut harness)[0]["runs"].as_array().cloned().unwrap();
    let last = runs.last().unwrap();
    assert_eq!(last["text"], "three");
    assert_eq!(last["bold"], true);
    assert_eq!(last["italic"], true);
}

/// **F7 opens the card for the next misspelled word, and a click on a suggestion replaces it.** The
/// status line counts the errors, and the count goes when the word is fixed.
#[test]
fn f7_opens_a_misspelled_words_card_and_a_suggestion_replaces_it() {
    let (_, mut harness) = a_word_window("unluminous-word-spelling", "blank.docx");
    type_text(&mut harness, "A sentance to check.");
    harness.get_by_label("Next spelling error (F7)");
    harness.key_press(Key::Home);
    harness.key_press(Key::F7);
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins run word selection")["text"], "sentance");
    harness.snapshot(shot("word_spelling"));
    harness.get_by_label("sentence").click();
    steady(&mut harness);
    assert_eq!(paragraphs(&mut harness)[0]["text"], "A sentence to check.");
    assert!(
        harness.query_by_label("Next spelling error (F7)").is_none(),
        "no errors are left to count"
    );
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

// ---- task-2359: the window's round 2 controls ----

fn point(value: &serde_json::Value) -> egui::Pos2 {
    egui::pos2(value["x"].as_f64().unwrap() as f32, value["y"].as_f64().unwrap() as f32)
}

fn press_at(
    harness: &mut Harness<'static, UnluminousApp>,
    at: egui::Pos2,
    button: egui::PointerButton,
) {
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    steady(harness);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::default(),
        });
        steady(harness);
    }
}

fn current(harness: &mut Harness<'static, UnluminousApp>) -> serde_json::Value {
    view(harness)["current"].clone()
}

fn menu(harness: &mut Harness<'static, UnluminousApp>) -> Vec<String> {
    current(harness)["contextMenu"]
        .as_array()
        .map(|a| a.iter().filter_map(|l| l.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn rect_of(value: &serde_json::Value) -> Vec<f32> {
    value.as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect()
}

/// **A right click opens the menu for what is under the pointer**: text and a table cell, each with
/// Word's own entries, and an entry runs what it names.
#[test]
fn the_right_click_menu_offers_what_is_under_the_pointer() {
    let (_, mut harness) = a_word_window("unluminous-word-context", "blank.docx");
    type_text(&mut harness, "Quick brown fox");
    let at = point(&did(&mut harness, "plugins run word locate 1 3"));
    press_at(&mut harness, at, egui::PointerButton::Secondary);
    let entries = menu(&mut harness);
    for wanted in ["Cut", "Copy", "Paste", "Font...", "Paragraph...", "New Comment"] {
        assert!(entries.iter().any(|e| e == wanted), "{wanted} in {entries:?}");
    }
    assert!(entries.iter().any(|e| e == "Synonyms"), "the word's synonyms: {entries:?}");
    harness.snapshot(shot("word_context_text"));
    harness.key_press(Key::Escape);
    steady(&mut harness);
    assert!(current(&mut harness)["contextMenu"].is_null());
    did(&mut harness, "plugins run word insert table 2 2");
    steady(&mut harness);
    let cell = paragraphs(&mut harness)
        .iter()
        .position(|p| p["cell"]["row"] == 1 && p["cell"]["column"] == 1)
        .unwrap()
        + 1;
    let at = point(&did(&mut harness, &format!("plugins run word locate {cell} 0")));
    press_at(&mut harness, at, egui::PointerButton::Secondary);
    let entries = menu(&mut harness);
    assert!(entries.iter().any(|e| e == "Rows Above"), "{entries:?}");
    // The four places a row or column goes open to the side of one row, as Word's menu has them.
    harness.get_by_label("Insert Rows and Columns").click();
    steady(&mut harness);
    harness.get_by_label("Rows Below").click();
    steady(&mut harness);
    assert!(paragraphs(&mut harness).iter().any(|p| p["cell"]["row"] == 3), "a row was added");
}

/// **A picture is selected, resized by a corner handle and wrapped from its menu.**
#[test]
fn a_picture_is_resized_by_its_handle_and_wrapped_from_its_menu() {
    // The picture is written before the window opens, so the folder pane lists it in every run rather
    // than whenever the folder watch next looks.
    let folder = a_word_folder("unluminous-word-picture", &["blank.docx"]);
    write_sample_picture(&folder.join("picture.png"));
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open blank.docx");
    steady(&mut harness);
    type_text(&mut harness, "Before after");
    did(&mut harness, "plugins run word select 1 7");
    did(
        &mut harness,
        &format!("plugins run word insert picture {} 120", folder.join("picture.png").display()),
    );
    did(&mut harness, "plugins run word picture select 1 7");
    steady(&mut harness);
    let object = current(&mut harness)["object"].clone();
    assert_eq!(object["kind"], "Picture");
    let width = object["width"].as_f64().unwrap();
    let screen = rect_of(&object["screen"]);
    let corner = egui::pos2(screen[0] + screen[2], screen[1] + screen[3]);
    harness.snapshot(shot("word_picture_selected"));
    drag(&mut harness, corner, corner + egui::vec2(60.0, 0.0));
    let after = current(&mut harness)["object"].clone();
    assert!(
        after["width"].as_f64().unwrap() > width + 20.0,
        "the corner made it wider: {width} then {}",
        after["width"]
    );
    let ratio =
        |o: &serde_json::Value| o["width"].as_f64().unwrap() / o["height"].as_f64().unwrap();
    assert!((ratio(&after) - ratio(&object)).abs() < 0.02, "a corner keeps the ratio");
    let screen = rect_of(&after["screen"]);
    let middle = egui::pos2(screen[0] + screen[2] / 2.0, screen[1] + screen[3] / 2.0);
    press_at(&mut harness, middle, egui::PointerButton::Secondary);
    assert!(menu(&mut harness).iter().any(|e| e == "Square"));
    harness.get_by_label("Wrap Text").click();
    steady(&mut harness);
    harness.get_by_label("Square").click();
    steady(&mut harness);
    assert_eq!(current(&mut harness)["object"]["wrap"], "square");
}

/// **The ruler's hanging indent box drags the left indent, and a press on the ruler adds a tab stop.**
#[test]
fn the_ruler_drags_an_indent_and_adds_a_tab_stop() {
    let (_, mut harness) = a_word_window("unluminous-word-ruler", "blank.docx");
    type_text(&mut harness, "A paragraph long enough to show its indents on the ruler");
    did(&mut harness, "plugins run word view ruler on");
    steady(&mut harness);
    let marks = rect_of(&current(&mut harness)["rulerMarks"]);
    let (hang, bottom) = (marks[1], marks[4]);
    let zoom = current(&mut harness)["zoom"].as_f64().unwrap() as f32;
    harness.snapshot(shot("word_ruler"));
    drag(
        &mut harness,
        egui::pos2(hang, bottom - 1.0),
        egui::pos2(hang + 36.0 * zoom, bottom - 1.0),
    );
    let look = did(&mut harness, "plugins run word paragraph info");
    assert!((look["indentLeft"].as_f64().unwrap() - 36.0).abs() < 5.0, "{look}");
    press_at(
        &mut harness,
        egui::pos2(hang + 200.0 * zoom, bottom - 4.0),
        egui::PointerButton::Primary,
    );
    let look = did(&mut harness, "plugins run word paragraph info");
    assert_eq!(look["tabs"].as_array().unwrap().len(), 1, "{look}");
}

/// **Ctrl+Alt+F makes a footnote and the caret goes into it to type.**
#[test]
fn a_footnote_is_made_with_its_chord_and_typed_into() {
    let (_, mut harness) = a_word_window("unluminous-word-footnote", "blank.docx");
    type_text(&mut harness, "A claim");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::F);
    steady(&mut harness);
    type_text(&mut harness, "the source");
    let note = did(&mut harness, "plugins run word read --story footnote 1");
    let text = note[0]["text"].as_str().unwrap();
    assert!(text.ends_with("the source"), "{text}");
}

/// **Ctrl+Alt+1 is Heading 1, not single spacing.**
#[test]
fn ctrl_alt_1_is_heading_1() {
    let (_, mut harness) = a_word_window("unluminous-word-headings", "blank.docx");
    type_text(&mut harness, "A title");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::ALT, Key::Num1);
    steady(&mut harness);
    assert_eq!(paragraphs(&mut harness)[0]["style"], "Heading 1");
}

/// **A table column's edge is dragged to make the column wider.**
#[test]
fn a_table_column_edge_is_dragged() {
    let (_, mut harness) = a_word_window("unluminous-word-columns", "blank.docx");
    did(&mut harness, "plugins run word insert table 1 2");
    steady(&mut harness);
    let second =
        paragraphs(&mut harness).iter().position(|p| p["cell"]["column"] == 2).unwrap() + 1;
    let before = point(&did(&mut harness, &format!("plugins run word locate {second} 0")));
    let zoom = current(&mut harness)["zoom"].as_f64().unwrap() as f32;
    let edge = egui::pos2(before.x - 5.4 * zoom, before.y);
    drag(&mut harness, edge, edge + egui::vec2(50.0, 0.0));
    let after = point(&did(&mut harness, &format!("plugins run word locate {second} 0")));
    assert!(
        after.x > before.x + 30.0,
        "the second column starts further right: {before:?} then {after:?}"
    );
}

/// Open the View chip in the format bar and choose `name` from its list.
fn choose_view(harness: &mut Harness<'static, UnluminousApp>, name: &str) {
    harness.get_by_label("View mode").click();
    steady(harness);
    harness.get_by_label(name).click();
    steady(harness);
}

/// **Bold, italic and the rest are in the Text options flyout, which opens and closes from its chip in the
/// format bar, and its Bold sets bold at the caret.** `task-2366`.
#[test]
fn the_text_options_flyout_holds_bold_and_the_lists() {
    let (_, mut harness) = a_word_window("unluminous-word-text-options", "blank.docx");
    type_text(&mut harness, "plain ");
    assert!(harness.query_by_label("Bold").is_none(), "bold is behind the chip");
    harness.get_by_label("Text options").click();
    steady(&mut harness);
    harness.get_by_label("Numbered list");
    harness.get_by_label("Justify");
    harness.snapshot(shot("word_text_options"));
    harness.get_by_label("Bold").click();
    steady(&mut harness);
    type_text(&mut harness, "strong");
    let runs = paragraphs(&mut harness)[0]["runs"].as_array().cloned().unwrap();
    let last = runs.last().unwrap();
    assert_eq!(last["text"], "strong");
    assert_eq!(last["bold"], true);
}

/// **The paragraph style chip lists the styles and sets the one chosen.** `task-2366`.
#[test]
fn the_style_chip_lists_the_styles_and_sets_one() {
    let (_, mut harness) = a_word_window("unluminous-word-style-chip", "blank.docx");
    type_text(&mut harness, "A heading");
    harness.get_by_label("Paragraph style").click();
    steady(&mut harness);
    harness.snapshot(shot("word_style_list"));
    harness.get_by_label("Heading 2").click();
    steady(&mut harness);
    assert_eq!(paragraphs(&mut harness)[0]["style"], "Heading 2");
}

/// **The page, the words and the spelling are in the window's status bar, and a press on the page opens
/// Go To.** The Word tab draws no strip of its own under the pages. `task-2366`.
/// **The pages follow a caret an agent moved.** Typing a page break and a heading through `plugins run
/// word` scrolls the view to the second page, as typing the same thing with the keyboard does, so a
/// person watching an agent edit sees the edit. `task-2377`.
#[test]
fn the_pages_follow_the_caret_an_agent_moves() {
    let (_, mut harness) = a_word_window("unluminous-word-follow", "blank.docx");
    let before = view(&mut harness)["current"]["scroll"].as_f64().unwrap_or(0.0);
    did(&mut harness, "plugins run word type First page");
    did(&mut harness, "plugins run word insert page-break");
    did(&mut harness, "plugins run word type Second page");
    steady(&mut harness);
    let after = view(&mut harness)["current"]["scroll"].as_f64().unwrap_or(0.0);
    assert!(after > before + 100.0, "the view scrolled to the second page: {before} to {after}");
}

/// **`replace <find> -- <with> --all` replaces.** The `--` between the two halves was dropped with the
/// flags, so every replace was refused for having no `--`. `task-2377`.
#[test]
fn replace_finds_the_separator_and_replaces_every_match() {
    let (_, mut harness) = a_word_window("unluminous-word-replace", "blank.docx");
    did(&mut harness, "plugins run word type Alta Via 1 and Alta Via 2 and Alta Via 1");
    let answer = did(&mut harness, "plugins run word replace Alta Via 1 -- Alta Via 1 (AV1) --all");
    assert_eq!(answer["replaced"], 2);
    let text = paragraphs(&mut harness)[0]["text"].as_str().unwrap_or_default().to_owned();
    assert_eq!(text, "Alta Via 1 (AV1) and Alta Via 2 and Alta Via 1 (AV1)");
}

/// **`toc insert 2` and `caption Table Quarterly sales` do what their reference says.** Both read their
/// arguments one place too far along: `toc insert 2` was refused as "not 2", `toc update` inserted a
/// second table, and `caption Table Quarterly sales` made a caption labelled `Quarterly`. `task-2377`.
#[test]
fn toc_and_caption_read_the_arguments_their_reference_shows() {
    let (_, mut harness) = a_word_window("unluminous-word-toc-caption", "blank.docx");
    did(&mut harness, "plugins run word style Heading 1");
    did(&mut harness, "plugins run word type Route");
    did(&mut harness, "plugins run word caption Table Quarterly sales");
    let caption = paragraphs(&mut harness)
        .into_iter()
        .find(|p| p["style"] == "Caption")
        .expect("a caption paragraph");
    assert_eq!(caption["text"], "Table 1 Quarterly sales");
    did(&mut harness, "plugins run word select 1 0");
    did(&mut harness, "plugins run word toc insert 2 --no-links");
    did(&mut harness, "plugins run word toc update");
    let tocs = paragraphs(&mut harness).into_iter().filter(|p| p["style"] == "TOC 1").count();
    assert_eq!(tocs, 1, "one table of contents with one entry, not a second table");
}

#[test]
fn the_window_status_bar_says_the_page_and_opens_go_to() {
    let (_, mut harness) = a_word_window("unluminous-word-status", "styles.docx");
    harness.get_by_label("Status: words");
    assert!(harness.query_by_label("Status: language").is_none(), "the language is gone");
    harness.get_by_label("Status: page").click();
    steady(&mut harness);
    harness.get_by_label("Close Go To");
    harness.snapshot(shot("word_go_to"));
}

/// **Outline view does not offer a tool that would fail where the caret is.** On body text, Demote and
/// Demote to Body Text are drawn quiet and a press on them does nothing and says nothing. `task-2366`.
#[test]
fn outline_view_does_not_offer_a_tool_that_would_fail() {
    let (_, mut harness) = a_word_window("unluminous-word-outline-tools", "styles.docx");
    choose_view(&mut harness, "Outline");
    let body = paragraphs(&mut harness).iter().position(|p| p["style"] == "Normal").unwrap() + 1;
    did(&mut harness, &format!("plugins run word select {body} 0"));
    steady(&mut harness);
    for name in ["Demote", "Demote to Body Text"] {
        assert!(harness.get_by_label(name).accesskit_node().is_disabled(), "{name} is not offered");
    }
    assert!(!harness.get_by_label("Promote").accesskit_node().is_disabled(), "Promote is");
    harness.get_by_label("Demote").click();
    steady(&mut harness);
    assert_eq!(paragraphs(&mut harness)[body - 1]["style"], "Normal", "nothing changed");
    assert!(harness.query_by_label_contains("lowest level").is_none(), "and nothing was said");
}

/// **Read Mode, Outline view, Fit Width, the thumbnails and Print.**
#[test]
fn the_views_and_print_work_from_the_window() {
    let (folder, mut harness) = a_word_window("unluminous-word-views", "styles.docx");
    choose_view(&mut harness, "Read Mode");
    assert_eq!(current(&mut harness)["mode"], "read");
    harness.snapshot(shot("word_read_mode"));
    harness.key_press(Key::Escape);
    steady(&mut harness);
    assert_eq!(current(&mut harness)["mode"], "print");
    choose_view(&mut harness, "Outline");
    assert_eq!(current(&mut harness)["mode"], "outline");
    harness.snapshot(shot("word_outline"));
    let heading =
        paragraphs(&mut harness).iter().position(|p| p["style"] == "Heading 2").map(|i| i + 1);
    if let Some(heading) = heading {
        did(&mut harness, &format!("plugins run word select {heading} 0"));
        harness.get_by_label("Promote").click();
        steady(&mut harness);
        assert_eq!(
            paragraphs(&mut harness)[heading - 1]["style"],
            "Heading 1",
            "Promote made it Heading 1"
        );
    }
    did(&mut harness, "plugins run word view print");
    steady(&mut harness);
    let before = current(&mut harness)["zoom"].as_f64().unwrap();
    did(&mut harness, "plugins run word view fit-width");
    assert_ne!(
        current(&mut harness)["zoom"].as_f64().unwrap(),
        before,
        "Fit Width changed the zoom"
    );
    did(&mut harness, "plugins run word view thumbnails");
    steady(&mut harness);
    harness.get_by_label("Page 1");
    harness.snapshot(shot("word_thumbnails"));
    let pdf = folder.join("printed.pdf");
    did(&mut harness, &format!("plugins run word print 1 --to {}", pdf.display()));
    assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"), "Print wrote the sheets");
}

/// **The Modify Style level changes a style.**
#[test]
fn the_modify_style_level_changes_a_style() {
    let (_, mut harness) = a_word_window("unluminous-word-modify", "styles.docx");
    did(&mut harness, "plugins run word panel styles");
    steady(&mut harness);
    harness.get_by_label("Modify Heading 1").click();
    steady(&mut harness);
    assert_eq!(
        current(&mut harness)["panel"],
        serde_json::json!(["Format", "Styles", "Modify style"])
    );
    harness.snapshot(shot("word_modify_style"));
    harness.get_by_label("2.0").click();
    steady(&mut harness);
    let heading =
        paragraphs(&mut harness).iter().position(|p| p["style"] == "Heading 1").unwrap() + 1;
    did(&mut harness, &format!("plugins run word select {heading} 0"));
    let look = did(&mut harness, "plugins run word paragraph info");
    assert!(
        look["line"].as_str().unwrap().contains("2.0"),
        "Heading 1 is double spaced now: {look}"
    );
}

/// **A press on the left half of the first letter after a bullet puts the caret before that letter**
/// (`task-2383`). The bullet and the tab after it stood at offset 0 of the paragraph, so the caret before
/// the first word was drawn and hit at the bullet: a press on the letter went after it, typing there
/// landed one letter in, and the first letter could not be selected.
#[test]
fn a_press_on_the_first_letter_after_a_bullet_puts_the_caret_before_it() {
    let (_, mut harness) = a_word_window("unluminous-word-bullet-caret", "blank.docx");
    type_text(&mut harness, "- Apple");
    assert_eq!(paragraphs(&mut harness)[0]["text"], "Apple", "a dash and a space make a bullet");
    let after_a = point(&did(&mut harness, "plugins run word locate 1 1"));
    let after_p = point(&did(&mut harness, "plugins run word locate 1 2"));
    let letter = after_p.x - after_a.x;
    let start = point(&did(&mut harness, "plugins run word locate 1 0"));
    assert!(
        (after_a.x - start.x - letter).abs() < letter * 0.6,
        "offset 0 is drawn a letter left of offset 1, not at the bullet: {start:?} {after_a:?}"
    );
    press_at(
        &mut harness,
        egui::pos2(after_a.x - letter * 0.8, after_a.y),
        egui::PointerButton::Primary,
    );
    type_text(&mut harness, "X");
    assert_eq!(paragraphs(&mut harness)[0]["text"], "XApple", "typed before the first letter");
    harness.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins run word selection")["text"], "A");
}

/// **The Edit menu's entries reach a Word tab** (`task-2383`). On macOS `Cmd+Z`, `Cmd+A`, `Cmd+X`, `Cmd+C`
/// and `Cmd+V` are the menu bar's, so the tab never sees the keys, and the entries went to the empty
/// document behind the tab. Run here as the menu runs them, on every platform.
#[test]
fn the_edit_menu_undoes_selects_and_cuts_in_a_word_tab() {
    let (_, mut harness) = a_word_window("unluminous-word-edit-menu", "blank.docx");
    type_text(&mut harness, "one two");
    did(&mut harness, "action run undo");
    steady(&mut harness);
    assert_ne!(paragraphs(&mut harness)[0]["text"], "One two", "Edit -> Undo undid the typing");
    did(&mut harness, "action run redo");
    steady(&mut harness);
    assert_eq!(paragraphs(&mut harness)[0]["text"], "One two", "and Edit -> Redo put it back");
    did(&mut harness, "action run select-all");
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins run word selection")["text"], "One two");
    did(&mut harness, "action run cut");
    steady(&mut harness);
    assert_eq!(paragraphs(&mut harness)[0]["text"], "", "Edit -> Cut took the selection out");
}
