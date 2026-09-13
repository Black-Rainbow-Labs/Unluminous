//! Colouring a file, the themes, and the Mermaid diagrams.
//!
//! The three language plugins whose colouring can only be judged by looking — TypeScript, CSS and
//! HTML — the plugins page, every theme with the colours it is made of, and one image per Mermaid
//! diagram type rendered through the real window and the graphics card.
//!
//! The parsers and the layout underneath all of this are tested in `unluminous-core` with no window
//! at all, where the numbers can be checked by hand. What is here is the one thing that cannot be
//! asserted: what the picture looks like. Every accepted image was opened and looked at before it
//! was accepted.
//!
//! **13 of the 22 tests here take a picture**, and `every_diagram_type_is_drawn_in_the_real_window`
//! alone takes twenty of them, one per diagram type.

mod common;

use common::*;

use std::sync::OnceLock;

use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotResults};
use unluminous_app::app::ViewMode;
use unluminous_app::UnluminousApp;
use unluminous_core::Color;

#[test]
fn a_typescript_file_is_coloured_by_its_plugin() {
    // Deliberately *not* a repository: this test is about the colours, and a window in a repository
    // has a git message in its status bar for the first few frames, which made the picture depend on
    // how quickly a thread answered.
    let folder = copy_out_of_the_repository(&git_folder("syntax"), "unluminous-screenshot-syntax");
    std::fs::remove_dir_all(folder.join(".git")).ok();
    let mut harness = harness_in(&folder);
    harness
        .state_mut()
        .open_path_permanently(&folder.join("sqlClient.ts"))
        .expect("the file opens");
    harness.run();
    harness.run();
    // The colours are in the document's own spans, so the test can check them without looking at
    // the picture: the `import` keyword is Dracula's pink and the comment is its blue-grey.
    let text = harness.state().document().text().to_string();
    let chars = harness.state().document().chars();
    // Asked one byte *inside* each token. `style_at` reports the earlier span for an offset that
    // falls on the boundary between two, so that typing at the end of a bold word stays bold, and
    // asking at a token's first byte would report whatever came before it.
    let inside = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("no {needle}")) + 1;
    assert_eq!(
        chars.style_at(inside("import")).color,
        Color::rgb(0xFF, 0x79, 0xC6),
        "import is a keyword, in Dracula's pink"
    );
    assert_eq!(
        chars.style_at(inside("/**")).color,
        Color::rgb(0x62, 0x72, 0xA4),
        "the doc comment, in Dracula's blue-grey"
    );
    assert_eq!(
        chars.style_at(inside("'../db")).color,
        Color::rgb(0xF1, 0xFA, 0x8C),
        "the import's path is a string, in Dracula's yellow"
    );
    assert_eq!(
        chars.style_at(inside("MessageRepository")).color,
        Color::rgb(0x8B, 0xE9, 0xFD),
        "a name starting with a capital is a type, in Dracula's cyan"
    );
    // Colouring is not an edit: nothing to undo and nothing to save.
    assert!(!harness.state().document().is_modified(), "colouring must not mark the file changed");
    assert!(!harness.state().document().can_undo(), "and must not push onto the undo history");
    harness.snapshot(shot("syntax_typescript"));
}

