//! Typing, formatting, the font, the zoom, the scrollbar, and marking a passage.
//!
//! What a person does to the text in front of them: typing and backspace, bold, italic, underline,
//! strikethrough, the colours, the size, the family, the alignments and the line spacings, the
//! clipboard, undo and redo, the background's opacity, the font size the whole window shares, the
//! zoom that keeps the line being read where it was, the scrollbar, the two halves of the side by
//! side view scrolling together, and the four highlight colours behind a passage.
//!
//! **31 of the 61 tests here take a picture**, and the rest drive the window and read its state
//! back. The pictures are the ones no assertion can stand in for: that bold text is actually bolder,
//! that centred text is actually centred, and that a mark is behind the words rather than over them.

mod common;

use common::*;

use egui::{vec2, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotResults};
use unluminous_app::app::actions::{Action, HighlightColor};
use unluminous_app::app::ViewMode;
use unluminous_app::components::title_bar::MenuPlacement;
use unluminous_app::settings;
use unluminous_app::UnluminousApp;
use unluminous_core::{Align, Color, Command, StyleChange};

#[test]
fn startup_shows_the_rail_the_explorer_and_an_empty_editor() {
    let mut harness = harness("");
    // The rail of pane buttons down the far left, which `task-1658` asks for.
    for button in ["Project", "Version Control", "Terminal tile"] {
        harness.get_by_label(button);
    }
    harness.snapshot(shot("startup"));
}

#[test]
fn a_nested_folder_opens_with_its_children_indented_under_it() {
    let mut harness = harness("");
    // Click the folder in the explorer, as a person would, rather than changing the tree directly.
    harness.get_by_label_contains("chapters").click();
    harness.run();
    harness.get_by_label_contains("appendix").click();
    harness.run();
    let rows = harness.state().tree.rows().len();
    assert!(rows >= 8, "the tree should show the nested folders, it has {rows} rows");
    harness.snapshot(shot("file_tree_expanded"));
}

#[test]
fn clicking_a_file_in_the_explorer_opens_it_in_the_editor() {
    let mut harness = harness("");
    harness.get_by_label_contains("readme.md").click();
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        "# Unluminous\n",
        "clicking the file should have loaded it"
    );
    // The text has to be laid out as well as loaded. Two documents can be at the same revision, so a layout
    // cache that only compares revisions keeps the last file's lines and the editing area comes out empty.
    let lines = &harness.state().layout().lines;
    // Two lines: the heading, and the empty one after the line break at the end of the file.
    assert_eq!(lines.len(), 2, "the file should have been laid out, got {}", lines.len());
    assert!(
        lines[0].runs.iter().any(|run| !lines[0].run_clusters(run).is_empty()),
        "and the first line should hold the characters of the file"
    );
    harness.snapshot(shot("file_opened"));
}

#[test]
fn typing_on_the_keyboard_puts_text_in_the_document() {
    let mut harness = harness("");
    // Real key and text events, through the same path the released binary uses.
    for text in ["Unluminous", " typed", " this."] {
        harness.input_mut().events.push(egui::Event::Text(text.to_owned()));
        harness.run();
    }
    harness.key_press(egui::Key::Enter);
    harness.run();
    harness.input_mut().events.push(egui::Event::Text("A second line.".to_owned()));
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        "Unluminous typed this.\nA second line."
    );
    harness.snapshot(shot("typed_text"));
}

#[test]
fn backspace_removes_what_was_typed() {
    let mut harness = harness("");
    harness.input_mut().events.push(egui::Event::Text("abcdef".to_owned()));
    harness.run();
    for _ in 0..3 {
        harness.key_press(egui::Key::Backspace);
        harness.run();
    }
    assert_eq!(harness.state().document().text().to_string(), "abc");
}

// `task-1747`: a key over a selection is an indent rather than a type.

#[test]
fn tab_over_a_selection_indents_the_lines_rather_than_replacing_them() {
    // The whole of the ask, through the real window: a block of lines, a real `Tab` press, and the
    // block indented rather than gone.
    let mut harness = harness("one\ntwo\nthree\nfour");
    select_and(&mut harness, 0..18, &[]);
    harness.key_press(egui::Key::Tab);
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        "\tone\n\ttwo\n\tthree\n\tfour",
        "a tab at the start of each of the four lines, and nothing else"
    );
    assert_eq!(
        harness.state().document().selection().range(),
        1..22,
        "the selection stays over the text it covered"
    );
    harness.snapshot(shot("tab_indents_the_selection"));
}

#[test]
fn space_over_a_selection_indents_the_lines_with_a_space() {
    // A space arrives as a text event, which is the same the released binary receives, and over a
    // selection it indents by one space per line rather than replacing the block.
    let mut harness = harness("one\ntwo\nthree");
    select_and(&mut harness, 0..13, &[]);
    harness.input_mut().events.push(egui::Event::Text(" ".to_owned()));
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        " one\n two\n three",
        "a space at the start of each line, which is what the Space key says"
    );
    harness.snapshot(shot("space_indents_the_selection"));
}

#[test]
fn tab_and_space_with_no_selection_still_type_at_the_caret() {
    // The other half of the rule: with no selection the keys do exactly what they always did, so a
    // person who means to type a tab or a space is not interrupted.
    let mut harness = harness("one\ntwo");
    harness.state_mut().command(Command::PlaceCaret { offset: 4, extend: false });
    harness.run();
    harness.key_press(egui::Key::Tab);
    harness.run();
    harness.input_mut().events.push(egui::Event::Text(" ".to_owned()));
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        "one\n\t two",
        "a tab and a space typed where the caret is, as before"
    );
}

#[test]
fn a_selection_is_highlighted_behind_part_of_a_line_only() {
    let mut harness = harness("Select only the middle words of this line, not the rest of it.");
    select_and(&mut harness, 12..28, &[]);
    assert_eq!(harness.state().document().selected_text(), "the middle words");
    let rects =
        harness.state().layout().selection_rects(harness.state().document().selection().range());
    assert_eq!(rects.len(), 1, "the selection is inside one line, so it is one rectangle");
    harness.snapshot(shot("selection"));
}

#[test]
fn select_all_then_pressing_bold_makes_the_whole_document_bold() {
    let mut harness = harness("Every word here should end up bold.");
    harness.state_mut().command(Command::SelectAll);
    harness.run();
    // Click the real button rather than sending the command, which now means opening the panel it
    // moved into first.
    open_text_options(&mut harness);
    harness.get_by_label("Bold").click();
    harness.run();
    assert!(
        harness.state().document().chars().style_at(4).bold,
        "the toolbar button should have applied bold"
    );
    harness.snapshot(shot("bold_all"));
}

#[test]
fn bold_applies_to_the_middle_word_and_not_the_words_either_side() {
    let mut harness = harness("plain BOLD plain");
    select_phrase(&mut harness, "BOLD", &[Command::ToggleBold]);
    collapse(&mut harness);
    harness.snapshot(shot("bold"));
}

#[test]
fn italic_applies_to_the_middle_word_and_not_the_words_either_side() {
    let mut harness = harness("plain ITALIC plain");
    select_phrase(&mut harness, "ITALIC", &[Command::ToggleItalic]);
    collapse(&mut harness);
    harness.snapshot(shot("italic"));
}

#[test]
fn underline_draws_a_rule_under_the_middle_word_only() {
    let mut harness = harness("plain UNDERLINE plain");
    select_phrase(&mut harness, "UNDERLINE", &[Command::ToggleUnderline]);
    collapse(&mut harness);
    let rules = harness.state().layout().decorations(&harness.state().renderer);
    assert_eq!(rules.len(), 1, "one underline rule to draw");
    harness.snapshot(shot("underline"));
}

#[test]
fn strikethrough_draws_a_rule_through_the_middle_word_only() {
    let mut harness = harness("plain STRUCK plain");
    select_phrase(&mut harness, "STRUCK", &[Command::ToggleStrikethrough]);
    collapse(&mut harness);
    harness.snapshot(shot("strikethrough"));
}

#[test]
fn the_keyboard_shortcut_for_bold_does_the_same_as_the_button() {
    let mut harness = harness("shortcut bold");
    harness.state_mut().command(Command::SelectAll);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::B);
    harness.run();
    assert!(
        harness.state().document().chars().style_at(2).bold,
        "command plus B should turn bold on"
    );
}

#[test]
fn three_font_sizes_stand_at_three_visibly_different_heights() {
    let mut harness = harness("Small size here\nMedium size here\nLarge size here");
    select_phrase(&mut harness, "Small size here", &[Command::ApplyStyle(StyleChange::size(11.0))]);
    select_phrase(
        &mut harness,
        "Medium size here",
        &[Command::ApplyStyle(StyleChange::size(24.0))],
    );
    select_phrase(&mut harness, "Large size here", &[Command::ApplyStyle(StyleChange::size(44.0))]);
    collapse(&mut harness);
    let heights: Vec<f32> = harness.state().layout().lines.iter().map(|line| line.height).collect();
    assert!(
        heights[0] < heights[1] && heights[1] < heights[2],
        "each line should be taller than the one before, heights were {heights:?}"
    );
    harness.snapshot(shot("font_size"));
}