#[test]
fn a_css_file_is_coloured_by_its_plugin() {
    // `task-1671`. Not a repository, for the reason the TypeScript one above gives: a window in one
    // has a git message in its status bar for the first few frames.
    let folder = std::env::temp_dir().join("unluminous-screenshot-css");
    std::fs::create_dir_all(&folder).expect("make the folder");
    let path = folder.join("site.css");
    std::fs::write(
        &path,
        "/* the card */\n@media screen and (min-width: 40rem) {\n  .card:hover {\n    background-color: #ff79c6;\n    display: flex;\n    font-family: \"Iosevka\", monospace;\n    width: calc(100% - 2rem);\n    --brand-hue: 280;\n  }\n}\n",
    )
    .expect("write site.css");
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&path).expect("the file opens");
    harness.run();
    harness.run();
    // Read out of the document's own spans, so the five things the plugin had to be taught are
    // checked as colours rather than only looked at.
    let text = harness.state().document().text().to_string();
    let chars = harness.state().document().chars();
    let inside = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("no {needle}")) + 1;
    assert_eq!(
        chars.style_at(inside("@media")).color,
        Color::rgb(0xFF, 0x79, 0xC6),
        "an at-rule is a keyword, in Dracula's pink, and the at sign is part of the word"
    );
    assert_eq!(
        chars.style_at(inside("background-color")).color,
        Color::rgb(0xBD, 0x93, 0xF9),
        "a property is a builtin, in Dracula's purple, hyphen and all"
    );
    assert_eq!(
        chars.style_at(inside("flex;")).color,
        Color::rgb(0x8B, 0xE9, 0xFD),
        "a value keyword is a type, in Dracula's cyan"
    );
    assert_eq!(
        chars.style_at(inside("#ff79c6")).color,
        Color::rgb(0xFF, 0xB8, 0x6C),
        "a hex colour is a number, in Dracula's orange"
    );
    assert_eq!(
        chars.style_at(inside("calc(")).color,
        Color::rgb(0x50, 0xFA, 0x7B),
        "a word before a bracket is a function, in Dracula's green"
    );
    assert_eq!(
        chars.style_at(inside("/* the card */")).color,
        Color::rgb(0x62, 0x72, 0xA4),
        "the comment, in Dracula's blue-grey"
    );
    assert!(!harness.state().document().is_modified(), "colouring must not mark the file changed");
    harness.snapshot(shot("syntax_css"));
}

#[test]
fn an_html_file_is_coloured_by_its_plugin() {
    // `task-1694`. Not a repository, for the reason the CSS one above gives: a window in one has a
    // git message in its status bar for the first few frames.
    let folder = std::env::temp_dir().join("unluminous-screenshot-html");
    std::fs::create_dir_all(&folder).expect("make the folder");
    let path = folder.join("page.html");
    std::fs::write(
        &path,
        "<!-- the card -->\n<div class=\"card\">\n  <style>\n    .card { background-color: #ff79c6; }\n  </style>\n  <my-widget>Tom &amp; Jerry</my-widget>\n</div>\n",
    )
    .expect("write page.html");
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&path).expect("the file opens");
    harness.run();
    harness.run();
    // Read out of the document's own spans, so the things the plugin had to be taught are checked
    // as colours rather than only looked at.
    let text = harness.state().document().text().to_string();
    let chars = harness.state().document().chars();
    let inside = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("no {needle}")) + 1;
    assert_eq!(
        chars.style_at(inside("<!--")).color,
        Color::rgb(0x62, 0x72, 0xA4),
        "the comment, in Dracula's blue-grey"
    );
    assert_eq!(
        chars.style_at(inside("div")).color,
        Color::rgb(0xFF, 0x79, 0xC6),
        "an element name is a keyword, in Dracula's pink"
    );
    assert_eq!(
        chars.style_at(inside("class")).color,
        Color::rgb(0xBD, 0x93, 0xF9),
        "an attribute name is a builtin, in Dracula's purple"
    );
    assert_eq!(
        chars.style_at(inside("\"card\"")).color,
        Color::rgb(0xF1, 0xFA, 0x8C),
        "a quoted value is a string, in Dracula's yellow"
    );
    assert_eq!(
        chars.style_at(inside("my-widget")).color,
        Color::rgb(0x8B, 0xE9, 0xFD),
        "an element the language does not name is a type, in Dracula's cyan"
    );
    assert_eq!(
        chars.style_at(inside("&amp;")).color,
        Color::rgb(0xFF, 0xB8, 0x6C),
        "a character reference in prose is a number, in Dracula's orange"
    );
    assert_eq!(
        chars.style_at(inside("Tom")).color,
        Color::rgb(0xE8, 0xEB, 0xF1),
        "a word of the prose is not coloured"
    );
    // The body of the style block is coloured by the plugin that claims css, asked at the moment
    // of use — the same seam `colour_the_embedded` reads.
    assert_eq!(
        chars.style_at(inside("background-color")).color,
        Color::rgb(0xBD, 0x93, 0xF9),
        "the style block is css, so its property is a builtin"
    );
    assert_eq!(
        chars.style_at(inside("#ff79c6")).color,
        Color::rgb(0xFF, 0xB8, 0x6C),
        "the style block is css, so its hex colour is a number"
    );
    assert!(!harness.state().document().is_modified(), "colouring must not mark the file changed");
    harness.snapshot(shot("syntax_html"));
}

#[test]
fn the_plugins_page_lists_the_ones_that_ship_with_unluminous() {
    let mut harness = harness("");
    // A store of its own, because `CUSTOMISE` writes a plugin's folder out under it and a test must
    // not touch the settings of the person running it.
    let store = std::env::temp_dir().join("unluminous-plugins-page-store");
    let _ = std::fs::remove_dir_all(&store);
    harness.state_mut().use_store(unluminous_app::services::store::Store::at(&store));
    harness.state_mut().settings_window.open();
    harness.state_mut().settings_window.page = unluminous_app::settings::Page::Plugins;
    harness.run();
    harness.run();
    for name in ["CSS", "JavaScript", "TypeScript", "Rust", "HTML"] {
        harness.get_by_label(name);
    }
    harness.get_by_label("Marketplace");

    // **The button says what pressing it does.** `task-1795` reported `INSTALL` on a plugin that is
    // bundled, ticked and working, which is exactly what it reads as. Writing the folder out so it
    // can be edited is `CUSTOMISE`; `UNINSTALL` is what takes that folder away again.
    assert_eq!(harness.query_all_by_label("INSTALL").count(), 0, "nothing here is uninstalled");
    harness.get_by_label("CUSTOMISE").click();
    harness.run();
    harness.run();
    harness.get_by_label("UNINSTALL");
    assert_eq!(harness.query_all_by_label("CUSTOMISE").count(), 0, "it is on disk now");

    harness.get_by_label("UNINSTALL").click();
    harness.run();
    harness.run();
    harness.get_by_label("CUSTOMISE");
    assert!(
        harness.state().plugins.all().iter().any(|plugin| plugin.id == "agent-chat"),
        "uninstalling a bundled plugin puts it back to the one Unluminous ships rather than removing it",
    );
    harness.snapshot(shot("plugins_page"));
}

/// `task-1776`. The theme page lists what can be chosen, with each theme's own colours beside it, so
/// the choice is made by looking rather than by choosing a name and then seeing what happened.
#[test]
fn the_theme_page_lists_every_theme_with_the_colours_it_is_made_of() {
    let mut harness = harness("");
    harness.state_mut().settings_window.open();
    harness.state_mut().settings_window.page = unluminous_app::settings::Page::Theme;
    harness.run();
    harness.run();
    for name in [
        "Unluminous Dark",
        "Islands Dracula Colorful",
        "Material Deep Ocean",
        "Monokai Pro",
        "One Dark",
    ] {
        harness.get_by_label(name);
    }
    harness.get_by_label("Icon set");
    harness.get_by_label("The theme's own");
    harness.snapshot(shot("settings_theme_page"));
}