#[test]
fn four_words_are_shown_in_four_colours() {
    let mut harness = harness("white red green blue");
    select_phrase(&mut harness, "red", &[Command::ApplyStyle(StyleChange::color(Color::RED))]);
    select_phrase(&mut harness, "green", &[Command::ApplyStyle(StyleChange::color(Color::GREEN))]);
    select_phrase(&mut harness, "blue", &[Command::ApplyStyle(StyleChange::color(Color::BLUE))]);
    collapse(&mut harness);
    assert_eq!(harness.state().document().chars().style_at(7).color, Color::RED);
    harness.snapshot(shot("font_colour"));
}

#[test]
fn the_same_sentence_is_shown_in_each_installed_family() {
    let families: Vec<String> = {
        let harness = harness("");
        harness.state().renderer.families().to_vec()
    };
    assert!(!families.is_empty(), "this system has none of the offered families");
    let text: String =
        families.iter().map(|family| format!("The quick brown fox in {family}\n")).collect();
    let mut harness = harness(text.trim_end());
    for family in &families {
        let line = format!("The quick brown fox in {family}");
        select_phrase(
            &mut harness,
            &line,
            &[
                Command::ApplyStyle(StyleChange::family(family.clone())),
                Command::ApplyStyle(StyleChange::size(22.0)),
            ],
        );
    }
    collapse(&mut harness);
    harness.snapshot(shot("font_family"));
}

/// The four alignments, each in its own screenshot, so a reader can see the same paragraph placed four
/// ways rather than having to compare four paragraphs of different text.
#[test]
fn each_alignment_places_the_same_paragraph_differently() {
    let paragraph = "This paragraph is long enough to wrap onto more than one line, which is what makes the difference between the four alignments visible.";
    let mut results = SnapshotResults::new();
    for (align, name) in [
        (Align::Left, "align_left"),
        (Align::Center, "align_centre"),
        (Align::Right, "align_right"),
        (Align::Justify, "align_justify"),
    ] {
        let mut harness = harness(paragraph);
        harness.state_mut().command(Command::SelectAll);
        harness.state_mut().command(Command::SetAlign(align));
        harness.run();
        assert_eq!(harness.state().document().paragraphs().get(0).align, align);
        results.add(harness.try_snapshot(shot(name)));
    }
    report(results);
}

#[test]
fn alignment_actually_moves_the_text_within_the_width() {
    let paragraph = "A short line.";
    let left_edge = |align: Align| {
        let mut harness = harness(paragraph);
        harness.state_mut().command(Command::SelectAll);
        harness.state_mut().command(Command::SetAlign(align));
        harness.run();
        harness.state().layout().lines[0].left()
    };
    let left = left_edge(Align::Left);
    let centre = left_edge(Align::Center);
    let right = left_edge(Align::Right);
    assert!(left < centre, "centred text should start further right than left aligned text");
    assert!(centre < right, "right aligned text should start further right still");
}

#[test]
fn double_spacing_puts_the_lines_twice_as_far_apart() {
    let text =
        "First line of the paragraph.\nSecond line of the paragraph.\nThird line of the paragraph.";
    let mut results = SnapshotResults::new();

    let mut single = harness(text);
    single.state_mut().command(Command::SelectAll);
    single.run();
    let single_height = single.state().layout().height;
    results.add(single.try_snapshot(shot("line_spacing_single")));

    let mut double = harness(text);
    double.state_mut().command(Command::SelectAll);
    double.state_mut().command(Command::SetLineSpacing(2.0));
    double.run();
    let double_height = double.state().layout().height;
    assert!(
        (double_height - single_height * 2.0).abs() < 1.0,
        "double spacing should be twice as tall: {single_height} then {double_height}"
    );
    results.add(double.try_snapshot(shot("line_spacing_double")));
    report(results);
}

#[test]
fn a_long_paragraph_wraps_inside_the_editing_area() {
    let text = "Unluminous breaks a long paragraph into lines that fit the width of the editing area, \
                breaking at a space so that no word is cut in half, and it does this with its own line \
                breaking rather than with a library. This paragraph is deliberately long enough to \
                need several lines at the width of this window.";
    let mut harness = harness(text);
    let lines = harness.state().layout().lines.len();
    assert!(lines >= 3, "this paragraph should need several lines, it took {lines}");
    assert!(
        harness.state().layout().lines.iter().all(|line| line.paragraph == 0),
        "every line belongs to the one paragraph"
    );
    harness.snapshot(shot("word_wrap"));
}

#[test]
fn cut_and_paste_move_text_through_the_clipboard() {
    let mut harness = harness("first second");
    select_and(&mut harness, 0..5, &[]);
    // A cut sends the selection to the clipboard and removes it from the document.
    harness.input_mut().events.push(egui::Event::Cut);
    harness.run();
    assert_eq!(harness.state().document().text().to_string(), " second");
    // Paste it back at the end.
    harness.state_mut().command(Command::MoveDocumentEnd { extend: false });
    harness.input_mut().events.push(egui::Event::Paste("first".to_owned()));
    harness.run();
    assert_eq!(harness.state().document().text().to_string(), " secondfirst");
}

#[test]
fn copy_leaves_the_document_alone() {
    let mut harness = harness("unchanged text");
    select_and(&mut harness, 0..9, &[]);
    harness.input_mut().events.push(egui::Event::Copy);
    harness.run();
    assert_eq!(harness.state().document().text().to_string(), "unchanged text");
}

/// The transparency requirement, checked by measurement rather than by eye.
///
/// A screenshot on its own cannot prove that the text stayed opaque, so this test reads the rendered
/// pixels at two slider positions and compares them.
///
/// The comparison is made on the pixels fully covered by a glyph. A pixel at the edge of a letter is
/// only partly covered, because the rasteriser antialiases the outline, so the background legitimately
/// shows through at the edges and always will. The claim being tested is about the body of the letters:
/// however faint the background is, the ink stays solid.
///
/// The text is set in red rather than the default near white. The render target keeps the window's alpha,
/// so a screenshot of the faint setting is shown by a viewer against whatever backdrop it uses. White
/// text over a white backdrop would look as though it had faded, which is the opposite of what this test
/// exists to show. Red reads clearly against a light backdrop and a dark one.
///
/// The render target holds colours multiplied by their alpha, so a pixel fully covered by a glyph at full
/// alpha comes out as exactly the text colour with an alpha of 255. Any dimming would move those numbers.
#[test]
fn the_background_fades_with_the_slider_and_the_text_stays_opaque() {
    /// A pixel fully covered by a glyph of red text: `unluminous_core::Color::RED` at full alpha.
    const TEXT_BODY: [u8; 4] = [0xE0, 0x4A, 0x4A, 255];

    let mut results = SnapshotResults::new();
    let mut measurements = Vec::new();
    for (opacity, expected_alpha, name) in
        [(0.15_f32, 38_u8, "opacity_low"), (1.0_f32, 255, "opacity_high")]
    {
        let mut harness = harness("TEXT STAYS OPAQUE");
        harness.state_mut().command(Command::SelectAll);
        // Bold at 64 point, so that the strokes are thick and a good number of pixels reach full
        // coverage. A thin face at a small size is mostly antialiased edge, which makes the measurement
        // needlessly delicate.
        harness.state_mut().command(Command::ApplyStyle(StyleChange::size(64.0)));
        harness.state_mut().command(Command::ToggleBold);
        harness.state_mut().command(Command::ApplyStyle(StyleChange::color(Color::RED)));
        harness.state_mut().command(Command::MoveDocumentStart { extend: false });
        harness.state_mut().settings.opacity = opacity;
        harness.run();

        assert_eq!(
            harness.state().background().a(),
            expected_alpha,
            "in {name}, a slider at {opacity} should give a background alpha of {expected_alpha}"
        );

        // Only the editing area is measured for text. The toolbar, the explorer and the status bar hold
        // text too, drawn by egui in its own colours, and counting those would measure something other
        // than the document.
        let area = harness.state().editor_area();
        let image = harness.render().expect("render the window");

        // The commonest alpha in the window is the background's, because the background is most of the
        // window. This is what the operating system compositor uses to blend Unluminous over the desktop.
        let mut alpha_counts = std::collections::BTreeMap::new();
        let mut text_body = 0_usize;
        for (x, y, pixel) in image.enumerate_pixels() {
            *alpha_counts.entry(pixel.0[3]).or_insert(0_usize) += 1;
            let inside_editor = area.contains(egui::pos2(x as f32, y as f32));
            if inside_editor && pixel.0 == TEXT_BODY {
                text_body += 1;
            }
        }
        let (commonest_alpha, count) = alpha_counts
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(alpha, count)| (*alpha, *count))
            .expect("the window has pixels");
        let total = (image.width() * image.height()) as usize;

        assert_eq!(
            commonest_alpha, expected_alpha,
            "in {name}, most of the window should carry the background alpha"
        );
        assert!(
            count * 2 > total,
            "in {name}, the background should be most of the window, it was {count} of {total}"
        );
        // This is the transparency requirement itself. `TEXT_BODY` carries an alpha of 255, so at the
        // low setting these pixels are fully opaque ink sitting on a background whose alpha is 38.
        assert!(
            text_body > 500,
            "in {name}, only {text_body} pixels of fully opaque text were drawn"
        );

        measurements.push((name, expected_alpha, text_body));
        results.add(harness.try_snapshot(shot(name)));
    }

    let (low_name, low_alpha, low_text) = measurements[0];
    let (high_name, high_alpha, high_text) = measurements[1];
    assert!(
        low_alpha < high_alpha,
        "the background should be fainter at the low setting: {low_name} was {low_alpha}, {high_name} was {high_alpha}"
    );
    // The same sentence in the same font at the same size is drawn either way, so if the background fade
    // were touching the text at all, this count would move. It does not.
    assert_eq!(
        low_text, high_text,
        "the amount of solid text changed with the background: {low_text} at {low_name} against {high_text} at {high_name}"
    );
    report(results);
}

/// Undo and redo are on the keyboard and in the Edit menu. The buttons they used to have are gone,
/// because `tasks/improvements.md` asks for the keyboard alone.
#[test]
fn undo_and_redo_go_back_and_forward_through_the_history() {
    let mut harness = harness("original");
    harness.input_mut().events.push(egui::Event::Text(" plus more".to_owned()));
    harness.run();
    assert_eq!(harness.state().document().text().to_string(), " plus moreoriginal");

    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Z);
    harness.run();
    assert_eq!(harness.state().document().text().to_string(), "original", "command and Z undoes");

    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::Z);
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        " plus moreoriginal",
        "command, shift and Z redoes"
    );
}

#[test]
fn the_text_tools_no_longer_hold_the_font_the_opacity_or_undo_and_redo() {
    // They moved: the font and the background are in `Edit -> Settings`, and undo and redo are on the
    // keyboard. A button that is still there would mean the move was not finished.
    let harness = harness("some text");
    for gone in ["Undo", "Redo", "Font family", "Font size", "Background opacity"] {
        assert!(
            harness.query_by_label(gone).is_none(),
            "{gone} should not be among the text tools any more"
        );
    }
}

#[test]
fn a_document_holding_every_feature_at_once_renders() {
    // One screenshot showing the whole feature set together, which is the quickest way for a person to
    // see that nothing interferes with anything else.
    let mut harness = harness(
        "Unluminous\nA text editor written in Rust.\nbold italic underline struck\ncoloured text here\nThis last paragraph is centred and double spaced so that the paragraph settings show up next to the character settings.",
    );
    select_phrase(
        &mut harness,
        "Unluminous",
        &[
            Command::ApplyStyle(StyleChange::size(40.0)),
            Command::ToggleBold,
            Command::SetAlign(Align::Center),
        ],
    );
    select_phrase(
        &mut harness,
        "A text editor written in Rust.",
        &[Command::ApplyStyle(StyleChange::size(18.0))],
    );
    select_phrase(&mut harness, "bold", &[Command::ToggleBold]);
    select_phrase(&mut harness, "italic", &[Command::ToggleItalic]);
    select_phrase(&mut harness, "underline", &[Command::ToggleUnderline]);
    select_phrase(&mut harness, "struck", &[Command::ToggleStrikethrough]);
    select_phrase(&mut harness, "coloured", &[Command::ApplyStyle(StyleChange::color(Color::RED))]);
    select_phrase(
        &mut harness,
        "text here",
        &[Command::ApplyStyle(StyleChange::color(Color::GREEN))],
    );
    select_phrase(
        &mut harness,
        "This last paragraph",
        &[Command::SetAlign(Align::Center), Command::SetLineSpacing(2.0)],
    );
    collapse(&mut harness);
    harness.snapshot(shot("everything"));
}

#[test]
fn choosing_a_font_size_in_the_settings_sets_it_for_the_whole_document() {
    let mut harness = harness("Two lines of writing\nso that both change together");
    let undo_before = harness.state().document().can_undo();
    open_settings(&mut harness);
    harness.get_by_label("Editor font size").click();
    harness.run();
    harness.get_by_label("24").click();
    harness.run();

    assert_eq!(harness.state().settings.font_size, 24.0);
    let document = harness.state().document();
    assert_eq!(document.chars().style_at(0).size, 24.0, "the first line is at the new size");
    let end = document.text().len_bytes() - 1;
    assert_eq!(document.chars().style_at(end).size, 24.0, "and so is the last");
    assert_eq!(
        harness.state().document().text().to_string(),
        "Two lines of writing\nso that both change together",
        "and the text itself is untouched"
    );
    assert_eq!(
        harness.state().document().can_undo(),
        undo_before,
        "a font setting pushes nothing onto the undo history"
    );
}

#[test]
fn choosing_a_family_in_the_settings_leaves_bold_and_colour_alone() {
    let mut harness = harness("plain BOLD plain");
    select_phrase(&mut harness, "BOLD", &[Command::ToggleBold]);
    collapse(&mut harness);
    let families = harness.state().renderer.families().to_vec();
    let other = families.last().expect("this system has a family").clone();

    let mut settings = harness.state().settings.clone();
    settings.font_family = other.clone();
    harness.state_mut().set_settings(settings);
    harness.run();

    let style = harness.state().document().chars().style_at(7);
    assert_eq!(&*style.family, other, "the word is in the new family");
    assert!(style.bold, "and still bold");
    harness.snapshot(shot("settings_font_applied"));
}

#[test]
fn changing_the_font_reaches_every_open_tab_and_not_only_the_one_showing() {
    // `task-1657`. The editor's font is one setting for the whole window, the way the reference editor has one
    // editor font. It used to reach the active document alone, so opening three files and then
    // changing the font left two of them in the old one until Unluminous was restarted.
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    for name in ["readme.md", "notes.txt", "program.rs"] {
        harness.state_mut().open_path_permanently(&folder.join(name)).expect("the file opens");
    }
    harness.run();
    assert_eq!(harness.state().files.len(), 3, "one tab each, and none of them transient");

    let mut settings = harness.state().settings.clone();
    settings.font_size = 24.0;
    harness.state_mut().set_settings(settings);
    harness.run();

    for (index, file) in harness.state().files.iter().enumerate() {
        assert_eq!(
            file.document.chars().style_at(0).size,
            24.0,
            "tab {index} should be in the new size, whether or not it is the one showing"
        );
    }
}

#[test]
fn the_keyboard_makes_the_text_bigger_and_smaller_and_puts_it_back() {
    // The size the keys reach is the setting the dialog holds, so it survives a restart and reaches
    // every tab. They walk the sizes the dialog offers rather than a step of their own.
    let mut harness = harness("Zoom this line");
    let ctx = harness.ctx.clone();
    assert_eq!(harness.state().settings.font_size, 16.0);

    harness.state_mut().run_action(Action::ChangeFontSize { larger: true }, &ctx);
    harness.run();
    assert_eq!(harness.state().settings.font_size, 20.0, "the next size the dialog offers");
    assert_eq!(harness.state().document().chars().style_at(0).size, 20.0);
    let taller = harness.state().layout().lines[0].height;

    harness.state_mut().run_action(Action::ChangeFontSize { larger: false }, &ctx);
    harness.state_mut().run_action(Action::ChangeFontSize { larger: false }, &ctx);
    harness.run();
    assert_eq!(harness.state().settings.font_size, 13.0);
    assert!(
        harness.state().layout().lines[0].height < taller,
        "smaller text should make a shorter line"
    );

    harness.state_mut().run_action(Action::ResetFontSize, &ctx);
    harness.run();
    assert_eq!(harness.state().settings.font_size, 16.0, "back to what a new Unluminous has");
    harness.snapshot(shot("font_size_reset"));
}

#[test]
fn a_pinch_over_the_editing_area_steps_the_font_size() {
    // A pinch on a trackpad and the wheel with the zoom modifier held both reach egui as
    // `Event::Zoom`, so this is the same path both take. The pointer has to be over the editing
    // area, because zooming is the document's and not the explorer's.
    let mut harness = harness("Pinch this line");
    let middle = harness.state().editor_area().center();
    assert_eq!(harness.state().settings.font_size, 16.0);

    let pinch = |harness: &mut Harness<'static, UnluminousApp>, factor: f32| {
        harness.input_mut().events.push(egui::Event::PointerMoved(middle));
        harness.input_mut().events.push(egui::Event::Zoom(factor));
        harness.run();
    };

    // Enough to ask for one size, which is the smallest gap between two of the sizes offered.
    pinch(&mut harness, 1.2);
    assert_eq!(harness.state().settings.font_size, 20.0, "one step up");
    // A bigger pinch asks for more than one, and lands on a size the dialog offers rather than on
    // whatever the multiplier works out to.
    pinch(&mut harness, 1.2 * 1.2 * 1.2);
    let bigger = harness.state().settings.font_size;
    assert!(bigger > 20.0, "a pinch three times the size should go further: {bigger}");
    assert!(settings::FONT_SIZES.contains(&bigger), "{bigger} is not a size the dialog offers");
    // And pinching the other way comes back down.
    pinch(&mut harness, 1.0 / (1.2 * 1.2 * 1.2));
    let smaller = harness.state().settings.font_size;
    assert!(smaller < bigger, "pinching in should shrink it: {bigger} then {smaller}");
    // The document is shown in it, not just the setting.
    assert_eq!(harness.state().document().chars().style_at(0).size, smaller);
}