/// Choosing a theme repaints the whole window — the editing area, the rail, the explorer, the tabs and
/// the status bar — and recolours the code with the theme's own nine token colours.
#[test]
fn a_theme_repaints_the_window_and_recolours_the_code() {
    let mut harness = harness_in(&sample_folder());
    let opened = run(&mut harness, "tab open program.rs");
    assert!(opened.ok, "{}", opened.message);
    harness.run();
    harness.run();
    let before = harness.state().settings.theme.clone();
    assert!(before.is_empty(), "a window that has chosen nothing says nothing");

    // The file is coloured before the theme is chosen, in the Rust plugin's own Dracula.
    let keyword_at = harness
        .state()
        .document()
        .text()
        .to_string()
        .find("fn ")
        .expect("the sample program starts with a function");
    assert_eq!(
        harness.state().document().chars().style_at(keyword_at).color,
        unluminous_core::Color::rgb(0xFF, 0x79, 0xC6),
        "Dracula's pink, from the Rust plugin's own scheme"
    );

    let reply = run(&mut harness, "theme set \"Monokai Pro\"");
    assert!(reply.ok, "{}", reply.message);
    harness.run();
    assert_eq!(harness.state().settings.theme, "themes-bundle-1/monokai-pro");
    assert_eq!(
        unluminous_app::theme::color::editor(),
        egui::Color32::from_rgb(0x2D, 0x2A, 0x2E),
        "the palette followed"
    );
    // **The document itself, not the theme value.** A theme that changed the scheme but left every open
    // file coloured in the one before it is what the review on `task-1776` found, and asserting on
    // `theme::active().syntax` would not have caught it: `colour_the_file` is asked once a revision.
    assert_eq!(
        harness.state().document().chars().style_at(keyword_at).color,
        unluminous_core::Color::rgb(0xFF, 0x61, 0x88),
        "and the file that was already open was coloured again in Monokai's"
    );
    harness.snapshot(shot("themed_window"));
}

/// A theme is named on the command line by what is on the screen, and a name nothing answers to says
/// what there is rather than only that this was not one of them.
#[test]
fn a_theme_is_set_by_name_and_an_unknown_one_says_what_there_is() {
    let mut harness = harness("");
    let reply = run(&mut harness, "theme set Material Deep Ocean");
    assert!(reply.ok, "{}", reply.message);
    assert_eq!(harness.state().settings.theme, "themes-bundle-1/deep-ocean");

    let reply = run(&mut harness, "theme set Solarized");
    assert!(!reply.ok);
    assert!(reply.message.contains("Monokai Pro"), "it lists them: {}", reply.message);

    // `settings set` is the other way in, and it is the same change: `apply_the_theme` is the one place
    // a theme becomes one.
    let reply = run(&mut harness, "settings set appearance.theme themes-bundle-1/dracula");
    assert!(reply.ok, "{}", reply.message);
    harness.run();
    assert_eq!(
        unluminous_app::theme::color::accent(),
        egui::Color32::from_rgb(0xFF, 0x79, 0xC6),
        "the two routes land in the same place"
    );
}

/// The accent is one colour over whatever the theme said, and it reaches the roles that **are** the
/// accent rather than every role that happens to be blue.
#[test]
fn an_accent_is_set_over_the_theme_and_cleared_back_to_it() {
    let mut harness = harness("");
    run(&mut harness, "theme set unluminous/dark --accent #FF79C6");
    harness.run();
    assert_eq!(unluminous_app::theme::color::accent(), egui::Color32::from_rgb(0xFF, 0x79, 0xC6));
    assert_eq!(
        unluminous_app::theme::color::folder_open(),
        egui::Color32::from_rgb(0xFF, 0x79, 0xC6)
    );
    assert_eq!(
        unluminous_app::theme::color::editor(),
        egui::Color32::from_rgb(0x1A, 0x1F, 0x26),
        "and nothing that is merely blue moved"
    );

    run(&mut harness, "theme set unluminous/dark --accent none");
    harness.run();
    assert_eq!(unluminous_app::theme::color::accent(), egui::Color32::from_rgb(0x48, 0x9F, 0xF8));

    let reply = run(&mut harness, "theme set unluminous/dark --accent puce");
    assert!(!reply.ok);
    assert!(reply.message.contains("#RRGGBB"), "{}", reply.message);
}