#[test]
fn a_pinch_too_small_to_ask_for_a_size_is_kept_rather_than_thrown_away() {
    // A pinch arrives as a stream of multipliers a fraction over one, so a step is taken when what
    // has been asked for adds up to one rather than on any single frame. Without the remainder
    // being carried, a slow pinch would never move anything at all.
    let mut harness = harness("Pinch this line");
    let middle = harness.state().editor_area().center();
    for _ in 0..4 {
        harness.input_mut().events.push(egui::Event::PointerMoved(middle));
        harness.input_mut().events.push(egui::Event::Zoom(1.05));
        harness.run();
        harness.run();
    }
    assert_eq!(harness.state().settings.font_size, 20.0, "four small pinches add up to one size");
}

/// A file long enough to be scrolled about in, one short line a paragraph so nothing wraps.
fn a_long_file() -> String {
    (0..200).map(|n| format!("line {n} of the file\n")).collect()
}

/// A folder of long files for the zoom tests, kept out of [`sample_folder`] because a file written
/// there is a row in the explorer of every screenshot in this file.
fn a_folder_of_long_files() -> std::path::PathBuf {
    let long = a_long_file();
    fixture(
        "unluminous-zoom-folder",
        &[("long.txt", &long), ("longer.txt", &long), ("short.txt", "a short file\n")],
    )
}

/// Which paragraph of `file` is drawn `above` points below the top of its view.
///
/// The pane's own rectangle does not come into it: `above` is a distance below the top of a view,
/// and every pane's view starts at the same height, so this can ask about a file in a pane the test
/// is not focused on.
fn paragraph_in(file: &unluminous_app::app::files::OpenFile, above: f32) -> usize {
    let offset = file.cached.layout.offset_at(0.0, file.scroll + above);
    file.document.text().byte_to_line(offset)
}

/// The open file with this name, which is how a test asks about a pane it is not focused on.
fn file_named<'a>(app: &'a UnluminousApp, name: &str) -> &'a unluminous_app::app::files::OpenFile {
    app.files
        .iter()
        .find(|file| file.name() == name)
        .unwrap_or_else(|| panic!("{name} should be open"))
}

/// Which paragraph of the open file is drawn at `screen_y`.
///
/// The paragraph rather than the line or the offset in points, because it is the same number
/// whatever size the text is drawn at — which is exactly the question `task-1672` asks: is the
/// reader still looking at what they were looking at.
fn paragraph_under(app: &UnluminousApp, screen_y: f32) -> usize {
    let top = app.editor_area().top() + unluminous_app::theme::size::EDITOR_PADDING_Y;
    let down = app.files.active().scroll + (screen_y - top);
    let offset = app.layout().offset_at(0.0, down);
    app.document().text().byte_to_line(offset)
}

/// How far below the top of the view the caret is drawn.
fn caret_below_the_top(app: &UnluminousApp) -> f32 {
    app.layout().caret_at(app.document().selection().head).y - app.files.active().scroll
}

#[test]
fn a_pinch_keeps_the_line_under_the_pointer_where_it_is() {
    // `task-1672`: zooming in and then having to scroll back to the line you were zooming in on is
    // the whole complaint. The text under the pointer is what the gesture is about, so it is what
    // must not move.
    let mut harness = harness(&a_long_file());
    let area = harness.state().editor_area();
    // A long way down the file, so there is room to go wrong in either direction.
    harness.state_mut().files.active_mut().scroll = 900.0;
    harness.run();
    let at = egui::pos2(area.center().x, area.top() + area.height() * 0.6);
    let was_under_the_pointer = paragraph_under(harness.state(), at.y);
    let was_scrolled_to = harness.state().files.active().scroll;

    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    harness.input_mut().events.push(egui::Event::Zoom(1.2));
    harness.run();
    harness.run();

    assert_eq!(harness.state().settings.font_size, 20.0, "the pinch should have asked for a size");
    assert_eq!(
        paragraph_under(harness.state(), at.y),
        was_under_the_pointer,
        "the same line should still be under the pointer"
    );
    // And it took moving the view to keep it there: the same line is further down a document laid
    // out in a larger font, so a scroll position that did not change would be the fault itself.
    assert!(
        harness.state().files.active().scroll > was_scrolled_to,
        "the view should have followed the text down the file"
    );

    // Pinching back out is the same question the other way round. Two sizes' worth, because what a
    // gesture asks for and what a step costs do not divide, and the remainder of the pinch above is
    // carried — so asking for exactly one size back can land either side of the next step.
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    harness.input_mut().events.push(egui::Event::Zoom(1.0 / (1.2 * 1.2)));
    harness.run();
    harness.run();
    let smaller = harness.state().settings.font_size;
    assert!(smaller < 20.0, "pinching out should have come back down: {smaller}");
    assert_eq!(
        paragraph_under(harness.state(), at.y),
        was_under_the_pointer,
        "and still under it on the way back out"
    );
}

#[test]
fn the_keyboards_zoom_keeps_the_caret_where_it_is() {
    // A pinch is about the pointer; the keyboard has none, so it is about the caret, which is what
    // a person pressing command and plus is working on.
    let text = a_long_file();
    let mut harness = harness(&text);
    let ctx = harness.ctx.clone();
    let offset = text.find("line 90 ").expect("the file has a ninetieth line");
    harness.state_mut().command(Command::PlaceCaret { offset, extend: false });
    harness.run();
    // Scrolled so the caret sits well inside the view rather than at either edge of it.
    let caret = harness.state().layout().caret_at(offset).y;
    harness.state_mut().files.active_mut().scroll = caret - 200.0;
    harness.run();
    let was = caret_below_the_top(harness.state());
    assert!((was - 200.0).abs() < 1.0, "the caret should be 200 points down the view, not {was}");

    harness.state_mut().run_action(Action::ChangeFontSize { larger: true }, &ctx);
    harness.run();
    harness.run();
    assert_eq!(harness.state().settings.font_size, 20.0);
    let now = caret_below_the_top(harness.state());
    assert!((now - was).abs() < 3.0, "the caret should have stayed put: {was} then {now}");

    // Back down two sizes, and it is still where it was.
    harness.state_mut().run_action(Action::ChangeFontSize { larger: false }, &ctx);
    harness.run();
    harness.state_mut().run_action(Action::ChangeFontSize { larger: false }, &ctx);
    harness.run();
    harness.run();
    assert_eq!(harness.state().settings.font_size, 13.0);
    let smaller = caret_below_the_top(harness.state());
    assert!((smaller - was).abs() < 3.0, "and still there in a smaller font: {smaller}");
}

#[test]
fn a_zoom_leaves_a_tab_that_was_not_showing_at_the_line_it_was_left_at() {
    // The font is one setting for the whole window, so every tab is laid out again — and a tab that
    // came back scrolled somewhere else would be a tab that had moved while nobody was looking at
    // it. What is kept for those is the top of their view, since there is no pointer over them and
    // no caret being typed at.
    let folder = a_folder_of_long_files();
    let long = folder.join("long.txt");
    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    harness.state_mut().open_path_permanently(&long).expect("the file opens");
    harness.run();
    harness.state_mut().files.active_mut().scroll = 900.0;
    harness.run();
    let top = harness.state().editor_area().top() + unluminous_app::theme::size::EDITOR_PADDING_Y;
    let was_at_the_top = paragraph_under(harness.state(), top);

    // Show a different file, change the size from there, and come back.
    harness.state_mut().open_path_permanently(&folder.join("short.txt")).expect("the file opens");
    harness.run();
    harness.state_mut().run_action(Action::ChangeFontSize { larger: true }, &ctx);
    harness.run();
    harness.state_mut().open_path_permanently(&long).expect("the file opens");
    harness.run();
    harness.run();
    assert_eq!(harness.state().files.active().name(), "long.txt");
    assert_eq!(
        paragraph_under(harness.state(), top),
        was_at_the_top,
        "the line it was left at should still be the first one showing"
    );
}