/// Switching the themes plugin off puts the window back to Unluminous's own in the same frame, which is
/// `Plugins::renders`' rule applied to colour.
#[test]
fn switching_the_themes_plugin_off_puts_the_window_back() {
    let mut harness = harness("");
    run(&mut harness, "theme set \"Material Palenight\"");
    harness.run();
    assert_eq!(unluminous_app::theme::color::editor(), egui::Color32::from_rgb(0x29, 0x2D, 0x3E));

    run(&mut harness, "plugins disable themes-bundle-1");
    harness.run();
    assert_eq!(
        unluminous_app::theme::color::editor(),
        egui::Color32::from_rgb(0x1A, 0x1F, 0x26),
        "back to Unluminous Dark rather than left in a palette nothing can name"
    );
    // The setting is left alone, so switching the plugin back on brings the theme back with it.
    assert_eq!(harness.state().settings.theme, "themes-bundle-1/palenight");
    run(&mut harness, "plugins enable themes-bundle-1");
    harness.run();
    assert_eq!(unluminous_app::theme::color::editor(), egui::Color32::from_rgb(0x29, 0x2D, 0x3E));
}

/// The icon set follows the theme unless the settings name one, and the explorer's arrow is what it
/// is most visible in.
#[test]
fn the_icon_set_follows_the_theme_and_the_setting_wins() {
    let mut harness = harness("");
    assert_eq!(
        unluminous_app::theme::icons(),
        unluminous_app::theme::IconSet::Material,
        "a window comes up in the improved marks"
    );

    run(&mut harness, "theme set \"One Dark\"");
    harness.run();
    assert_eq!(
        unluminous_app::theme::icons(),
        unluminous_app::theme::IconSet::Classic,
        "One Dark names them"
    );

    run(&mut harness, "theme set \"One Dark\" --icons material");
    harness.run();
    assert_eq!(
        unluminous_app::theme::icons(),
        unluminous_app::theme::IconSet::Material,
        "and the setting wins"
    );

    run(&mut harness, "theme set \"One Dark\" --icons follow");
    harness.run();
    assert_eq!(
        unluminous_app::theme::icons(),
        unluminous_app::theme::IconSet::Classic,
        "and follow gives it back"
    );
}

/// `theme show` answers with the whole palette by name, so an agent can read a colour without a
/// screenshot, and `theme list` says which one is showing.
#[test]
fn theme_list_and_show_answer_in_a_payload_proportionate_to_the_question() {
    let mut harness = harness("");
    let listed = did(&mut harness, "theme list");
    let themes = listed["themes"].as_array().expect("a list of themes");
    assert_eq!(themes.len(), 6, "Unluminous's own and the bundle's five");
    assert_eq!(themes[0]["key"], "unluminous/dark");
    assert_eq!(themes[0]["active"], true);
    assert!(themes[0]["colours"]["accent"].is_string(), "six colours a theme is recognised by");
    assert!(
        themes[0]["colours"].as_object().expect("an object").len() <= 6,
        "not the whole palette"
    );

    let shown = did(&mut harness, "theme show \"Monokai Pro\"");
    assert_eq!(shown["icons"], "material");
    assert_eq!(shown["colours"]["editor"], "#2D2A2E");
    assert_eq!(shown["syntax"]["keyword"], "#FF6188");
    assert_eq!(
        shown["colours"].as_object().expect("an object").len(),
        unluminous_app::theme::Palette::NAMES.len(),
        "the whole palette, by the names a manifest sets"
    );
}

// Mermaid diagrams (`task-1660`).
//
// One image a diagram type, rendered through the real window and the graphics card. The parsers and
// the layout are tested in `unluminous-core` with no window at all, where the numbers can be checked by
// hand; these exist for the one thing that cannot be asserted — **what the picture looks like** —
// and every one of them was opened and looked at before it was accepted.
//
// The sources are the files in `sample-diagrams`, which are also what a person opens with
// `cargo run --release`. One set of samples rather than two, so the picture a test renders and the
// picture a person sees come from the same place.

/// The folder of sample diagrams, copied where a test can open them.
///
/// Written once per run behind a `OnceLock`, for the reason `sample_folder` already is: several of
/// these tests want it, they run at the same time, and one of them reading a file another was part
/// way through writing is a failure that has nothing to do with Unluminous.
///
/// **Not [`fixture`], because there is no list of files**: what it writes is whatever is in
/// `sample-diagrams`, copied out of the repository, which is also what a person opens.
fn diagram_folder() -> std::path::PathBuf {
    static FOLDER: OnceLock<std::path::PathBuf> = OnceLock::new();
    FOLDER
        .get_or_init(|| {
            let root = std::env::temp_dir().join("unluminous-mermaid-samples");
            std::fs::create_dir_all(&root).expect("make the folder");
            let from =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-diagrams");
            for entry in std::fs::read_dir(&from).expect("read sample-diagrams").flatten() {
                let path = entry.path();
                if path.is_file() {
                    let name = path.file_name().expect("a name");
                    std::fs::copy(&path, root.join(name)).expect("copy the sample");
                }
            }
            root
        })
        .clone()
}

/// Open one of the sample diagrams, in whichever view mode is wanted.
fn diagram_harness(name: &str, mode: ViewMode) -> Harness<'static, UnluminousApp> {
    let folder = diagram_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join(name)).expect("the file opens");
    harness.run();
    harness.state_mut().set_view_mode(mode);
    harness.run();
    harness
}

#[test]
fn every_diagram_type_is_drawn_in_the_real_window() {
    // Twenty images, one a diagram type. **Look at them**: this is the test that says an arrowhead
    // points the right way and that nothing overlaps anything, which no assertion about a scene
    // graph can tell you.
    let mut results = SnapshotResults::new();
    for name in [
        "flowchart",
        "sequence",
        "class",
        "state",
        "er",
        "requirement",
        "pie",
        "gantt",
        "journey",
        "gitgraph",
        "mindmap",
        "timeline",
        "quadrant",
        "xychart",
        "sankey",
        "block",
        "packet",
        "kanban",
        "radar",
        "treemap",
    ] {
        let mut harness = diagram_harness(&format!("{name}.mmd"), ViewMode::Preview);
        assert!(
            harness.query_by_label(&format!("Diagram: {name}.mmd")).is_some(),
            "{name} should have drawn a diagram"
        );
        results.add(harness.try_snapshot(shot(&format!("mermaid_{name}"))));
    }
    report(results);
}

#[test]
fn a_mermaid_file_gets_the_three_view_modes_named_after_what_it_is() {
    let mut harness = diagram_harness("flowchart.mmd", ViewMode::Raw);
    // The words say Mermaid, not Markdown. A button over a diagram that said `Markdown preview`
    // would be a small wrongness a reader notices at once.
    for name in ["Raw Mermaid", "Side by side", "Mermaid diagram"] {
        assert!(harness.query_by_label(name).is_some(), "{name} should be there");
    }
    assert!(
        harness.query_by_label("Raw Markdown").is_none(),
        "and the Markdown wording should not be"
    );
    // The `F` is absent: a diagram is not prose, so bold and a line spacing mean nothing in it.
    assert!(
        harness.query_by_label("Text options").is_none(),
        "a diagram has no formatting to offer"
    );
    harness.snapshot(shot("mermaid_view_raw"));
}

#[test]
fn the_three_view_mode_buttons_switch_a_mermaid_file_between_the_modes() {
    let mut harness = diagram_harness("pie.mmd", ViewMode::Raw);
    for (name, expected) in [
        ("Side by side", ViewMode::SideBySide),
        ("Mermaid diagram", ViewMode::Preview),
        ("Raw Mermaid", ViewMode::Raw),
    ] {
        harness.get_by_label(name).click();
        harness.run();
        assert_eq!(harness.state().view_mode(), expected, "clicking {name} should switch to it");
    }
}