#[test]
fn a_pinch_in_a_split_is_the_pointers_pane_and_steps_the_size_once() {
    // The size is one setting for the window, so a gesture is the window's rather than a pane's.
    // Every pane used to take the same `zoom_delta` for itself, which stepped the size once for
    // each of them: with two panes one notch of the wheel took sixteen points to thirty two.
    let folder = a_folder_of_long_files();
    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    harness.state_mut().open_path_permanently(&folder.join("long.txt")).expect("the file opens");
    harness.run();
    harness.state_mut().open_path_permanently(&folder.join("longer.txt")).expect("the file opens");
    harness.state_mut().run_action(Action::SplitRight, &ctx);
    harness.run();
    assert_eq!(harness.state().files.pane_count(), 2, "there should be two panes");
    let showing = harness.state().files.active().name();
    assert_eq!(showing, "longer.txt", "the new pane has the keyboard");

    // Both panes scrolled into their files, so an anchor that was not applied shows up as a line
    // that is not the one that was there.
    for name in ["long.txt", "longer.txt"] {
        let index = harness.state().files.iter().position(|file| file.name() == name).unwrap();
        harness.state_mut().files.at_mut(index).scroll = 900.0;
    }
    harness.run();

    // The pointer over the left hand pane, which is the one **without** the keyboard. The right
    // hand pane is the focused one, so its rectangle is the one the window reports, and the left
    // one is beside it.
    let right = harness.state().editor_area();
    let down = right.top() + unluminous_app::theme::size::EDITOR_PADDING_Y + 300.0;
    let at = egui::pos2(right.left() - 100.0, down);
    let was_under_the_pointer = paragraph_in(file_named(harness.state(), "long.txt"), 300.0);
    let was_at_the_top = paragraph_in(file_named(harness.state(), "longer.txt"), 0.0);

    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    harness.input_mut().events.push(egui::Event::Zoom(1.2));
    harness.run();
    harness.run();

    assert_eq!(
        harness.state().settings.font_size,
        20.0,
        "one notch is one size, however many panes thought the gesture was theirs"
    );
    assert_eq!(
        paragraph_in(file_named(harness.state(), "long.txt"), 300.0),
        was_under_the_pointer,
        "the pane under the pointer should have kept the line the pointer was on"
    );
    assert_eq!(
        paragraph_in(file_named(harness.state(), "longer.txt"), 0.0),
        was_at_the_top,
        "and the other pane the line at the top of it, there being no pointer over it"
    );
}

#[test]
fn a_zoom_keeps_the_markdown_previews_place_too() {
    // The preview is laid out from the same base style, so it moves in exactly the same way, and it
    // scrolls on its own — so it needs its own anchor rather than the source's.
    let text: String = (0..120).map(|n| format!("Paragraph {n} of the page.\n\n")).collect();
    let mut harness = harness(&text);
    let ctx = harness.ctx.clone();
    harness.state_mut().set_view_mode(ViewMode::Preview);
    harness.run();
    harness.state_mut().files.active_mut().preview_scroll = 700.0;
    harness.run();
    let at_the_top = |app: &UnluminousApp| {
        let scrolled = app.files.active().preview_scroll;
        app.preview_layout().offset_at(0.0, scrolled)
    };
    let was = at_the_top(harness.state());
    assert!(was > 0, "the preview should be scrolled into the page, not sitting at the top of it");

    harness.state_mut().run_action(Action::ChangeFontSize { larger: true }, &ctx);
    harness.run();
    harness.run();
    assert_eq!(harness.state().settings.font_size, 20.0);
    assert_eq!(at_the_top(harness.state()), was, "the same words should be at the top of the page");
}

#[test]
fn the_filter_box_puts_its_words_on_the_same_line_as_the_magnifier() {
    // It used to lay them out against the top edge of the box: a `TextEdit` with `Frame::NONE` has
    // no margin to be pushed down by, and it was given the whole height of the field to sit in.
    let harness = harness("");
    let filter = harness.get_by_label("Filter files").rect();
    // **The field is asked for rather than written down.** It was spelled out here as a copy of the
    // component's own arithmetic — 24 points tall, 36 points down the explorer, which itself starts under
    // the title bar — and every one of those numbers is a measurement of something else, so the copy went
    // stale twice. Once when `5161273` took the title bar from 50 points to 38, and again when
    // `task-1904` made the heading's height depend on whether the explorer is in a panel or in a node.
    //
    // Both times it failed by naming the wrong fault: "the row should sit in the middle of the field"
    // while the row was exactly in the middle of the field the component really drew. And the first time
    // the reason was hidden as well, because the snapshot assertion in the same test panicked first, so
    // what came back was a picture that had changed rather than a number that had moved.
    //
    // `explorer::filter_field` is the rectangle the component draws, so what is asserted now is the
    // relationship between the words and their field rather than a second copy of where the field is.
    let field = harness.state().explorer_filter_field();
    assert!(
        filter.height() < field.height(),
        "the box is one row of text, not the whole field: {filter:?} in {field:?}"
    );
    assert!(
        (filter.center().y - field.center().y).abs() < 1.5,
        "the row should sit in the middle of the field: {} against {}",
        filter.center().y,
        field.center().y
    );
}

#[test]
fn the_caret_is_no_taller_than_the_text_it_sits_in() {
    // The caret used to be drawn the full height of the line, which carries the font's line gap, the
    // reading leading Unluminous adds for prose and the paragraph's line spacing on top of the letters.
    let mut harness = harness("A line to put the caret in\nand a second one");
    harness.state_mut().command(Command::SelectAll);
    harness.state_mut().command(Command::SetLineSpacing(2.0));
    harness.state_mut().command(Command::MoveDocumentStart { extend: false });
    harness.run();
    let layout = harness.state().layout();
    let line = &layout.lines[0];
    let caret = layout.caret_at(0);
    assert!(
        caret.height < line.height * 0.75,
        "at double spacing the caret must be well short of the line: {} against {}",
        caret.height,
        line.height
    );
    assert!(caret.y >= line.y, "and it must start inside its own line");
    assert!(caret.y + caret.height <= line.bottom() + 0.01, "and end inside it");
    harness.snapshot(shot("caret_height"));
}

#[test]
fn the_background_setting_fades_the_window() {
    let mut harness = harness("The desktop shows through behind this.");
    let mut settings = harness.state().settings.clone();
    settings.opacity = 0.2;
    harness.state_mut().set_settings(settings);
    harness.run();
    assert_eq!(harness.state().background().a(), 51, "a fifth of the way up from nothing");
    harness.snapshot(shot("settings_background_faint"));
}

// The scrollbar, the two halves scrolling together, and dragging a tab — `task-1673`.

/// A long enough file that both halves of the side by side view have somewhere to scroll to.
fn a_long_markdown() -> String {
    let mut source = String::from("# The top of the file\n\n");
    for section in 0..40 {
        source.push_str(&format!("## Section {section}\n\nA paragraph of prose in section {section}, long enough that it is worth reading and takes a line or two of the page to say.\n\n"));
    }
    source
}

#[test]
fn a_document_taller_than_its_pane_has_a_scrollbar_that_can_be_dragged() {
    let mut harness = harness(&a_long_markdown());
    assert_eq!(harness.state().files.active().scroll, 0.0, "it starts at the top");
    let track = harness.get_by_label("Scroll untitled").rect();
    // From the thumb at the top of the track to a good way down it.
    let from = egui::pos2(track.center().x, track.top() + 10.0);
    drag(&mut harness, from, egui::pos2(track.center().x, track.center().y));
    let scrolled = harness.state().files.active().scroll;
    assert!(
        scrolled > 100.0,
        "dragging the thumb should have scrolled the file, it is at {scrolled}"
    );
    // And it stops where the page stops rather than running off the end.
    drag(
        &mut harness,
        egui::pos2(track.center().x, track.center().y),
        egui::pos2(track.center().x, track.bottom() + 400.0),
    );
    let bottom = harness.state().files.active().scroll;
    let overflow = harness.state().layout().height - harness.state().editor_area().height()
        + unluminous_app::theme::size::EDITOR_PADDING_Y * 2.0;
    assert!(bottom <= overflow + 1.0, "{bottom} is past the {overflow} there is to scroll");
    harness.snapshot(shot("scrollbar_dragged"));
}

#[test]
fn a_document_that_fits_its_pane_has_no_scrollbar() {
    let mut harness = harness("one line");
    harness.run();
    assert!(
        harness.query_by_label("Scroll untitled").is_none(),
        "nothing to scroll, so there is nothing to draw"
    );
}

#[test]
fn scrolling_the_source_scrolls_the_preview_with_it() {
    let mut harness = harness(&a_long_markdown());
    harness.get_by_label("Side by side").click();
    harness.run();
    assert_eq!(harness.state().files.active().preview_scroll, 0.0);
    // The wheel over the source, which is what a person does.
    let over_the_source = harness.state().editor_area().center();
    harness.input_mut().events.push(egui::Event::PointerMoved(over_the_source));
    harness.run();
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -600.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::default(),
    });
    harness.run();
    harness.run();
    let source = harness.state().files.active().scroll;
    let preview = harness.state().files.active().preview_scroll;
    assert!(source > 100.0, "the source should have scrolled, it is at {source}");
    assert!(preview > 100.0, "and the preview should have followed it, it is at {preview}");
    // And they are showing the same part of the file, rather than the same number of points down two
    // pages of different heights.
    let paragraph = harness.state().layout().paragraph_at_y(source).0;
    let map = harness.state().preview_source_lines();
    let line =
        map.get(harness.state().preview_layout().paragraph_at_y(preview).0).copied().unwrap_or(0);
    assert!(
        line.abs_diff(paragraph) <= 1,
        "the source is at line {paragraph} and the preview is showing line {line}"
    );
    harness.snapshot(shot("side_by_side_scrolled_together"));
}