#[test]
fn side_by_side_shows_a_mermaid_source_and_its_diagram_at_once() {
    let mut harness = diagram_harness("state.mmd", ViewMode::Raw);
    let whole = harness.state().editor_area().width();
    harness.get_by_label("Side by side").click();
    harness.run();
    let half = harness.state().editor_area().width();
    assert!(half < whole, "the source gives up half its width: {whole} then {half}");
    assert!(
        harness.query_by_label("Diagram: state.mmd").is_some(),
        "the diagram is drawn beside it"
    );
    harness.snapshot(shot("mermaid_side_by_side"));
}

#[test]
fn a_diagram_that_will_not_parse_says_which_line_rather_than_drawing_nothing() {
    let folder = diagram_folder();
    let broken = folder.join("broken.mmd");
    std::fs::write(&broken, "flowchart LR\n  A --> B\n  C[never closed --> D\n")
        .expect("write the broken sample");
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&broken).expect("the file opens");
    harness.state_mut().set_view_mode(ViewMode::Preview);
    harness.run();
    harness.snapshot(shot("mermaid_problem"));
}

#[test]
fn a_diagram_type_unluminous_does_not_draw_is_named_rather_than_left_blank() {
    let folder = diagram_folder();
    let path = folder.join("wardley.mmd");
    std::fs::write(&path, "wardley\n  title A value chain\n  anchor Customer [0.9, 0.8]\n")
        .expect("write the sample");
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&path).expect("the file opens");
    harness.state_mut().set_view_mode(ViewMode::Preview);
    harness.run();
    harness.snapshot(shot("mermaid_not_drawn"));
}

#[test]
fn mermaid_blocks_in_a_markdown_file_are_drawn_in_its_preview() {
    let folder = diagram_folder();
    let mut harness = harness_in(&folder);
    harness
        .state_mut()
        .open_path_permanently(&folder.join("in-markdown.md"))
        .expect("the file opens");
    harness.state_mut().set_view_mode(ViewMode::Preview);
    harness.run();

    let diagrams = harness.state().preview_diagrams();
    assert_eq!(diagrams.len(), 3, "two that draw and one that cannot");
    assert!(diagrams[0].laid.is_ok(), "the flowchart draws");
    assert!(diagrams[1].laid.is_ok(), "the pie draws");
    assert!(diagrams[2].laid.is_err(), "the one with the unclosed bracket does not");
    // Each one's paragraph was made tall enough to hold it, which is the whole of the two-pass
    // arrangement working.
    for diagram in diagrams {
        assert!(diagram.size.y > 0.0, "a diagram with no height would be invisible");
    }
    // The `rust` fence is still code: only `mermaid` is drawn.
    assert!(
        harness.state().preview_text().contains("still code"),
        "an ordinary code fence keeps its text"
    );
    harness.snapshot(shot("mermaid_in_markdown"));
}

#[test]
fn switching_the_mermaid_plugin_off_withdraws_the_diagrams() {
    // The whole reason this is a plugin rather than a feature: turning it off has to actually take
    // the feature away, in the same frame, in both of the places it appears.
    let folder = diagram_folder();
    let mut harness = harness_in(&folder);
    harness
        .state_mut()
        .open_path_permanently(&folder.join("in-markdown.md"))
        .expect("the file opens");
    harness.state_mut().set_view_mode(ViewMode::Preview);
    harness.run();
    assert_eq!(harness.state().preview_diagrams().len(), 3);
    assert!(harness.state().mermaid_is_enabled());

    harness.state_mut().set_plugin_enabled("mermaid", false);
    harness.run();
    assert!(!harness.state().mermaid_is_enabled());
    assert!(
        harness.state().preview_diagrams().is_empty(),
        "with the plugin off, a mermaid fence is code again"
    );

    // And a `.mmd` file says so rather than drawing.
    harness.state_mut().open_path_permanently(&folder.join("pie.mmd")).expect("the file opens");
    harness.state_mut().set_view_mode(ViewMode::Preview);
    harness.run();
    assert!(
        harness.query_by_label("Diagram: pie.mmd").is_none(),
        "no diagram is drawn while the plugin is off"
    );
    harness.snapshot(shot("mermaid_plugin_off"));
}