#[test]
fn scrolling_the_preview_scrolls_the_source_with_it() {
    let mut harness = harness(&a_long_markdown());
    harness.get_by_label("Side by side").click();
    harness.run();
    // The wheel over the preview, which is the right hand half of the editing area.
    let source_area = harness.state().editor_area();
    let over_the_preview =
        egui::pos2(source_area.right() + source_area.width() / 2.0, source_area.center().y);
    harness.input_mut().events.push(egui::Event::PointerMoved(over_the_preview));
    harness.run();
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -600.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::default(),
    });
    harness.run();
    harness.run();
    assert!(harness.state().files.active().preview_scroll > 100.0);
    assert!(
        harness.state().files.active().scroll > 100.0,
        "the source should have followed the preview, it is at {}",
        harness.state().files.active().scroll
    );
}

/// **The two halves settle rather than chasing each other.** The crossing snaps to a paragraph, so a
/// rule that moved both halves every frame would creep down the file on its own for as long as the
/// window was left open. Nothing is touched, and both stay where they were.
#[test]
fn the_two_halves_do_not_chase_each_other_when_nothing_is_touched() {
    let mut harness = harness(&a_long_markdown());
    harness.get_by_label("Side by side").click();
    harness.run();
    harness.state_mut().files.active_mut().scroll = 900.0;
    harness.run();
    harness.run();
    let settled =
        (harness.state().files.active().scroll, harness.state().files.active().preview_scroll);
    for _ in 0..30 {
        harness.run();
    }
    let after =
        (harness.state().files.active().scroll, harness.state().files.active().preview_scroll);
    assert!(
        (settled.0 - after.0).abs() < 0.5 && (settled.1 - after.1).abs() < 0.5,
        "left alone for thirty frames the view moved from {settled:?} to {after:?}"
    );
}

// Highlighting a passage (`task-1663`).
//
// The four colour blocks, the drawn colour wheel and the command line that marks passages across as
// many files as you like. The set itself is tested in `unluminous-core` with no window, and the file
// beside the project in `services::file_marks`; these are for what only the real window can show —
// that the colour is behind the words, that the writing over it is still readable, and that the menu
// looks like the rest of Unluminous.

/// A passage to mark, and enough text round it that a screenshot shows the mark against the page.
const MARKABLE: &str = "The quick brown fox jumps over the lazy dog.\n\
                        Sphinx of black quartz, judge my vow.\n\
                        Pack my box with five dozen liquor jugs.\n\
                        How vexingly quick daft zebras jump.\n";

/// Open the editing area's own menu where the gutter's menu tests open theirs: by setting the
/// window's state, because the harness cannot press the right mouse button.
fn open_text_menu(harness: &mut Harness<'static, UnluminousApp>, offset: usize) {
    let at = harness.state().editor_area().left_top() + vec2(120.0, 60.0);
    harness.state_mut().text_menu =
        Some(unluminous_app::components::text_menu::TextMenu::new(at, offset));
    harness.run();
}

#[test]
fn the_editing_areas_own_menu_holds_four_colours_and_the_wheels_icon() {
    let mut harness = harness(MARKABLE);
    select_phrase(&mut harness, "quick brown fox", &[]);
    open_text_menu(&mut harness, 4);
    for name in ["Highlight yellow", "Highlight green", "Highlight blue", "Highlight pink"] {
        harness.get_by_label(name);
    }
    harness.get_by_label("Choose a colour");
    harness.get_by_label("Copy");
    harness.snapshot(shot("text_menu"));
}

#[test]
fn the_colour_wheel_opens_inside_the_menu_rather_than_in_a_second_popup() {
    // egui keeps one popup open at a time, so the wheel has to be part of this one. What that means
    // for a test is that the menu's own rows are still there while the wheel is showing.
    let mut harness = harness(MARKABLE);
    select_phrase(&mut harness, "quick brown fox", &[]);
    open_text_menu(&mut harness, 4);
    harness.get_by_label("Choose a colour").click();
    harness.run();
    for name in ["Highlight hue", "Highlight shade", "Highlight opacity", "Apply highlight"] {
        harness.get_by_label(name);
    }
    harness.get_by_label("Highlight yellow");
    harness.snapshot(shot("text_menu_wheel"));
}

#[test]
fn choosing_a_colour_marks_the_selection_and_shuts_the_menu() {
    let mut harness = harness(MARKABLE);
    select_phrase(&mut harness, "quick brown fox", &[]);
    open_text_menu(&mut harness, 4);
    harness.get_by_label("Highlight blue").click();
    harness.run();
    assert!(harness.state().text_menu.is_none(), "choosing a colour puts the menu away");
    let marks = harness.state().document().highlights();
    assert_eq!(marks.len(), 1);
    assert_eq!(
        marks.iter().next().unwrap().color,
        unluminous_core::Rgba::new(0x48, 0x9F, 0xF8, 0x59)
    );
}

#[test]
fn three_passages_in_three_colours_are_drawn_behind_the_writing() {
    let mut harness = harness(MARKABLE);
    let ctx = harness.ctx.clone();
    for (phrase, action) in [
        ("quick brown fox", Action::Highlight(HighlightColor::Yellow)),
        ("black quartz", Action::Highlight(HighlightColor::Green)),
        ("five dozen liquor jugs", Action::Highlight(HighlightColor::Pink)),
    ] {
        select_phrase(&mut harness, phrase, &[]);
        harness.state_mut().run_action(action, &ctx);
        harness.run();
    }
    collapse(&mut harness);
    let colours: Vec<String> =
        harness.state().document().highlights().iter().map(|mark| mark.color.to_hex()).collect();
    assert_eq!(colours, vec!["#FEBC2E59", "#7FCA9859", "#B4588C59"], "in the order they appear");
    harness.snapshot(shot("highlights"));
}

#[test]
fn clearing_takes_the_one_under_the_caret_and_leaves_the_others_drawn() {
    let mut harness = harness(MARKABLE);
    let ctx = harness.ctx.clone();
    for phrase in ["quick brown fox", "black quartz", "five dozen liquor jugs"] {
        select_phrase(&mut harness, phrase, &[]);
        harness.state_mut().run_action(Action::Highlight(HighlightColor::Yellow), &ctx);
        harness.run();
    }
    // The caret inside the second one, as a right click on it would leave it.
    let text = harness.state().document().text().to_string();
    let at = text.find("black quartz").expect("the phrase") + 3;
    harness.state_mut().command(Command::PlaceCaret { offset: at, extend: false });
    harness.state_mut().run_action(Action::ClearHighlight, &ctx);
    harness.run();
    assert_eq!(harness.state().document().highlights().len(), 2);
    harness.snapshot(shot("highlight_cleared"));
}

#[test]
fn a_mark_moves_with_the_text_it_is_on() {
    let mut harness = harness(MARKABLE);
    let ctx = harness.ctx.clone();
    select_phrase(&mut harness, "black quartz", &[]);
    harness.state_mut().run_action(Action::Highlight(HighlightColor::Green), &ctx);
    let before = harness.state().document().highlights().iter().next().unwrap().range.clone();

    // Type a whole line above it.
    harness.state_mut().command(Command::MoveDocumentStart { extend: false });
    harness.state_mut().command(Command::Insert("a new first line\n".to_owned()));
    harness.run();
    let after = harness.state().document().highlights().iter().next().unwrap().range.clone();
    assert_eq!(after.start, before.start + "a new first line\n".len());
    let marked = harness.state().document().text().byte_slice(after.clone());
    assert_eq!(marked, "black quartz", "the mark is still on the words it was put on");

    // And undo puts it back where it was.
    harness.state_mut().command(Command::Undo);
    harness.run();
    assert_eq!(harness.state().document().highlights().iter().next().unwrap().range, before);
}

#[test]
fn a_right_click_in_the_writing_opens_the_menu_where_it_was_pressed() {
    let mut harness = harness(MARKABLE);
    collapse(&mut harness);
    // Over the second line, which is where the caret should end up.
    let at = harness.state().editor_area().left_top() + vec2(120.0, 60.0);
    right_click_at(&mut harness, at);
    let menu = harness.state().text_menu.clone().expect("the right click should open the menu");
    assert_eq!(menu.at, at, "the menu opens where the pointer was");
    assert_eq!(
        harness.state().document().selection().head,
        menu.offset,
        "and a right click outside a selection puts the caret where it was pressed"
    );
    assert!(menu.offset > 0, "the pointer was over the writing, not before it");
    harness.get_by_label("Highlight yellow");
}