#[test]
fn a_diagram_is_laid_out_once_however_many_frames_it_is_drawn_for() {
    // A preview is redrawn sixty times a second, so laying a diagram out on every frame would be
    // sixty layouts a second for a picture that has not changed.
    let folder = diagram_folder();
    let mut harness = harness_in(&folder);
    harness
        .state_mut()
        .open_path_permanently(&folder.join("flowchart.mmd"))
        .expect("the file opens");
    harness.state_mut().set_view_mode(ViewMode::Preview);
    harness.run();
    let after_first = harness.state().mermaid_scene_count();
    for _ in 0..5 {
        harness.run();
    }
    assert_eq!(
        harness.state().mermaid_scene_count(),
        after_first,
        "drawing it again should not lay it out again"
    );
    assert_eq!(after_first, 1, "one diagram, one scene");
}

#[test]
fn the_command_line_can_read_what_a_diagram_came_out_as() {
    // `task-1661` asks that every feature be reachable from the command line. A picture cannot be
    // sent down a socket, so what comes back is what it is, how large it came out and every word in
    // it — which is enough for a script to tell that the right diagram was drawn.
    let folder = diagram_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("pie.mmd")).expect("the file opens");
    harness.run();
    let answer = did(&mut harness, "editor preview");
    assert_eq!(answer["diagram"], "pie", "{answer}");
    assert!(answer["width"].as_f64().unwrap_or(0.0) > 0.0);
    let text = answer["text"].to_string();
    assert!(text.contains("Where the work went"), "it reads the words back: {text}");
}

/// The counterpart of `cargo run --example mermaid_check`, run automatically as part of the suite
/// rather than left for somebody to remember to run by hand.
///
/// It walks the same `sample-diagrams` folder the example reads and calls the same
/// `unluminous_core::mermaid::render`, through the same `FixedMetrics` stub the layout tests use, so it
/// needs no window and no graphics card. Reading that folder from a test is fine here in a way a test
/// must not usually be: the files in it are checked into this repository, so they are the same on
/// every machine that runs the suite, unlike a person's own settings, projects or fonts, which would
/// answer differently from one machine to the next.
///
/// Every refusal is collected rather than the first one stopping the test, so a change that breaks
/// two diagram types is reported as two rather than one at a time. The folder is also asserted
/// non-empty, so a walk that silently found nothing cannot pass as though every diagram drew.
///
/// This does not also run `mermaid::check::properties` on each scene. That function's last check
/// wants the words a diagram's own labels should hold, which for an arbitrary sample means reading
/// the source and pulling its labels back out by hand for each diagram type — exactly the parsing
/// `mermaid::render` has already done once. The properties are exercised with real wanted labels by
/// every renderer's own tests in `unluminous-core`; what this test adds is the same drawing check
/// `mermaid_check` runs, over the whole folder, so it stays a test rather than a second copy of those.
#[test]
fn every_sample_diagram_lays_out_with_no_refusal() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample-diagrams");
    let metrics = unluminous_core::metrics::FixedMetrics::default();
    let options = unluminous_core::mermaid::Options::new(&metrics);

    let mut names: Vec<std::path::PathBuf> = std::fs::read_dir(&folder)
        .unwrap_or_else(|problem| panic!("read {}: {problem}", folder.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "mmd"))
        .collect();
    names.sort();
    assert!(!names.is_empty(), "found no .mmd files in {}", folder.display());

    let mut refusals = Vec::new();
    for path in &names {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let source = std::fs::read_to_string(path).expect("read the sample");
        if let Err(problem) = unluminous_core::mermaid::render(&source, &options) {
            refusals.push(format!("{name}: {}", problem.message()));
        }
    }
    assert!(
        refusals.is_empty(),
        "{} of {} sample diagrams refused to draw:\n{}",
        refusals.len(),
        names.len(),
        refusals.join("\n")
    );
}