#[test]
fn a_right_click_inside_a_selection_leaves_the_selection_alone() {
    // Otherwise the menu would open with nothing to mark, which is the whole point of it.
    let mut harness = harness(MARKABLE);
    select_phrase(&mut harness, "The quick brown fox jumps over the lazy dog", &[]);
    let before = harness.state().document().selection();
    let at = harness.state().editor_area().left_top() + vec2(120.0, 20.0);
    right_click_at(&mut harness, at);
    assert!(harness.state().text_menu.is_some());
    assert_eq!(harness.state().document().selection(), before, "the selection is untouched");
    harness.get_by_label("Highlight blue").click();
    harness.run();
    assert_eq!(harness.state().document().highlights().len(), 1);
}

#[test]
fn the_edit_menu_holds_the_four_colours_under_a_highlight_heading() {
    // The four colours are on a menu so that each has an `Action` with a name, which is what puts
    // them on the command line. Inside the window a submenu is drawn as a heading with its rows
    // indented under it, which is what Recent Projects and the explorer's Git submenu already do.
    let mut harness = harness(MARKABLE);
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    select_phrase(&mut harness, "quick brown fox", &[]);
    harness.get_by_label("Edit").click();
    harness.run();
    for entry in ["Yellow", "Green", "Blue", "Pink", "Clear Highlight", "Clear All Highlights"] {
        harness.get_by_label(entry);
    }
    harness.snapshot(shot("edit_menu"));
}

#[test]
fn the_command_line_marks_a_passage_and_lists_it() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open readme.md --permanent");
    let marked = did(&mut harness, "highlight add --from-line 1 --to-line 1 --color blue");
    assert_eq!(marked["marked"], 1);
    assert_eq!(marked["color"], "#489FF859");

    let listed = did(&mut harness, "highlight list");
    let rows = listed["highlights"].as_array().expect("a list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["fromLine"], 1);
    assert_eq!(rows[0]["text"], "# Unluminous");
    assert_eq!(harness.state().document().highlights().len(), 1);
}

#[test]
fn the_command_line_marks_every_occurrence_of_some_words() {
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-highlight-occurrences");
    std::fs::write(folder.join("repeated.txt"), "one two one two one\n").expect("write it");
    let mut harness = harness_in(&folder);
    let marked = did(&mut harness, "highlight add repeated.txt --text one --color pink");
    assert_eq!(marked["marked"], 3, "every occurrence, not the first");
    let listed = did(&mut harness, "highlight list repeated.txt");
    assert_eq!(listed["highlights"].as_array().unwrap().len(), 3);
    assert!(
        harness.state().files.index_of(&folder.join("repeated.txt")).is_none(),
        "the file was never opened, which is the point of naming it"
    );
}

#[test]
fn a_bulk_request_marks_passages_across_several_files_in_one_call() {
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-highlight-bulk");
    let mut harness = harness_in(&folder);
    // Two hashes on the raw string, because a colour of its own is written `"#FF00FF80"` and `"#`
    // would otherwise end the literal in the middle of the request.
    let request = r##"[
        {"path":"readme.md","fromLine":1,"toLine":1,"color":"yellow"},
        {"path":"notes.txt","fromLine":1,"toLine":1,"color":"green"},
        {"path":"chapters/one.md","fromLine":1,"toLine":1,"color":"#FF00FF80"},
        {"path":"nowhere.md","fromLine":1,"toLine":1}
    ]"##
    .split_whitespace()
    .collect::<String>();
    // In single quotes, as it would be typed at a shell: the window splits a command line the same
    // way, so the double quotes inside the JSON survive.
    let result = did(&mut harness, &format!("highlight apply --json-text '{request}'"));
    assert_eq!(result["marked"], 3);
    assert_eq!(result["files"].as_array().unwrap().len(), 3);
    assert_eq!(
        result["refused"].as_array().unwrap().len(),
        1,
        "the file that is not there is refused by number and the rest still go in"
    );

    let everywhere = did(&mut harness, "highlight list --all");
    assert_eq!(everywhere["highlights"].as_array().unwrap().len(), 3);

    // Opening one of them shows the mark that was made while it was closed.
    did(&mut harness, "tab open chapters/one.md");
    let marks = harness.state().document().highlights();
    assert_eq!(marks.len(), 1);
    assert_eq!(
        marks.iter().next().unwrap().color,
        unluminous_core::Rgba::new(0xFF, 0x00, 0xFF, 0x80)
    );

    // And clearing everything really does clear the open file as well as the closed ones.
    let cleared = did(&mut harness, "highlight clear --all");
    assert_eq!(cleared["cleared"], 3);
    assert!(harness.state().document().highlights().is_empty());
    assert_eq!(
        did(&mut harness, "highlight list --all")["highlights"].as_array().unwrap().len(),
        0
    );
}

#[test]
fn a_colour_that_is_not_a_colour_is_refused_with_the_names_that_are() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open readme.md --permanent");
    let reply = run(&mut harness, "highlight add --from-line 1 --color puce");
    assert!(!reply.ok);
    assert!(
        reply.message.contains("yellow"),
        "the refusal should name the colours: {}",
        reply.message
    );
    assert_eq!(refused(&mut harness, "highlight add --from-line 9 --to-line 2"), "usage");
}

#[test]
fn the_four_colours_and_the_two_ways_of_clearing_are_menu_entries_with_names() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open readme.md --permanent");
    did(&mut harness, "editor select --all");
    for name in ["highlight-yellow", "highlight-green", "highlight-blue", "highlight-pink"] {
        did(&mut harness, &format!("action run {name}"));
    }
    assert_eq!(
        harness.state().document().highlights().len(),
        1,
        "each colour replaces the last over the same passage"
    );
    did(&mut harness, "action run clear-highlights");
    assert!(harness.state().document().highlights().is_empty());
    let listed = did(&mut harness, "action list");
    let names: Vec<String> = listed["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    for name in ["highlight-yellow", "clear-highlight", "clear-highlights"] {
        assert!(names.contains(&name.to_owned()), "`action list` should offer {name}");
    }
}

// -------------------------------------------------------------------------------------- task-1922
//
// The line commands, the comment toggles and the two settings that decide what a keystroke types.
// WP4's editing half: each is a `Command` in `unluminous-core` reached two ways — the menu entry a
// person clicks and the command the catalogue offers — and what these prove is that the two ways
// arrive at the same text.

/// A project with one file of each of the three shapes the comment rules care about.
fn commenting_folder() -> std::path::PathBuf {
    fixture(
        "unluminous-1922-comments",
        &[
            ("main.rs", "fn one() {}\nfn two() {}\nfn three() {}\n"),
            ("site.css", "a { color: red; }\nb { color: blue; }\n"),
            ("notes.md", "one  \ntwo  \n"),
        ],
    )
}

fn opened(name: &str) -> Harness<'static, UnluminousApp> {
    let folder = commenting_folder();
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("tab open {name} --permanent"));
    harness
}

#[test]
fn the_line_comment_marker_comes_from_the_plugin_and_the_toggle_is_its_own_inverse() {
    let mut harness = opened("main.rs");
    let was = harness.state().document().text().to_string();
    did(&mut harness, "editor select --from-line 1 --to-line 2");
    let commented = did(&mut harness, "editor comment --toggle");
    assert_eq!(
        commented["marker"], "//",
        "the marker is the Rust plugin's, not a list in Unluminous"
    );
    assert_eq!(commented["lines"], 2);
    let text = harness.state().document().text().to_string();
    assert!(text.starts_with("//fn one() {}\n//fn two() {}\n"), "{text}");
    assert!(text.ends_with("fn three() {}\n"), "the third line was not touched: {text}");
    // The same command again gives the bytes back, which is what makes one chord both.
    did(&mut harness, "editor comment --toggle");
    assert_eq!(harness.state().document().text().to_string(), was);
    // And it is one undo step whichever way it went.
    did(&mut harness, "editor comment --toggle");
    did(&mut harness, "editor undo");
    assert_eq!(harness.state().document().text().to_string(), was);
}

#[test]
fn a_language_with_no_line_comment_is_refused_and_has_no_menu_entry() {
    // CSS has a block comment and no `//`, so one of the two entries is absent and the other is
    // there — which is the whole reason the two questions are separate.
    let mut harness = opened("site.css");
    assert_eq!(refused(&mut harness, "editor comment --toggle"), "not-applicable");
    let found = did(&mut harness, "action find comment");
    let names: Vec<String> = found["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(!names.contains(&"toggle-line-comment".to_owned()), "absent, not dimmed: {names:?}");
    assert!(
        names.contains(&"toggle-block-comment".to_owned()),
        "CSS keeps the block one: {names:?}"
    );
    // And the block one really works on it.
    did(&mut harness, "editor select --from-line 1 --to-line 1");
    did(&mut harness, "editor comment --block");
    assert!(harness.state().document().text().to_string().starts_with("/*"));
}

#[test]
fn the_four_line_commands_change_the_text_and_each_is_one_undo_step() {
    let mut harness = opened("main.rs");
    let was = harness.state().document().text().to_string();

    did(&mut harness, "editor caret --line 1");
    let duplicated = did(&mut harness, "editor lines duplicate");
    assert_eq!(duplicated["lines"], 1);
    assert!(harness
        .state()
        .document()
        .text()
        .to_string()
        .starts_with("fn one() {}\nfn one() {}\n"));
    did(&mut harness, "editor undo");
    assert_eq!(harness.state().document().text().to_string(), was);

    did(&mut harness, "editor caret --line 2");
    assert_eq!(did(&mut harness, "editor lines move --by -1")["moved"], true);
    assert!(harness
        .state()
        .document()
        .text()
        .to_string()
        .starts_with("fn two() {}\nfn one() {}\n"));
    did(&mut harness, "editor undo");
    assert_eq!(harness.state().document().text().to_string(), was);

    did(&mut harness, "editor caret --line 1");
    assert_eq!(did(&mut harness, "editor lines join")["joined"], true);
    assert!(harness.state().document().text().to_string().starts_with("fn one() {} fn two() {}"));
    did(&mut harness, "editor undo");
    assert_eq!(harness.state().document().text().to_string(), was);

    did(&mut harness, "editor select --from-line 1 --to-line 3");
    did(&mut harness, "editor lines sort");
    let sorted = harness.state().document().text().to_string();
    assert!(sorted.starts_with("fn one() {}\nfn three() {}\nfn two() {}\n"), "{sorted}");
    did(&mut harness, "editor undo");
    assert_eq!(harness.state().document().text().to_string(), was);
}

#[test]
fn sorting_with_nothing_selected_is_refused_rather_than_sorting_the_whole_file() {
    let mut harness = opened("main.rs");
    let was = harness.state().document().text().to_string();
    assert_eq!(refused(&mut harness, "editor lines sort"), "not-applicable");
    assert_eq!(harness.state().document().text().to_string(), was);
    // And the menu row says the same thing by being dimmed rather than absent: a selection is a
    // thing somebody can make in a moment.
    let row = did(&mut harness, "action find sort lines");
    assert_eq!(row["actions"][0]["name"], "sort-lines");
    assert_eq!(row["actions"][0]["enabled"], false);
}

#[test]
fn a_menu_entry_and_its_command_reach_the_same_text() {
    // The rule the whole of WP4 is held to: a person's half and an agent's half are one function.
    let mut harness = opened("main.rs");
    did(&mut harness, "editor select --from-line 1 --to-line 2");
    did(&mut harness, "editor comment --toggle");
    let by_command = harness.state().document().text().to_string();
    did(&mut harness, "editor undo");

    did(&mut harness, "editor select --from-line 1 --to-line 2");
    choose(&mut harness, Action::ToggleLineComment);
    assert_eq!(harness.state().document().text().to_string(), by_command);
}

#[test]
fn trimming_never_runs_on_a_markdown_file_whatever_the_setting_says() {
    // Two spaces at the end of a line are a line break there, so trimming them would change what
    // the document means rather than tidying it.
    let mut harness = opened("notes.md");
    assert_eq!(refused(&mut harness, "editor trim"), "not-applicable");
    assert_eq!(harness.state().document().text().to_string(), "one  \ntwo  \n");
    did(&mut harness, "settings set editor.trim true");
    did(&mut harness, "tab save");
    assert_eq!(
        harness.state().document().text().to_string(),
        "one  \ntwo  \n",
        "saving a Markdown file leaves its line breaks alone even with the setting on"
    );
}

#[test]
fn trailing_whitespace_goes_on_a_save_only_once_the_setting_asks_for_it() {
    let folder = fixture("unluminous-1922-trim", &[("main.rs", "fn one() {}   \nfn two() {}\t\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open main.rs --permanent");
    // Off, which is the shipped default: a save that changed bytes nobody typed is a save nobody
    // asked for.
    did(&mut harness, "editor caret --line 1 --column 1");
    did(&mut harness, "editor insert x");
    did(&mut harness, "tab save");
    assert!(std::fs::read_to_string(folder.join("main.rs")).unwrap().contains("   \n"));

    did(&mut harness, "settings set editor.trim true");
    did(&mut harness, "editor insert y");
    did(&mut harness, "tab save");
    let written = std::fs::read_to_string(folder.join("main.rs")).unwrap();
    assert!(!written.contains("   \n"), "{written:?}");
    assert!(!written.contains("\t\n"), "{written:?}");
}

/// Press one key the way a keyboard does, which is the only way to find out what it types.
fn press(harness: &mut Harness<'static, UnluminousApp>, key: egui::Key) {
    harness.event(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();
}

#[test]
fn what_the_tab_key_types_is_a_tab_until_the_indent_setting_says_otherwise() {
    let folder = fixture("unluminous-1922-tab", &[("main.rs", "one\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open main.rs --permanent");
    did(&mut harness, "editor caret --line 1 --column 4");
    harness.state_mut().focus = unluminous_app::app::Focus::Editor;
    harness.run();

    press(&mut harness, egui::Key::Tab);
    assert_eq!(harness.state().document().text().to_string(), "one\t\n");

    did(&mut harness, "settings set editor.indent spaces:4");
    press(&mut harness, egui::Key::Tab);
    assert_eq!(harness.state().document().text().to_string(), "one\t    \n");
    // And a width no editor offers is refused rather than clamped, so `spaces:40` does not quietly
    // become `spaces:8`.
    assert_eq!(refused(&mut harness, "settings set editor.indent spaces:40"), "usage");
    assert_eq!(did(&mut harness, "settings get editor.indent")["value"], "spaces:4");
}

#[test]
fn a_new_line_starts_where_the_line_it_was_started_from_starts() {
    let folder =
        fixture("unluminous-1922-indent", &[("main.rs", "fn one() {\n    let a = 1;\n}\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open main.rs --permanent");
    did(&mut harness, "editor caret --line 2 --column 15");
    harness.state_mut().focus = unluminous_app::app::Focus::Editor;
    harness.run();
    press(&mut harness, egui::Key::Enter);
    let text = harness.state().document().text().to_string();
    assert!(text.contains("    let a = 1;\n    \n}"), "{text:?}");
    // One undo step, because the line break and its indentation are one `Command::Insert`.
    did(&mut harness, "editor undo");
    assert_eq!(harness.state().document().text().to_string(), "fn one() {\n    let a = 1;\n}\n");

    did(&mut harness, "settings set editor.auto_indent false");
    press(&mut harness, egui::Key::Enter);
    let text = harness.state().document().text().to_string();
    assert!(
        text.contains("    let a = 1;\n\n}"),
        "switched off, a new line is a new line: {text:?}"
    );
}

// -------------------------------------------------------------------------------------- task-1984
//
// The two documents that ended the process, through the window rather than through the crate.

/// A document of ten thousand `>` and one of ten thousand `*` are shown rather than ending the
/// process.
///
/// `task-1984` C2 and C3 measured both, and `unluminous-core`'s own tests hold the parser's half.
/// This is the half that says the window survives them, on the two paths a person actually reaches
/// them by: the Markdown preview, which runs on every text revision of the open tab, and
/// `components::markdown_text`, which is what every message in the chat pane is drawn with — so a
/// model's answer with a banner of asterisks in it is a thing that arrives rather than a thing
/// somebody types.
///
/// A stack overflow is not a panic. `crash.log` would be empty, macOS would file no report, and the
/// test binary would end with `STATUS_STACK_OVERFLOW` and no failing test named — which is what this
/// did before the fix.
#[test]
fn a_pathological_document_is_drawn_rather_than_ending_the_process() {
    let quotes = format!("{}hello\n", ">".repeat(10_000));
    let stars = format!("{}a{}\n", "*".repeat(10_000), "*".repeat(10_000));
    let folder = fixture(
        "unluminous-1984-pathological",
        &[("quotes.md", quotes.as_str()), ("stars.md", stars.as_str())],
    );
    let mut harness = harness_in(&folder);

    for name in ["quotes.md", "stars.md"] {
        did(&mut harness, &format!("tab open {name} --permanent"));
        // Both view modes, because the preview is what reads the document and the split view lays it
        // out beside the source.
        did(&mut harness, "editor view preview");
        did(&mut harness, "editor view side");
        did(&mut harness, "editor view raw");
    }

    // And the chat pane's own renderer, which is the reachable path: this is a model's answer rather
    // than a file somebody opened.
    let colors = unluminous_app::components::markdown_text::Colors {
        text: egui::Color32::WHITE,
        strong: egui::Color32::WHITE,
        code: egui::Color32::GRAY,
        link: egui::Color32::LIGHT_BLUE,
        quiet: egui::Color32::GRAY,
        rule: egui::Color32::GRAY,
    };
    for source in [&quotes, &stars] {
        let rendered = unluminous_app::components::markdown_text::render(
            source,
            &harness.state().renderer,
            "Consolas",
            12.0,
            colors,
            400.0,
            None,
        );
        assert!(rendered.height() > 0.0, "the answer was laid out rather than ending the process");
    }
}
