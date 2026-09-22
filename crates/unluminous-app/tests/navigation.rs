//! Finding your way round a project: the split view, definitions, references, rename, find and
//! replace, imports, completion, folding, and moving a file with the code that names it.
//!
//! The explorer following the tab, the editing area cut into panes each with its own tabs, go to
//! definition and find all references and rename a symbol, the find bar and Replace All, the list
//! that opens inside an import and the one that opens under a half typed word, collapsing and
//! expanding a block, and deleting or moving a file so that every import naming it is rewritten.
//!
//! What is underneath each of these is tested with no window — `unluminous_core::symbols`,
//! `unluminous_core::completion`, `unluminous_core::imports`, `unluminous_core::folding`,
//! `services::file_move` — so what is here is what only a real window can show, and what the command
//! line reaches.
//!
//! **21 of the 61 tests here take a picture**, and the rest read the window's state back.

mod common;

use common::*;

use egui::Modifiers;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::app::actions::{Action, FoldAction};
use unluminous_app::UnluminousApp;
use unluminous_core::Command;

#[test]
fn the_explorer_opens_out_the_folders_above_the_file_that_is_showing_and_scrolls_to_it() {
    // The file is two folders down and both of them start shut, so before `task-1664` there was no
    // row to select at all. Opening it should open `chapters` and `chapters/appendix` and leave the
    // row on the screen.
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness
        .state_mut()
        .open_path_permanently(&folder.join("chapters/appendix/tables.txt"))
        .expect("the file opens");
    steady(&mut harness);
    assert_eq!(harness.state().files.active().name(), "tables.txt");
    let open = harness.state().tree.expanded_folders();
    assert!(open.contains(&folder.join("chapters")), "chapters should be open, and {open:?} is");
    assert!(
        open.contains(&folder.join("chapters/appendix")),
        "appendix should be open, and {open:?} is"
    );
    // The row exists and is drawn as the selected one, which is what the picture is of.
    harness.get_by_label("tables.txt");
    harness.snapshot(shot("explorer_follows_the_tab"));
}

#[test]
fn a_folder_shut_by_hand_is_not_opened_again_until_the_tab_changes() {
    // The reveal is a one shot. A person who shut the folder holding the open file shut it on
    // purpose, and a reveal that ran every frame would open it again before the pointer was up.
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness
        .state_mut()
        .open_path_permanently(&folder.join("chapters/one.md"))
        .expect("the file opens");
    steady(&mut harness);
    assert!(harness.state().tree.expanded_folders().contains(&folder.join("chapters")));
    harness.state_mut().tree.toggle(&folder.join("chapters"));
    steady(&mut harness);
    steady(&mut harness);
    assert!(
        !harness.state().tree.expanded_folders().contains(&folder.join("chapters")),
        "the folder should have stayed shut"
    );
    // Showing a different file and then this one again is a change, so it is revealed again.
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    steady(&mut harness);
    harness
        .state_mut()
        .open_path_permanently(&folder.join("chapters/one.md"))
        .expect("the file opens");
    steady(&mut harness);
    assert!(
        harness.state().tree.expanded_folders().contains(&folder.join("chapters")),
        "asking for the file again opens the folder again"
    );
}

#[test]
fn splitting_a_tab_puts_two_files_side_by_side() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    harness.state_mut().open_path_permanently(&folder.join("program.rs")).expect("the file opens");
    steady(&mut harness);
    assert_eq!(harness.state().files.pane_count(), 1);

    choose(&mut harness, Action::SplitRight);
    assert_eq!(harness.state().files.pane_count(), 2);
    assert_eq!(harness.state().files.focused_pane(), 1);
    // One file in each pane, and each pane has a tab strip of its own.
    assert_eq!(harness.state().files.tabs_in(0).len(), 1);
    assert_eq!(harness.state().files.tabs_in(1).len(), 1);
    assert_eq!(harness.state().files.active().name(), "program.rs");
    harness.snapshot(shot("split_two_panes"));
}

#[test]
fn three_panes_each_show_a_file_of_their_own() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    for name in ["readme.md", "notes.txt", "program.rs"] {
        harness.state_mut().open_path_permanently(&folder.join(name)).expect("the file opens");
    }
    steady(&mut harness);
    choose(&mut harness, Action::SplitRight);
    // The second split is on the pane that still holds two tabs, which is the one on the left.
    harness.state_mut().files.focus_pane(0);
    choose(&mut harness, Action::SplitRight);
    assert_eq!(harness.state().files.pane_count(), 3);
    for pane in 0..3 {
        assert_eq!(harness.state().files.tabs_in(pane).len(), 1, "pane {pane}");
    }
    harness.snapshot(shot("split_three_panes"));
}

#[test]
fn a_tabs_own_menu_offers_the_splits() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    harness.state_mut().open_path_permanently(&folder.join("program.rs")).expect("the file opens");
    steady(&mut harness);
    // Opened through the window's own state, as the gutter's menu is, because the harness cannot
    // press the right mouse button.
    harness.state_mut().tab_menu = Some((egui::pos2(360.0, 96.0), 0));
    steady(&mut harness);
    harness.get_by_label("Split Right");
    harness.get_by_label("Unsplit All");
    harness.snapshot(shot("tab_menu"));
}

#[test]
fn only_the_pane_with_the_keyboard_takes_what_is_typed() {
    // The one fault the pane loop invites: `files.active()` answers with the pane being drawn while
    // it is being drawn, so without the keyboard being passed in separately every pane would take
    // the same key presses and draw a caret.
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    harness.state_mut().open_path_permanently(&folder.join("notes.txt")).expect("the file opens");
    steady(&mut harness);
    choose(&mut harness, Action::SplitRight);
    let left = harness.state().files.tabs_in(0)[0];
    let before = harness.state().files.at(left).document.text().to_string();

    harness.input_mut().events.push(egui::Event::Text("typed".to_owned()));
    steady(&mut harness);
    assert_eq!(
        harness.state().files.at(left).document.text().to_string(),
        before,
        "the pane without the keyboard should not have taken the text"
    );
    assert!(
        harness.state().files.active().document.text().to_string().contains("typed"),
        "the pane with the keyboard should have"
    );
}

#[test]
fn each_pane_lays_its_own_file_out_at_its_own_width() {
    // The reason the ten cache fields moved onto the tab. With one cache on the window the two panes
    // would lay their files out over each other every frame, and neither would be at the right width.
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    harness.state_mut().open_path_permanently(&folder.join("program.rs")).expect("the file opens");
    steady(&mut harness);
    choose(&mut harness, Action::SplitRight);
    // Two panes of unequal width, so one cache could not be right for both.
    harness.state_mut().files.set_pane_width(0, 0.3);
    steady(&mut harness);
    steady(&mut harness);
    let left = harness.state().files.tabs_in(0)[0];
    let right = harness.state().files.tabs_in(1)[0];
    let narrow = harness.state().files.at(left).cached.laid_out_width;
    let wide = harness.state().files.at(right).cached.laid_out_width;
    assert!(narrow > 0.0 && wide > 0.0, "both panes laid their file out: {narrow} and {wide}");
    assert!(wide > narrow, "the wider pane laid out at a greater width: {wide} against {narrow}");
}

#[test]
fn unsplitting_brings_every_tab_back_into_one_pane() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    harness.state_mut().open_path_permanently(&folder.join("notes.txt")).expect("the file opens");
    steady(&mut harness);
    choose(&mut harness, Action::SplitRight);
    assert_eq!(harness.state().files.pane_count(), 2);
    choose(&mut harness, Action::UnsplitAll);
    assert_eq!(harness.state().files.pane_count(), 1);
    assert_eq!(harness.state().files.tabs_in(0).len(), 2);
}

#[test]
fn the_command_line_splits_the_editing_area_and_says_what_it_did() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open readme.md --permanent");
    did(&mut harness, "tab open program.rs --permanent");

    let result = did(&mut harness, "pane split");
    assert_eq!(result["count"].as_u64(), Some(2));
    assert_eq!(harness.state().files.pane_count(), 2);

    let result = did(&mut harness, "pane list");
    assert_eq!(result["count"].as_u64(), Some(2), "pane list should say there are two");
    assert_eq!(result["focused"].as_u64(), Some(1));

    // A pane that is not there is refused rather than clamped, so a script is told.
    assert_eq!(refused(&mut harness, "pane focus 9"), "not-found");
    assert_eq!(refused(&mut harness, "pane move sideways"), "usage");

    did(&mut harness, "pane move left");
    assert_eq!(harness.state().files.pane_count(), 1, "the pane it left was emptied");
    assert_eq!(refused(&mut harness, "pane unsplit"), "not-applicable");
}

#[test]
fn the_command_line_scrolls_the_explorer_to_the_file_that_is_showing() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open chapters/appendix/tables.txt --permanent");
    // Shut the folders again and put the explorer away, so the command has something to do.
    harness.state_mut().tree.toggle(&folder.join("chapters"));
    harness.state_mut().explorer_visible = false;
    steady(&mut harness);

    did(&mut harness, "explorer select-open-file");
    assert!(harness.state().explorer_visible, "it shows the explorer if it was put away");
    assert!(
        harness.state().tree.expanded_folders().contains(&folder.join("chapters/appendix")),
        "and opens the folders above the file"
    );
}

#[test]
fn a_split_project_opens_split_again() {
    // The whole round trip through `.unluminous`, on a folder of its own so that no other test's window
    // is reading or writing the same state file.
    let folder =
        copy_out_of_the_repository(&sample_folder(), "unluminous-screenshot-split-project");
    {
        let mut harness = harness_in(&folder);
        harness.state_mut().restore_project();
        harness
            .state_mut()
            .open_path_permanently(&folder.join("readme.md"))
            .expect("the file opens");
        harness
            .state_mut()
            .open_path_permanently(&folder.join("program.rs"))
            .expect("the file opens");
        steady(&mut harness);
        let ctx = harness.ctx.clone();
        harness.state_mut().run_action(Action::SplitRight, &ctx);
        // Written on the frame after the change, as every other piece of project state is.
        steady(&mut harness);
        steady(&mut harness);
        assert_eq!(harness.state().files.pane_count(), 2);
    }
    let mut second = harness_in(&folder);
    second.state_mut().restore_project();
    steady(&mut second);
    assert_eq!(second.state().files.pane_count(), 2, "the split should have come back");
    assert_eq!(second.state().files.tabs_in(0).len(), 1);
    assert_eq!(second.state().files.tabs_in(1).len(), 1);
    std::fs::remove_dir_all(&folder).ok();
}

/// A small project written in a language that says what a definition is.
fn code_folder() -> std::path::PathBuf {
    fixture("unluminous-screenshot-code", &[
        ("layout.rs", "//! Laying a document out.\n\npub struct Layout;\n\nimpl Layout {\n    pub fn new() -> Self {\n        Layout\n    }\n\n    /// Draw the whole of it.\n    pub fn draw(&self) {\n        let label = \"draw\";\n        let _ = label;\n    }\n}\n"),
        ("caret.rs", "//! The caret, and how it is drawn.\n\npub struct Caret;\n\nimpl Caret {\n    pub fn new() -> Self {\n        Caret\n    }\n\n    // draw the caret over the text\n    pub fn paint(&self, layout: &Layout) {\n        layout.draw();\n        layout.draw();\n    }\n}\n"),
        ("panel.rs", "pub struct Panel;\n\nimpl Panel {\n    pub fn show(&self, layout: &Layout) {\n        layout.draw();\n    }\n}\n"),
        ("notes.md", "# Notes\n\nA note that mentions draw.\n"),
    ])
}

/// A window on the code folder, with its definitions index built and the named file open.
fn code_harness(open: &str) -> Harness<'static, UnluminousApp> {
    let folder = code_folder();
    let mut harness = harness_in(&folder);
    let path = folder.join(open);
    harness.state_mut().open_path_permanently(&path).expect("the file opens");
    // The index is read on a thread, exactly as git and the text search are, so the harness is run
    // until the answer arrives. Each run is a frame; nothing here waits on a clock.
    for _ in 0..600 {
        pump(&mut harness);
        let ready = harness
            .state()
            .symbols_indexer()
            .is_some_and(|indexer| !indexer.is_building() && !indexer.index().is_empty());
        if ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    steady(&mut harness);
    harness
}

/// Put the caret on the first `needle` in the file that is showing, a little way into the word.
fn caret_on(harness: &mut Harness<'static, UnluminousApp>, needle: &str, into: usize) -> usize {
    let text = harness.state().document().text().to_string();
    let at = text.find(needle).unwrap_or_else(|| panic!("{needle} is not in this file")) + into;
    harness.state_mut().command(Command::PlaceCaret { offset: at, extend: false });
    steady(harness);
    at
}

/// Wait for the references modal's own search to finish.
fn settle_the_references(harness: &mut Harness<'static, UnluminousApp>) {
    for _ in 0..400 {
        let searching = harness
            .state()
            .references
            .as_ref()
            .is_some_and(unluminous_app::components::references::References::is_searching);
        if !searching {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.step();
    }
    steady(harness);
}

/// What the references modal found, as `name:line · role` for each row.
fn references(harness: &Harness<'static, UnluminousApp>) -> Vec<String> {
    harness
        .state()
        .references
        .as_ref()
        .expect("the modal should be open")
        .hits()
        .iter()
        .map(|hit| {
            format!(
                "{}:{}{}",
                hit.path.file_name().unwrap().to_string_lossy(),
                hit.line,
                match hit.role {
                    unluminous_core::symbols::Role::Code => String::new(),
                    other => format!(" \u{00B7} {}", other.suffix()),
                }
            )
        })
        .collect()
}

#[test]
fn go_to_definition_jumps_to_the_definition_and_selects_its_name() {
    // Scenario 1, through the real window: the caret is on a call, and the definition is what ends
    // up selected.
    let mut harness = code_harness("caret.rs");
    let ctx = harness.ctx.clone();
    caret_on(&mut harness, "layout.draw()", "layout.".len() + 1);
    harness.state_mut().run_action(Action::GoToDefinition, &ctx);
    steady(&mut harness);
    assert_eq!(harness.state().files.active().name(), "layout.rs");
    assert_eq!(harness.state().document().selected_text(), "draw");
}

#[test]
fn the_modifier_underlines_the_word_it_would_go_to_and_nothing_else() {
    // Scenario 7. The affordance is resolution-driven: only a word that really has somewhere to go
    // is underlined, so the promise it makes is one the click can keep.
    let mut harness = code_harness("caret.rs");
    let text = harness.state().document().text().to_string();
    let call = text.find("layout.draw()").expect("the call") + "layout.".len() + 1;
    // `layout` is a parameter, which no definer keyword declares. Since `task-2063` it resolves to
    // where it is first written in its function, the parameter list, rather than to nothing.
    let parameter = text.find("layout.draw()").expect("the call") + 1;

    assert!(
        harness.state_mut().resolve_under_the_pointer(call).is_some(),
        "`draw` is defined in layout.rs, so it resolves"
    );
    harness.state_mut().forget_the_hover();
    let hover = harness
        .state_mut()
        .resolve_under_the_pointer(parameter)
        .expect("`layout` resolves to its parameter");
    let written = text.find("layout: &Layout").expect("the parameter");
    assert_eq!(hover.candidates[0].name_range, written..written + "layout".len());
    // A definition of its own still resolves, because the click there means something: it pivots
    // to the references, which is scenario 8.
    harness.state_mut().forget_the_hover();
    let definition = text.find("fn paint").expect("paint") + 4;
    let hover = harness
        .state_mut()
        .resolve_under_the_pointer(definition)
        .expect("its own definition resolves");
    assert!(hover.at_definition, "and the window knows it is standing on it");
    // And a keyword is not a question about a symbol at all.
    harness.state_mut().forget_the_hover();
    let keyword = text.find("-> Self").expect("Self") + 4;
    assert!(harness.state_mut().resolve_under_the_pointer(keyword).is_none());
}

#[test]
fn asking_from_the_definition_opens_the_references_instead() {
    // Scenario 8, and the picture of the modal the ticket describes: the results grouped by file
    // with a count on each heading, and the file the chosen reference is in shown underneath,
    // scrolled to it with the reference picked out.
    let mut harness = code_harness("layout.rs");
    let ctx = harness.ctx.clone();
    caret_on(&mut harness, "fn draw", 4);
    harness.state_mut().run_action(Action::GoToDefinition, &ctx);
    settle_the_references(&mut harness);
    let found = references(&harness);
    assert!(found.contains(&"layout.rs:11".to_owned()), "the definition itself: {found:?}");
    assert!(found.contains(&"caret.rs:12".to_owned()), "and the calls: {found:?}");
    assert!(
        found.iter().any(|row| row.ends_with("comment")),
        "the mention in a comment is listed, second-class: {found:?}"
    );
    assert!(
        found.iter().any(|row| row.ends_with("string")),
        "and so is the one inside a string: {found:?}"
    );
    // The headings and one row of each kind name themselves, which is how a test finds them.
    harness.get_by_label(&references_heading("layout.rs"));
    harness.get_by_label("Reference layout.rs:11");
    harness.snapshot(shot("references"));
}

/// The label the references modal gives a file's heading.
///
/// The modal names the file the way the platform spells a path, so the label has a backslash in it on
/// Windows and a slash everywhere else. Written down twice as a literal, these two tests passed on
/// Windows and failed on macOS for a reason that has nothing to do with what they are testing.
fn references_heading(file: &str) -> String {
    let path = std::path::Path::new("unluminous-screenshot-code").join(file);
    format!("References in {}", path.display())
}

#[test]
fn choosing_a_file_heading_shows_that_files_first_reference() {
    // Scenario 21, which is the ticket's own sentence: *a modal that has the file path, then under
    // that scrolled to the first reference in that file*.
    let mut harness = code_harness("layout.rs");
    let ctx = harness.ctx.clone();
    caret_on(&mut harness, "fn draw", 4);
    harness.state_mut().run_action(Action::FindReferences, &ctx);
    settle_the_references(&mut harness);
    harness.get_by_label(&references_heading("caret.rs")).click();
    steady(&mut harness);
    steady(&mut harness);
    let (path, line) =
        harness.state().references.as_ref().expect("the modal").scrolled_to().expect("somewhere");
    assert_eq!(path.file_name().unwrap(), "caret.rs");
    // The first reference *in the list*, which within a file is the first code one: the textual
    // matches are listed after them, so a heading previews the answer rather than a mention of it.
    assert_eq!(line, 12, "the call on line 12, not the comment above it on line 10");
}

#[test]
fn opening_a_reference_selects_it_in_the_document() {
    // Scenario 30. The same contract `Find in Files` has: enter or a double click opens it and the
    // modal closes.
    let mut harness = code_harness("layout.rs");
    let ctx = harness.ctx.clone();
    caret_on(&mut harness, "fn draw", 4);
    harness.state_mut().run_action(Action::FindReferences, &ctx);
    settle_the_references(&mut harness);
    double_click(&mut harness, "Reference caret.rs:12");
    assert!(harness.state().references.is_none(), "opening a reference shuts the modal");
    assert_eq!(harness.state().files.active().name(), "caret.rs");
    assert_eq!(harness.state().document().selected_text(), "draw");
}

// task-1804 §3.1: Find and Replace in the file that is showing.

/// `Ctrl+F` opens the bar, the tally counts, and every other match carries a band.
///
/// The picture is the point of this one: the bar in the corner with `1 of N` on it, the current
/// match as the selection, and the rest of them in `find_match`. Those three being told apart by
/// eye is the whole of what the colour was added for.
#[test]
fn the_find_bar_counts_the_matches_and_marks_every_one_of_them() {
    let mut harness = code_harness("layout.rs");
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Find, &ctx);
    steady(&mut harness);
    let find = harness.state().find.as_ref().expect("the bar is open");
    assert!(!find.replacing, "Ctrl+F opens it without the Replace row");

    harness.state_mut().find.as_mut().expect("the bar").needle = "draw".to_owned();
    steady(&mut harness);
    let find = harness.state().find.as_ref().expect("the bar");
    assert!(find.count() > 1, "layout.rs holds several of them: {}", find.count());
    assert_eq!(find.index(), Some(1), "the bar starts on the first");
    assert_eq!(find.tally().as_deref(), Some(format!("1 of {}", find.count()).as_str()));
    harness.get_by_label("Find text");
    harness.get_by_label("Next match");
    harness.snapshot(shot("find_bar"));

    // Next walks forward and wraps, and the current match is **selected**, which is what makes
    // Escape leave the caret on it and Ctrl+C copy it.
    let total = harness.state().find.as_ref().expect("the bar").count();
    harness.state_mut().run_action(Action::FindNext, &ctx);
    steady(&mut harness);
    assert_eq!(harness.state().find.as_ref().expect("the bar").index(), Some(2));
    assert_eq!(harness.state().document().selected_text().to_lowercase(), "draw");
    for _ in 1..total {
        harness.state_mut().run_action(Action::FindNext, &ctx);
    }
    steady(&mut harness);
    assert_eq!(
        harness.state().find.as_ref().expect("the bar").index(),
        Some(1),
        "past the last one it wraps round to the first"
    );

    // And Escape puts it away, leaving the caret where it stopped.
    let stopped = harness.state().document().selection().start();
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert!(harness.state().find.is_none());
    assert_eq!(harness.state().document().selection().start(), stopped);
}

/// `Ctrl+H` opens the same bar with the Replace row on it, and Replace All is one undo step.
///
/// The undo is the assertion worth having: replacing forty occurrences one at a time would be forty
/// presses of `Ctrl+Z` to get back, and `Command::ReplaceMany` is what makes it one.
#[test]
fn replace_all_changes_every_match_and_one_undo_puts_them_back() {
    let mut harness = code_harness("layout.rs");
    let ctx = harness.ctx.clone();
    let before = harness.state().document().text().to_string();
    harness.state_mut().run_action(Action::Replace, &ctx);
    steady(&mut harness);
    assert!(harness.state().find.as_ref().expect("the bar").replacing, "Ctrl+H opens the row");

    {
        let find = harness.state_mut().find.as_mut().expect("the bar");
        find.needle = "draw".to_owned();
        find.replacement = "paint".to_owned();
    }
    steady(&mut harness);
    let total = harness.state().find.as_ref().expect("the bar").count();
    assert!(total > 1, "there are several to replace: {total}");
    harness.get_by_label("Replace with");
    harness.snapshot(shot("replace_bar"));

    // Through the button, because that is what a person presses.
    harness.get_by_label("Replace all").click();
    steady(&mut harness);
    let after = harness.state().document().text().to_string();
    assert_ne!(after, before);
    assert_eq!(
        after.matches("paint").count(),
        before.matches("paint").count() + total,
        "every one of them was replaced"
    );

    // One undo, not `total` of them.
    harness.state_mut().command(unluminous_core::Command::Undo);
    steady(&mut harness);
    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "one undo puts the whole replacement back"
    );
}

/// The same two things through the command line, which is where an agent reaches them.
///
/// `editor find` opens **the bar a person opens**, which is the rule the whole product rests on: the
/// reply and the window say the same thing, so a screenshot taken after the command shows it.
#[test]
fn an_agent_finds_and_replaces_through_the_same_bar_a_person_uses() {
    let mut harness = code_harness("layout.rs");

    let found = did(&mut harness, "editor find draw --json");
    let total = found["total"].as_u64().expect("a total");
    assert!(total > 1, "several matches: {total}");
    assert_eq!(found["current"], serde_json::json!(1));
    assert!(harness.state().find.is_some(), "it opened the bar a person opens");
    // Lower cased, because the search ignores case unless told otherwise -- so the first match in
    // this file is `Draw` in a comment, and that is the right answer rather than a near miss.
    assert_eq!(harness.state().document().selected_text().to_lowercase(), "draw");
    // The payload is proportionate: fifty rows unless more are asked for, and the count either way.
    assert!(found["matches"].as_array().expect("the rows").len() <= 50);

    let stepped = did(&mut harness, "editor find --next --json");
    assert_eq!(stepped["current"], serde_json::json!(2));

    // Whole word narrows it, and the flags stay set across a later call that does not name them.
    let word = did(&mut harness, "editor find draw --whole-word --json");
    assert_eq!(word["wholeWord"], serde_json::json!(true));
    assert!(word["total"].as_u64().expect("a total") <= total);

    did(&mut harness, "editor find --close");
    assert!(harness.state().find.is_none());

    // Replace says what it *would* do and changes nothing without --apply, which is `editor rename`'s
    // shape and is here for the same reason.
    let before = harness.state().document().text().to_string();
    let dry = did(&mut harness, "editor replace draw paint --all --json");
    assert_eq!(dry["applied"], serde_json::json!(false));
    assert!(dry["wouldChange"].as_u64().expect("a count") > 1);
    assert_eq!(harness.state().document().text().to_string(), before, "nothing changed");

    let applied = did(&mut harness, "editor replace draw paint --all --apply --json");
    assert_eq!(applied["applied"], serde_json::json!(true));
    assert_eq!(applied["remaining"], serde_json::json!(0));
    assert_ne!(harness.state().document().text().to_string(), before);

    // And a needle that is not there is a refusal rather than a cheerful nothing -- the rule
    // `task-1804` §7.2 applied to this command.
    assert_eq!(refused(&mut harness, "editor replace zebra horse --all"), "not-found");
}

#[test]
fn the_rename_modal_is_the_preview_and_the_ticks_are_the_change_set() {
    // Scenarios 34, 38 and 39 in one picture: the field pre-filled, a tick on every row, the
    // project-wide default for a function, and the footer saying what is wrong with a bad name.
    let mut harness = code_harness("layout.rs");
    let ctx = harness.ctx.clone();
    caret_on(&mut harness, "fn draw", 4);
    harness.state_mut().run_action(Action::RenameSymbol, &ctx);
    settle_the_references(&mut harness);
    let modal = harness.state().references.as_ref().expect("the rename modal");
    assert_eq!(modal.new_name, "draw", "the field starts as the name it is about");
    let ticked: Vec<bool> = modal.ticks().to_vec();
    let roles: Vec<unluminous_core::symbols::Role> =
        modal.hits().iter().map(|hit| hit.role).collect();
    for (ticked, role) in ticked.iter().zip(&roles) {
        match role {
            unluminous_core::symbols::Role::Code => {
                assert!(ticked, "a function is renamed across the project by default")
            }
            _ => assert!(!ticked, "a comment or a string is never ticked by default"),
        }
    }
    harness.get_by_label("New name");
    harness.snapshot(shot("rename_symbol"));

    // A name this language could not hold is refused, with the reason in the footer.
    harness.state_mut().references.as_mut().expect("the modal").new_name = "match".to_owned();
    steady(&mut harness);
    let refusal =
        harness.state().references.as_ref().expect("the modal").refusal.clone().expect("a refusal");
    assert!(refusal.contains("keyword"), "{refusal}");

    // And a collision is a warning rather than a refusal, because the mechanism cannot know whether
    // it shadows — that is semantic — so it says what it does know.
    harness.state_mut().references.as_mut().expect("the modal").new_name = "new".to_owned();
    steady(&mut harness);
    let modal = harness.state().references.as_ref().expect("the modal");
    assert!(modal.refusal.is_none(), "a collision does not stop it");
    assert!(
        modal.warning.as_deref().is_some_and(|said| said.contains("already defined")),
        "{:?}",
        modal.warning
    );
}

#[test]
fn a_file_whose_language_says_nothing_has_none_of_the_three_entries() {
    // Scenario 17 through the menu the window really builds, and the reason a control that can
    // never apply is absent rather than dimmed.
    let harness = code_harness("notes.md");
    let names: Vec<String> = unluminous_app::app::actions::menus(&harness.state().menu_state())
        .iter()
        .find(|menu| menu.name == "Edit")
        .expect("the Edit menu")
        .entries
        .iter()
        .filter_map(|entry| match entry {
            unluminous_app::app::actions::Entry::Item { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    for absent in ["Go to Definition", "Find References", "Rename Symbol..."] {
        assert!(!names.contains(&absent.to_owned()), "{absent} should be absent: {names:?}");
    }
    assert!(names.contains(&"Navigate Back".to_owned()), "the history is about the window");
    // And a source file has all three.
    let mut harness = code_harness("layout.rs");
    let state = harness.state().menu_state();
    assert!(state.definitions_apply && state.symbols_apply);
    steady(&mut harness);
}

/// Where on the screen a byte of the file that is showing is drawn.
///
/// The same arithmetic `show_editor` does: the editing area's own rectangle — which is what is left
/// after the gutter has taken its column — plus the padding, less how far the file is scrolled.
fn point_of(harness: &Harness<'static, UnluminousApp>, offset: usize) -> egui::Pos2 {
    let area = harness.state().editor_area();
    let caret = harness.state().layout().caret_at(offset);
    let scroll = harness.state().files.active().scroll;
    egui::pos2(
        area.left() + unluminous_app::components::editor_view::PADDING + caret.x + 1.0,
        area.top() + unluminous_app::theme::size::EDITOR_PADDING_Y - scroll
            + caret.y
            + caret.height / 2.0,
    )
}

/// Move the pointer over a byte of the file, with the platform's modifier held or not.
///
/// `Event::ModifiersChanged` is how the modifier is said to be held: egui carries the state of the
/// modifier keys on that event rather than on the pointer's, which is what a real window sends when
/// the key goes down with the pointer already where it is — and that, rather than a click, is the
/// whole gesture up to the moment of the click.
fn hover_over(harness: &mut Harness<'static, UnluminousApp>, offset: usize, modifier: bool) {
    let at = point_of(harness, offset);
    let held = if modifier { Modifiers::COMMAND } else { Modifiers::NONE };
    harness.input_mut().events.push(egui::Event::ModifiersChanged(held));
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    steady(harness);
}

#[test]
fn the_underline_appears_under_the_word_the_modifier_is_over_and_goes_with_it() {
    // Scenario 7 as a picture. The affordance is what the whole gesture rests on: a word that is
    // underlined is a word the click will take you somewhere from, and one that is not is a word an
    // ordinary click will put the caret in.
    let mut harness = code_harness("caret.rs");
    let text = harness.state().document().text().to_string();
    let call = text.find("layout.draw()").expect("the call") + "layout.".len() + 1;

    // The pointer over the word with nothing held: no underline, and the ordinary writing bar.
    hover_over(&mut harness, call, false);
    let plain = harness.render().expect("render the window");

    // The same point with the modifier held: the word is underlined.
    hover_over(&mut harness, call, true);
    let underlined = harness.render().expect("render the window");
    assert_ne!(
        plain.as_raw(),
        underlined.as_raw(),
        "holding the modifier over a word that resolves has to change what is drawn"
    );
    harness.snapshot(shot("go_to_definition_underline"));

    // Letting go of it takes the underline away again, which is what stops an affordance outliving
    // the gesture that asked for it.
    hover_over(&mut harness, call, false);
    let released = harness.render().expect("render the window");
    assert_eq!(
        plain.as_raw(),
        released.as_raw(),
        "letting go of the modifier puts the window back exactly as it was"
    );
}

/// Press and release the primary button where the pointer is, with the modifier held.
fn modifier_click(harness: &mut Harness<'static, UnluminousApp>, offset: usize) {
    let at = point_of(harness, offset);
    harness.input_mut().events.push(egui::Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    steady(harness);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::COMMAND,
        });
    }
    steady(harness);
}

#[test]
fn a_modifier_click_on_a_word_goes_to_its_definition_and_an_ordinary_one_places_the_caret() {
    // Scenario 1's click half, and scenario 5's: the gesture is what a person really does, and the
    // same click without the modifier has to keep meaning what it always meant.
    let mut harness = code_harness("caret.rs");
    let text = harness.state().document().text().to_string();
    let call = text.find("layout.draw()").expect("the call") + "layout.".len() + 1;

    // Without the modifier it is an ordinary click: the caret lands in the word and nothing opens.
    hover_over(&mut harness, call, false);
    let at = point_of(&harness, call);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    steady(&mut harness);
    assert_eq!(harness.state().files.active().name(), "caret.rs", "nothing was opened");
    assert!(
        harness.state().document().selection().is_empty(),
        "and a click places a caret rather than selecting anything"
    );

    // With it held, the same click goes to the definition and selects the name it landed on.
    modifier_click(&mut harness, call);
    assert_eq!(harness.state().files.active().name(), "layout.rs");
    assert_eq!(harness.state().document().selected_text(), "draw");
}

/// The first modifier-click jumps even when the keyboard was somewhere else. `task-2063`: the reference editor
/// jumps on the first `Ctrl`/`Cmd`+Click, and here the first click after using the explorer only moved
/// the keyboard.
#[test]
fn a_modifier_click_jumps_even_when_the_keyboard_was_elsewhere() {
    let mut harness = code_harness("caret.rs");
    harness.state_mut().focus = unluminous_app::app::Focus::Explorer;
    steady(&mut harness);
    let text = harness.state().document().text().to_string();
    let call = text.find("layout.draw()").expect("the call") + "layout.".len() + 1;
    modifier_click(&mut harness, call);
    assert_eq!(harness.state().files.active().name(), "layout.rs", "the first click went there");
    assert_eq!(harness.state().document().selected_text(), "draw");
}

/// A parameter has no keyword in front of it, and a click on a use of one goes to where it is written in
/// the parameter list. `task-2063`.
#[test]
fn a_modifier_click_on_a_parameter_goes_to_the_parameter() {
    let mut harness = code_harness("caret.rs");
    let text = harness.state().document().text().to_string();
    let parameter = text.find("layout: &Layout").expect("the parameter");
    let used = text.find("layout.draw()").expect("a use");
    modifier_click(&mut harness, used + 2);
    assert_eq!(harness.state().files.active().name(), "caret.rs");
    let selection = harness.state().document().selection().range();
    assert_eq!(selection, parameter..parameter + "layout".len(), "the parameter is selected");
}

/// `Ctrl`/`Cmd`+`[` goes back to where the caret was before a jump, and `]` forward again. `task-2063`.
#[test]
fn the_brackets_walk_back_and_forward_through_the_jumps() {
    let mut harness = code_harness("caret.rs");
    let text = harness.state().document().text().to_string();
    let call = text.find("layout.draw()").expect("the call") + "layout.".len() + 1;
    harness.state_mut().command(Command::PlaceCaret { offset: call, extend: false });
    steady(&mut harness);
    modifier_click(&mut harness, call);
    assert_eq!(harness.state().files.active().name(), "layout.rs");

    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::OpenBracket);
    steady(&mut harness);
    assert_eq!(harness.state().files.active().name(), "caret.rs", "back to the file");
    assert_eq!(harness.state().document().selection().head, call, "at the same place in it");

    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::CloseBracket);
    steady(&mut harness);
    assert_eq!(harness.state().files.active().name(), "layout.rs", "and forward again");
}

#[test]
fn a_modifier_click_on_the_definition_itself_opens_the_references() {
    // Scenario 8 through the gesture rather than through the menu: one gesture serves both
    // directions of the question, which is what the reference editor calls "Go to Declaration or Usages".
    let mut harness = code_harness("layout.rs");
    let text = harness.state().document().text().to_string();
    let definition = text.find("fn draw").expect("the definition") + 4;
    modifier_click(&mut harness, definition);
    settle_the_references(&mut harness);
    let modal = harness.state().references.as_ref().expect("the references opened");
    assert_eq!(modal.name, "draw");
    assert!(!modal.hits().is_empty());
}

/// A small TypeScript project for the import pictures.
///
/// Its own folder, for the reason [`completion_folder`] has its own: the explorer draws whatever is
/// in the folder, so adding a file to a fixture another test has already accepted a picture of would
/// change that picture for a reason that is not a change to Unluminous.
fn import_folder() -> std::path::PathBuf {
    fixture(
        "unluminous-screenshot-imports",
        &[
            ("src/app/main.ts", ""),
            (
                "src/app/layout.ts",
                "export class Layout {}\n\
                 \n\
                 export interface Placed {}\n\
                 \n\
                 export const LINE_HEIGHT = 18;\n\
                 \n\
                 export function drawFrame() {\n\
                 \x20   const hidden = 1;\n\
                 \x20   return hidden;\n\
                 }\n\
                 \n\
                 export function drawGutter() {}\n\
                 \n\
                 export function drawCaret() {}\n\
                 \n\
                 const secret = 2;\n",
            ),
            ("src/app/caret.ts", "export class Caret {}\n"),
            ("src/app/widgets/index.ts", "export class Button {}\n"),
            ("src/app/widgets/scrollbar.ts", "export class Bar {}\n"),
            ("src/core/completion.ts", "export function rank() {}\n"),
            ("src/core/document.ts", "export class Document {}\n"),
            ("readme.md", "# a project\n"),
        ],
    )
}

/// A window on it, with its index built and `src/app/main.ts` open and empty.
fn import_harness() -> Harness<'static, UnluminousApp> {
    let folder = import_folder();
    let mut harness = harness_in(&folder);
    harness
        .state_mut()
        .open_path_permanently(&folder.join("src/app/main.ts"))
        .expect("the file opens");
    for _ in 0..600 {
        pump(&mut harness);
        let ready = harness
            .state()
            .symbols_indexer()
            .is_some_and(|indexer| !indexer.is_building() && !indexer.index().is_empty());
        if ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    steady(&mut harness);
    harness
}

#[test]
fn typing_a_module_specifier_offers_the_projects_own_files() {
    // Scenarios 2 and 41: the quotes open and the list is the project, written as the specifier
    // that would reach each file from this one.
    let mut harness = import_harness();
    type_letters(&mut harness, "import { Layout } from '");
    let offered = completions(&harness);
    assert!(offered.contains(&"./layout".to_owned()), "{offered:?}");
    assert!(offered.contains(&"./widgets".to_owned()), "{offered:?}");
    assert!(offered.contains(&"../core/completion".to_owned()), "{offered:?}");
    assert!(!offered.iter().any(|row| row.ends_with(".ts")), "the extension is dropped");
    harness.snapshot(shot("completion_import_specifier"));
}

#[test]
fn a_name_typed_between_the_braces_offers_what_that_module_exports() {
    // Scenarios 11 and 44: the module is written after the caret, and only what `export` marks is
    // offered — `hidden` and `secret` are in the file and are not something another file can name.
    let mut harness = import_harness();
    let line = "import { draw } from './layout'";
    harness.state_mut().command(Command::Insert(line.to_owned()));
    let caret = line.find("draw").expect("the sample") + 4;
    harness.state_mut().command(Command::PlaceCaret { offset: caret, extend: false });
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::CompleteWord, &ctx);
    steady(&mut harness);
    let offered = completions(&harness);
    assert_eq!(
        offered,
        vec!["drawCaret".to_owned(), "drawFrame".to_owned(), "drawGutter".to_owned()],
        "only the exports, best first"
    );
    harness.snapshot(shot("completion_import_named"));
}

/// A small project for the completion pictures.
///
/// Separate from [`code_folder`] on purpose: the explorer draws whatever is in the folder, so adding
/// a file to a fixture another test has already accepted a picture of would change that picture for
/// a reason that is not a change to Unluminous.
///
/// It is built so that one stem, `dra`, offers ten rows covering all five kinds a definition can be
/// — a function, a variable, a module, a type and a constant — and more rows than the eight the list
/// draws, which is what the scrolling picture needs. `distant.rs` is never opened, so what it
/// defines can only have come from the project's index.
fn completion_folder() -> std::path::PathBuf {
    fixture("unluminous-screenshot-completion", &[
        ("layout.rs", "//! Laying a document out.\n\
                 \n\
                 pub struct Layout;\n\
                 \n\
                 const DRAW_LIMIT: usize = 8;\n\
                 \n\
                 impl Layout {\n\
                 \x20   pub fn new() -> Self {\n\
                 \x20       let drawn = 0;\n\
                 \x20       let _ = drawn;\n\
                 \x20       Layout\n\
                 \x20   }\n\
                 \n\
                 \x20   /// Draw the whole of it.\n\
                 \x20   pub fn draw(&self) {}\n\
                 \n\
                 \x20   pub fn draw_frame(&self) {}\n\
                 \n\
                 \x20   pub fn draw_gutter(&self) {}\n\
                 \n\
                 \x20   pub fn draw_caret(&self) {}\n\
                 \n\
                 \x20   pub fn redraw(&self) {}\n\
                 \n\
                 \x20   pub fn paint_text(&self) {}\n\
                 }\n\
                 \n\
                 pub mod parts {\n\
                 \x20   pub fn first() {}\n\
                 \n\
                 \x20   pub fn second() {}\n\
                 \n\
                 \x20   pub fn third() {}\n\
                 \n\
                 \x20   pub fn fourth() {}\n\
                 \n\
                 \x20   pub fn fifth() {}\n\
                 \n\
                 \x20   pub fn sixth() {}\n\
                 }\n"),
        ("caret.rs", "pub struct Caret;\n\nimpl Caret {\n    pub fn new() -> Self {\n        Caret\n    }\n\n    pub fn paint(&self, layout: &Layout) {\n        layout.draw();\n    }\n}\n"),
        ("distant.rs", "pub struct Drawing;\n\npub mod drawings {}\n\npub fn draw_everything() {}\n"),
        ("notes.md", "# Notes\n\nA note about drawing.\n"),
    ])
}

/// A window on the completion folder, with its index built and the named file open.
fn completion_harness(open: &str) -> Harness<'static, UnluminousApp> {
    let folder = completion_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join(open)).expect("the file opens");
    // The index is read on a thread, so the harness is run until the answer arrives. Each run is a
    // frame; nothing here waits on a clock.
    for _ in 0..600 {
        pump(&mut harness);
        let ready = harness
            .state()
            .symbols_indexer()
            .is_some_and(|indexer| !indexer.is_building() && !indexer.index().is_empty());
        if ready {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    steady(&mut harness);
    harness
}

/// Type `text` a letter at a time, as real text events, which is the path the released binary takes
/// and the only one that fires the automatic trigger.
fn type_letters(harness: &mut Harness<'static, UnluminousApp>, text: &str) {
    for letter in text.chars() {
        harness.input_mut().events.push(egui::Event::Text(letter.to_string()));
        steady(harness);
    }
}

/// A supplied stem is a read-only query over the same completion sources the popup uses.
#[test]
fn a_hypothetical_completion_stem_changes_nothing_in_the_document() {
    let mut harness = completion_harness("layout.rs");
    let before_text = harness.state().document().text().to_string();
    let before_selection = harness.state().document().selection();
    let before_revision = harness.state().document().revision();
    let before_undo = harness.state().document().can_undo();
    let before_redo = harness.state().document().can_redo();

    let result = did(&mut harness, "editor complete --stem ar --limit 20");
    let names: Vec<&str> = result["rows"]
        .as_array()
        .expect("completion rows")
        .iter()
        .filter_map(|row| row["name"].as_str())
        .collect();
    assert_eq!(result["stem"], "ar");
    assert!(
        names.contains(&"Caret"),
        "the project index was ranked for the supplied stem: {names:?}"
    );
    assert_eq!(harness.state().document().text().to_string(), before_text);
    assert_eq!(harness.state().document().selection(), before_selection);
    assert_eq!(harness.state().document().revision(), before_revision);
    assert_eq!(harness.state().document().can_undo(), before_undo);
    assert_eq!(harness.state().document().can_redo(), before_redo);
    assert!(!harness.state().document().is_modified());
    assert!(harness.state().completion().is_none(), "listing does not open the visible popup");
    assert_eq!(refused(&mut harness, "editor complete --stem ar --choose Caret"), "usage");
}

#[test]
fn typing_a_word_offers_the_names_it_could_become() {
    // Scenarios 12 and 33: the list opens on the second character, hangs under the caret, and each
    // row carries its matched letters in the accent colour, a glyph for what it is, and a quiet word
    // saying where it came from.
    let mut harness = completion_harness("layout.rs");
    // On the blank line under the struct, so the list has room to hang below the caret.
    let text = harness.state().document().text().to_string();
    let blank = text.find("\nconst DRAW_LIMIT").expect("the blank line above the constant");
    harness.state_mut().command(Command::PlaceCaret { offset: blank, extend: false });
    steady(&mut harness);

    type_letters(&mut harness, "d");
    assert!(harness.state().completion().is_none(), "one character is noise, not an offer");
    type_letters(&mut harness, "ra");

    let offered = completions(&harness);
    assert_eq!(
        offered,
        [
            "draw",
            "drawn",
            "drawings",
            "Drawing",
            "draw_caret",
            "draw_frame",
            "draw_gutter",
            "draw_everything",
            "DRAW_LIMIT",
            "redraw",
        ],
        "the order the rubric gives"
    );
    assert_eq!(harness.state().completion().expect("open").chosen, 0, "the best row is pre-chosen");
    // Every row names itself, which is what makes it findable at all.
    harness.get_by_label("Completion draw");
    harness.get_by_label("Completion Drawing");
    // The list is drawn under the caret's own line.
    let anchor = harness.state().completion_anchor().expect("the popup was drawn");
    assert!(anchor.pane.contains_rect(unluminous_app::components::completion::where_it_goes(
        8,
        anchor.caret,
        anchor.pane
    )));
    harness.snapshot(shot("completion_list"));
}

#[test]
fn the_list_flips_above_the_caret_at_the_bottom_of_the_pane() {
    // Scenario 22. Under the word is where the eye already is, so it only ever flips when the rows
    // would cross the bottom of the pane.
    let mut harness = completion_harness("layout.rs");
    let end = harness.state().document().text().len_bytes();
    harness.state_mut().command(Command::PlaceCaret { offset: end, extend: false });
    steady(&mut harness);
    type_letters(&mut harness, "dra");
    assert!(harness.state().completion().is_some(), "{:?}", completions(&harness));
    let anchor = harness.state().completion_anchor().expect("the popup was drawn");
    let rows = harness.state().completion().expect("open").shown().len();
    let area =
        unluminous_app::components::completion::where_it_goes(rows, anchor.caret, anchor.pane);
    assert!(
        area.bottom() <= anchor.caret.top(),
        "the caret is at the bottom of the pane, so the list belongs above it: {area:?} against {:?}",
        anchor.caret
    );
    assert!(anchor.pane.contains_rect(area), "and all of it is still on the screen");
    harness.snapshot(shot("completion_above_the_caret"));
}

#[test]
fn walking_past_the_eighth_row_scrolls_the_list() {
    // Scenario 25's second half: eight rows are drawn and the pill drags the rest into view.
    let mut harness = completion_harness("layout.rs");
    let text = harness.state().document().text().to_string();
    let blank = text.find("\nconst DRAW_LIMIT").expect("the blank line");
    harness.state_mut().command(Command::PlaceCaret { offset: blank, extend: false });
    steady(&mut harness);
    type_letters(&mut harness, "dra");
    assert!(completions(&harness).len() > 8, "{:?}", completions(&harness));
    for _ in 0..9 {
        harness.key_press(egui::Key::ArrowDown);
        steady(&mut harness);
    }
    let state = harness.state().completion().expect("open");
    assert_eq!(state.chosen, 9, "the tenth row, and no further: the ends are clamped");
    assert!(state.scroll > 0, "so the list scrolled to reach it");
    assert!(state.shown().contains(&state.chosen));
    // The caret has not moved: the arrows were consumed before the editing area read them.
    assert!(
        harness.state().document().text().to_string().contains("dra\nconst DRAW_LIMIT"),
        "and nothing was typed by the arrows"
    );
    harness.snapshot(shot("completion_scrolled"));
}

#[test]
fn clicking_a_row_takes_it_and_the_click_never_reaches_the_document() {
    // Scenario 30. The list's own `Area` is in front of the editing area, so the click lands on the
    // row rather than placing a caret behind it.
    let mut harness = completion_harness("layout.rs");
    let text = harness.state().document().text().to_string();
    let blank = text.find("\nconst DRAW_LIMIT").expect("the blank line");
    harness.state_mut().command(Command::PlaceCaret { offset: blank, extend: false });
    steady(&mut harness);
    type_letters(&mut harness, "dra");
    harness.get_by_label("Completion draw_gutter").click();
    steady(&mut harness);
    assert!(harness.state().completion().is_none(), "taking a row closes the list");
    let after = harness.state().document().text().to_string();
    assert!(after.contains("draw_gutter\nconst DRAW_LIMIT"), "{after:?}");
    assert!(
        harness.state().document().selection().is_empty(),
        "and the click placed no caret in the document behind the list"
    );
}

#[test]
fn tab_takes_the_best_row_and_the_editing_area_never_sees_the_key() {
    // Scenarios 26 and 28 through the real window: `Tab` is the gesture the ticket names, and while
    // the list is open it must not also type a tab into the file.
    let mut harness = completion_harness("layout.rs");
    let text = harness.state().document().text().to_string();
    let blank = text.find("\nconst DRAW_LIMIT").expect("the blank line");
    harness.state_mut().command(Command::PlaceCaret { offset: blank, extend: false });
    steady(&mut harness);
    type_letters(&mut harness, "dra");
    harness.key_press(egui::Key::Tab);
    steady(&mut harness);
    let after = harness.state().document().text().to_string();
    assert!(after.contains("draw\nconst DRAW_LIMIT"), "{after:?}");
    assert!(!after.contains('\t'), "no tab was typed into the file");
    assert!(harness.state().completion().is_none());
    // And with the list shut, `Tab` means what it always meant.
    harness.key_press(egui::Key::Tab);
    steady(&mut harness);
    assert!(
        harness.state().document().text().to_string().contains("draw\t"),
        "{:?}",
        harness.state().document().text().to_string()
    );
}

#[test]
fn a_split_view_has_one_list_at_most_and_it_is_in_the_pane_with_the_keyboard() {
    // Scenario 23. One `Option` on the window makes "at most one" true by construction; the picture
    // is what says it is drawn in the right pane and over the divider rather than under it.
    let mut harness = completion_harness("caret.rs");
    let folder = completion_folder();
    harness.state_mut().open_path_permanently(&folder.join("layout.rs")).expect("the file opens");
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::SplitRight, &ctx);
    steady(&mut harness);
    assert_eq!(harness.state().files.pane_count(), 2);
    let text = harness.state().document().text().to_string();
    let blank = text.find("\nconst DRAW_LIMIT").expect("the blank line");
    harness.state_mut().command(Command::PlaceCaret { offset: blank, extend: false });
    steady(&mut harness);
    type_letters(&mut harness, "dra");
    assert!(harness.state().completion().is_some(), "{:?}", completions(&harness));
    let anchor = harness.state().completion_anchor().expect("the popup was drawn");
    let editing = harness.state().editor_area();
    assert!(
        anchor.pane.left() >= editing.left() - 1.0,
        "the list belongs to the pane with the keyboard: {:?} against {editing:?}",
        anchor.pane
    );
    harness.snapshot(shot("completion_split_view"));
}

#[test]
fn nothing_is_offered_in_a_file_no_plugin_claims() {
    // Scenario 13 through the menu the window really builds: absent, not dimmed.
    let harness = completion_harness("notes.md");
    let names: Vec<String> = unluminous_app::app::actions::menus(&harness.state().menu_state())
        .iter()
        .find(|menu| menu.name == "Edit")
        .expect("the Edit menu")
        .entries
        .iter()
        .filter_map(|entry| match entry {
            unluminous_app::app::actions::Entry::Item { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert!(!names.contains(&"Complete Word".to_owned()), "{names:?}");
    let mut harness = completion_harness("layout.rs");
    let names: Vec<String> = unluminous_app::app::actions::menus(&harness.state().menu_state())
        .iter()
        .find(|menu| menu.name == "Edit")
        .expect("the Edit menu")
        .entries
        .iter()
        .filter_map(|entry| match entry {
            unluminous_app::app::actions::Entry::Item { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert!(names.contains(&"Complete Word".to_owned()), "{names:?}");
    steady(&mut harness);
}

#[test]
fn the_editor_page_holds_the_gutter_and_the_suggestions() {
    // `task-1677` §8.3 put `editor.suggestions` here, beside the line numbers, because both are
    // about what the editing area does rather than about what it looks like. The tick box is the
    // furniture `components::modal` and this dialog already had; what is new is the section.
    let mut harness = harness("");
    open_settings(&mut harness);
    harness.get_by_label("Editor").click();
    steady(&mut harness);
    harness.get_by_label("Show line numbers");
    harness.get_by_label("Suggest completions as you type");
    assert!(harness.state().settings.suggestions.is_automatic(), "on in a fresh Unluminous");
    harness.snapshot(shot("settings_editor"));

    // And the box really is the setting: clicking it puts the popup back to being asked for.
    harness.get_by_label("Suggest completions as you type").click();
    steady(&mut harness);
    assert!(!harness.state().settings.suggestions.is_automatic());
}

#[test]
fn enter_presses_the_button_that_does_the_thing() {
    // The About box has one button and no field in it, so before `task-1682` there was no way to
    // answer it from the keyboard at all. `components::modal::footer` is where that is decided, so
    // this is the rule every modal built from it follows.
    let mut harness = harness("Text behind the About box.");
    open_about(&mut harness);
    assert!(harness.state().about.is_some());
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    assert!(harness.state().about.is_none(), "Enter should have pressed Done");
}

#[test]
fn enter_answers_a_question_that_has_no_field_in_it_and_reaches_nothing_behind_it() {
    let folder = scratch_folder("enter-confirms");
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("tab open {}", folder.join("readme.md").display()));

    did(
        &mut harness,
        &format!("action run delete-path --path {}", folder.join("readme.md").display()),
    );
    assert!(harness.state().confirmation.is_some(), "the question is asked");
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    assert!(harness.state().confirmation.is_none(), "Enter answered it");
    assert!(!folder.join("readme.md").exists(), "and the file has gone");
}

#[test]
fn a_modal_takes_the_keyboard_from_the_editing_area_and_the_explorer() {
    // The confirmation has no field in it, so nothing had egui's focus and the panes behind it went
    // on reading the frame's keys. `Enter` therefore meant three things at once: answer the
    // question, insert a new line into the file, and open the row the explorer's cursor was on.
    let folder = scratch_folder("modal-keyboard");
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("tab open {}", folder.join("app/main.ts").display()));
    let before = harness.state().document().text().to_string();
    harness.state_mut().command(unluminous_core::Command::PlaceCaret { offset: 0, extend: false });
    steady(&mut harness);

    did(
        &mut harness,
        &format!("action run delete-path --path {}", folder.join("readme.md").display()),
    );
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "the file behind the question should not have gained a new line"
    );
    assert!(!harness.state().document().is_modified());
    assert!(!folder.join("readme.md").exists(), "and the question was answered");

    // A letter typed while a modal is open does not reach the document either.
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("tab open {}", folder.join("app/main.ts").display()));
    let before = harness.state().document().text().to_string();
    did(
        &mut harness,
        &format!("action run delete-path --path {}", folder.join("app/main.ts").display()),
    );
    harness.input_mut().events.push(egui::Event::Text("typed".to_owned()));
    steady(&mut harness);
    assert_eq!(harness.state().document().text().to_string(), before);
    did(&mut harness, "modal cancel");
}

#[test]
fn the_explorers_menu_holds_delete_and_it_asks_before_anything_goes() {
    let folder = scratch_folder("menu");
    let mut harness = harness_in(&folder);
    let entries = unluminous_app::app::actions::explorer_menu(
        &folder.join("readme.md"),
        false,
        false,
        unluminous_app::app::actions::Aim::AtARow,
    );
    let names: Vec<String> = entries
        .iter()
        .filter_map(|entry| match entry {
            unluminous_app::app::actions::Entry::Item { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert!(names.contains(&"Delete".to_owned()), "the menu holds it: {names:?}");

    did(
        &mut harness,
        &format!("action run delete-path --path {}", folder.join("readme.md").display()),
    );
    let question = harness.state().confirmation.clone().expect("the question is asked");
    assert!(question.note.contains("readme.md"), "it names the file: {}", question.note);
    assert!(
        folder.join("readme.md").is_file(),
        "and nothing has gone while the question is still on the screen"
    );
    assert!(
        harness.state().message.is_none(),
        "and the status bar is not still saying what the last thing to happen was, which is what          made asking the question report a deletion that had not happened: {:?}",
        harness.state().message
    );
    harness.snapshot(shot("delete_confirmation"));
}

#[test]
fn confirming_the_question_takes_the_file_off_the_disk_and_closes_its_tab() {
    let folder = scratch_folder("confirm");
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("tab open {}", folder.join("readme.md").display()));
    assert!(harness.state().files.paths().contains(&folder.join("readme.md")));

    did(
        &mut harness,
        &format!("action run delete-path --path {}", folder.join("readme.md").display()),
    );
    did(&mut harness, "modal accept");
    steady(&mut harness);
    assert!(!folder.join("readme.md").exists(), "the file has gone");
    assert!(
        !harness.state().files.paths().contains(&folder.join("readme.md")),
        "and the tab that was on it has gone with it"
    );
}

#[test]
fn cancelling_the_question_leaves_the_file_exactly_where_it_was() {
    let folder = scratch_folder("cancel");
    let mut harness = harness_in(&folder);
    did(
        &mut harness,
        &format!("action run delete-path --path {}", folder.join("readme.md").display()),
    );
    did(&mut harness, "modal cancel");
    steady(&mut harness);
    assert!(folder.join("readme.md").is_file());
    assert!(harness.state().confirmation.is_none());
}

#[test]
fn delete_means_the_file_in_the_explorer_and_the_letter_in_the_editor() {
    let folder = scratch_folder("two-meanings");
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("tab open {}", folder.join("readme.md").display()));

    // With the editing area holding the keyboard, `Delete` is what it has always been.
    harness.state_mut().command(unluminous_core::Command::PlaceCaret { offset: 0, extend: false });
    steady(&mut harness);
    harness.key_press(egui::Key::Delete);
    steady(&mut harness);
    assert_eq!(
        harness.state().document().text().to_string(),
        " Notes\n",
        "it took the letter in front of the caret"
    );
    assert!(harness.state().confirmation.is_none(), "and asked nothing");

    // With the explorer holding it, the same key is about the file.
    did(&mut harness, &format!("explorer select {}", folder.join("readme.md").display()));
    harness.key_press(egui::Key::Delete);
    steady(&mut harness);
    let question = harness.state().confirmation.clone().expect("the question is asked instead");
    assert!(question.note.contains("readme.md"));
}

#[test]
fn the_arrow_keys_walk_the_selection_and_a_letter_hands_the_keyboard_back() {
    let folder = scratch_folder("arrows");
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("explorer select {}", folder.join("app").display()));
    let rows: Vec<std::path::PathBuf> =
        harness.state().tree.rows().iter().map(|row| row.entry.path.clone()).collect();
    let at =
        rows.iter().position(|row| *row == folder.join("app")).expect("the app folder is a row");

    harness.key_press(egui::Key::ArrowDown);
    steady(&mut harness);
    assert_eq!(
        harness.state().selected.as_deref(),
        Some(rows[at + 1].as_path()),
        "Down moves to the next row that is showing"
    );
    harness.key_press(egui::Key::ArrowUp);
    steady(&mut harness);
    assert_eq!(harness.state().selected.as_deref(), Some(folder.join("app").as_path()));

    // A letter belongs to the editor, so it hands the keyboard over and the letter lands in the
    // document. Without this, clicking a file in the tree and then typing would swallow the word.
    let before = harness.state().document().text().to_string();
    harness.input_mut().events.push(egui::Event::Text("x".to_owned()));
    steady(&mut harness);
    assert_eq!(harness.state().focus, unluminous_app::app::Focus::Editor);
    assert_eq!(
        harness.state().document().text().to_string(),
        format!("x{before}"),
        "and the letter that handed it over is the one that was typed"
    );
}

#[test]
fn closing_a_tab_that_was_edited_writes_it_and_an_untitled_one_is_not_written() {
    let folder = scratch_folder("save-on-close");
    let mut harness = harness_in(&folder);
    // A window opens with one untitled tab, which is the case that has nowhere to be written. It is
    // closed as it always was and says so, rather than putting `untitled.md` in somebody's project
    // because they shut a scratch buffer.
    harness.input_mut().events.push(egui::Event::Text("scratch".to_owned()));
    steady(&mut harness);
    did(&mut harness, "tab close");
    steady(&mut harness);
    assert!(
        harness.state().message.clone().unwrap_or_default().contains("without saving"),
        "it says what it did: {:?}",
        harness.state().message
    );
    assert!(!folder.join("untitled.md").exists(), "and wrote nothing into the project");

    did(&mut harness, &format!("tab open {}", folder.join("readme.md").display()));
    harness.state_mut().command(unluminous_core::Command::PlaceCaret { offset: 0, extend: false });
    steady(&mut harness);
    harness.input_mut().events.push(egui::Event::Text("Hello ".to_owned()));
    steady(&mut harness);
    assert!(harness.state().document().is_modified(), "it has changes that are not on the disk");

    did(&mut harness, "tab close");
    steady(&mut harness);
    assert_eq!(
        std::fs::read_to_string(folder.join("readme.md")).expect("read it back"),
        "Hello # Notes\n",
        "closing the tab wrote what was typed"
    );
}

#[test]
fn discarding_is_how_a_script_closes_a_tab_without_writing_it() {
    let folder = scratch_folder("discard");
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("tab open {}", folder.join("readme.md").display()));
    harness.state_mut().command(unluminous_core::Command::PlaceCaret { offset: 0, extend: false });
    steady(&mut harness);
    harness.input_mut().events.push(egui::Event::Text("Hello ".to_owned()));
    steady(&mut harness);
    did(&mut harness, "tab close --discard");
    steady(&mut harness);
    assert_eq!(
        std::fs::read_to_string(folder.join("readme.md")).expect("read it back"),
        "# Notes\n",
        "the file on the disk is untouched"
    );
}

#[test]
fn moving_a_file_rewrites_a_closed_importer_and_leaves_an_open_one_modified() {
    let folder = scratch_folder("move");
    let mut harness = harness_in(&folder);
    // One of the two importers is open, so the ownership rule has both cases to answer.
    did(&mut harness, &format!("tab open {}", folder.join("app/main.ts").display()));

    let result = did(
        &mut harness,
        &format!(
            "explorer move {} {}",
            folder.join("app/layout.ts").display(),
            folder.join("draw").display()
        ),
    );
    assert_eq!(result["applied"], serde_json::json!(true));
    steady(&mut harness);

    assert!(folder.join("draw/layout.ts").is_file(), "the file moved");
    assert!(!folder.join("app/layout.ts").exists());
    assert_eq!(
        std::fs::read_to_string(folder.join("app/other.ts")).expect("read the closed importer"),
        "import { draw } from '../draw/layout';\n",
        "the closed file was written"
    );
    let open = harness
        .state()
        .files
        .iter()
        .find(|file| file.path() == Some(folder.join("app/main.ts").as_path()))
        .expect("the open importer is still a tab");
    assert_eq!(
        open.document.text().to_string(),
        "import { draw } from '../draw/layout';\n",
        "the open file was edited as a document"
    );
    assert!(open.document.is_modified(), "and left unsaved rather than written behind somebody");
    assert_eq!(
        std::fs::read_to_string(folder.join("app/main.ts")).expect("read what is on the disk"),
        "import { draw } from './layout';\n",
        "so the disk still holds what it held"
    );
}

#[test]
fn a_dry_run_says_what_would_change_and_changes_nothing() {
    let folder = scratch_folder("dry-run");
    let mut harness = harness_in(&folder);
    let result = did(
        &mut harness,
        &format!(
            "explorer move {} {} --dry-run",
            folder.join("app/layout.ts").display(),
            folder.join("draw").display()
        ),
    );
    assert_eq!(result["applied"], serde_json::json!(false));
    assert_eq!(result["references"], serde_json::json!(2), "both importers would change");
    assert!(folder.join("app/layout.ts").is_file(), "and nothing moved");
    assert_eq!(
        std::fs::read_to_string(folder.join("app/other.ts")).expect("read it"),
        "import { draw } from './layout';\n",
        "and nothing was written"
    );
}

#[test]
fn a_move_asked_for_without_the_refactor_leaves_every_reference_alone() {
    let folder = scratch_folder("no-refactor");
    let mut harness = harness_in(&folder);
    did(
        &mut harness,
        &format!(
            "explorer move {} {} --no-refactor",
            folder.join("app/layout.ts").display(),
            folder.join("draw").display()
        ),
    );
    assert!(folder.join("draw/layout.ts").is_file());
    assert_eq!(
        std::fs::read_to_string(folder.join("app/other.ts")).expect("read it"),
        "import { draw } from './layout';\n",
        "the import is exactly as it was, and now points at nothing"
    );
}

#[test]
fn a_move_and_the_move_back_leave_the_project_exactly_as_it_started() {
    let folder = scratch_folder("its-own-inverse");
    let before = std::fs::read_to_string(folder.join("app/other.ts")).expect("read it");
    let mut harness = harness_in(&folder);
    did(
        &mut harness,
        &format!(
            "explorer move {} {}",
            folder.join("app/layout.ts").display(),
            folder.join("draw").display()
        ),
    );
    did(
        &mut harness,
        &format!(
            "explorer move {} {}",
            folder.join("draw/layout.ts").display(),
            folder.join("app").display()
        ),
    );
    assert!(folder.join("app/layout.ts").is_file(), "it is back where it started");
    assert_eq!(
        std::fs::read_to_string(folder.join("app/other.ts")).expect("read it again"),
        before,
        "and so is every specifier, which is why a move needs no undo of its own"
    );
}

#[test]
fn renaming_a_file_takes_the_code_that_names_it_with_it() {
    let folder = scratch_folder("rename");
    let mut harness = harness_in(&folder);
    did(
        &mut harness,
        &format!("modal open rename --path {}", folder.join("app/layout.ts").display()),
    );
    did(&mut harness, "modal type page.ts");
    did(&mut harness, "modal accept");
    steady(&mut harness);
    assert!(folder.join("app/page.ts").is_file(), "the file was renamed");
    assert_eq!(
        std::fs::read_to_string(folder.join("app/other.ts")).expect("read the importer"),
        "import { draw } from './page';\n",
        "and the import followed it, because a rename is a move to a new name"
    );
}

#[test]
fn dragging_a_row_onto_a_folder_moves_it_and_rewrites_what_named_it() {
    let folder = scratch_folder("drag");
    let mut harness = harness_in(&folder);
    // The folders have to be open for their rows to be there to aim at.
    did(&mut harness, &format!("explorer expand {}", folder.join("app").display()));
    steady(&mut harness);
    let from = row_middle(&mut harness, "layout.ts");
    let to = row_middle(&mut harness, "draw");
    drag(&mut harness, from, to);
    steady(&mut harness);
    assert!(folder.join("draw/layout.ts").is_file(), "it landed in the folder it was dropped on");
    assert_eq!(
        std::fs::read_to_string(folder.join("app/other.ts")).expect("read the importer"),
        "import { draw } from '../draw/layout';\n"
    );
}

#[test]
fn a_row_dropped_where_it_already_is_does_nothing_at_all() {
    let folder = scratch_folder("no-op-drag");
    let mut harness = harness_in(&folder);
    did(&mut harness, &format!("explorer expand {}", folder.join("app").display()));
    steady(&mut harness);
    let from = row_middle(&mut harness, "layout.ts");
    let to = row_middle(&mut harness, "main.ts");
    drag(&mut harness, from, to);
    steady(&mut harness);
    assert!(folder.join("app/layout.ts").is_file(), "it is where it was");
    assert_eq!(
        std::fs::read_to_string(folder.join("app/other.ts")).expect("read the importer"),
        "import { draw } from './layout';\n",
        "and nothing was rewritten"
    );
}

/// The middle of the explorer row whose name contains `name`.
fn row_middle(harness: &mut Harness<'static, UnluminousApp>, name: &str) -> egui::Pos2 {
    let node = harness.get_by_label_contains(name);
    node.rect().center()
}

/// How many lines of the file are on the page, which is what folding changes.
fn laid_out_paragraphs(harness: &Harness<'static, UnluminousApp>) -> Vec<usize> {
    harness.state().layout().lines.iter().map(|line| line.paragraph).collect()
}

#[test]
fn a_function_can_be_collapsed_from_the_gutter_and_the_line_numbers_stay_right() {
    let mut harness = folding_harness("gutter");
    // Line 3 is `fn add(...) {`, which is where the arrow goes. The numbers a person reads are one
    // more than the paragraph numbers Unluminous counts in.
    let before = laid_out_paragraphs(&harness);
    assert!(before.contains(&5), "the body of `add` is on the page to start with");

    harness.get_by_label("Collapse block at line 3").click();
    steady(&mut harness);
    steady(&mut harness);

    let after = laid_out_paragraphs(&harness);
    assert!(after.contains(&2), "the line the function starts on is still there");
    assert!(!after.contains(&5), "the body of `add` is not");
    assert!(after.contains(&10), "and `fn subtract` still is");
    // The whole of the ticket's sixth point: the numbers of what is still showing are unchanged.
    assert_eq!(
        after.iter().filter(|paragraph| **paragraph >= 9).copied().collect::<Vec<_>>(),
        before.iter().filter(|paragraph| **paragraph >= 9).copied().collect::<Vec<_>>(),
        "every line below the fold keeps the number it had"
    );
    // Two rows say so: the arrow in the gutter, and the badge drawn after the head line's text.
    assert_eq!(harness.get_all_by_label("Expand block at line 3").count(), 2);
    harness.snapshot(shot("folding_collapsed"));
}

#[test]
fn the_badge_on_a_collapsed_block_expands_it_again() {
    let mut harness = folding_harness("badge");
    harness.get_by_label("Collapse block at line 3").click();
    steady(&mut harness);
    steady(&mut harness);
    assert!(!laid_out_paragraphs(&harness).contains(&5));
    // Two rows now say `Expand block at line 3`: the arrow in the gutter and the badge in the text.
    // The badge is the one a person reaches for first, so it has to be the affordance it looks like.
    assert_eq!(harness.get_all_by_label("Expand block at line 3").count(), 2);
    harness.get_all_by_label("Expand block at line 3").last().expect("the badge").click();
    steady(&mut harness);
    steady(&mut harness);
    assert!(laid_out_paragraphs(&harness).contains(&5), "the body came back");
}

#[test]
fn collapse_all_then_expand_all_puts_the_file_back_exactly_as_it_was() {
    let mut harness = folding_harness("all");
    let before = laid_out_paragraphs(&harness);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Fold(FoldAction::All), &ctx);
    steady(&mut harness);
    steady(&mut harness);
    let collapsed = laid_out_paragraphs(&harness);
    assert!(collapsed.len() < before.len(), "collapsing everything hides lines");
    assert!(collapsed.contains(&2) && collapsed.contains(&10), "every head line is still there");
    harness.snapshot(shot("folding_all_collapsed"));

    harness.state_mut().run_action(Action::Fold(FoldAction::None_), &ctx);
    steady(&mut harness);
    steady(&mut harness);
    assert_eq!(laid_out_paragraphs(&harness), before, "show all again gives back what was there");
}

#[test]
fn collapse_all_but_highlighted_leaves_the_marked_passage_showing() {
    let mut harness = folding_harness("marked");
    // Mark the `if` inside `add`, which is line 5 and is inside two blocks.
    let start = harness.state().document().text().line_to_byte(4);
    let end = harness.state().document().text().line_to_byte(6);
    harness
        .state_mut()
        .document_mut()
        .highlight(start..end, unluminous_core::Rgba::new(0xC9, 0xA2, 0x27, 0x66));
    steady(&mut harness);

    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Fold(FoldAction::Others), &ctx);
    steady(&mut harness);
    steady(&mut harness);

    let showing = laid_out_paragraphs(&harness);
    assert!(showing.contains(&4), "the marked line is showing");
    // Its parents had to stay open for it to be: the function, and the `if` it is the head of.
    assert!(showing.contains(&2), "the function that holds it is open");
    assert!(!showing.contains(&15), "and `fn main`, which holds nothing marked, is collapsed");
    harness.snapshot(shot("folding_all_but_marked"));
}

#[test]
fn a_caret_put_inside_a_collapsed_block_expands_it() {
    // The rule that makes everything else safe: a caret is never inside a hidden paragraph, so a
    // jump into one — go to definition, a search hit, `editor caret --line` — opens it first.
    let mut harness = folding_harness("reveal");
    harness.get_by_label("Collapse block at line 3").click();
    steady(&mut harness);
    steady(&mut harness);
    assert!(!laid_out_paragraphs(&harness).contains(&5));

    let offset = harness.state().document().text().line_to_byte(5);
    harness
        .state_mut()
        .document_mut()
        .apply(unluminous_core::Command::PlaceCaret { offset, extend: false });
    harness.state_mut().reveal_the_caret_from_a_fold();
    steady(&mut harness);
    steady(&mut harness);
    assert!(laid_out_paragraphs(&harness).contains(&5), "the block opened for the caret");
}

#[test]
fn collapsing_the_block_the_caret_is_in_moves_the_caret_to_its_head() {
    // The other half of the same rule. Collapsing what the caret is inside must not expand it
    // again, or `Collapse All` would do nothing whenever somebody was in the middle of a function.
    let mut harness = folding_harness("caret");
    let offset = harness.state().document().text().line_to_byte(5);
    harness
        .state_mut()
        .document_mut()
        .apply(unluminous_core::Command::PlaceCaret { offset, extend: false });
    steady(&mut harness);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Fold(FoldAction::All), &ctx);
    steady(&mut harness);
    steady(&mut harness);
    let caret = harness.state().document().selection().head;
    let line = harness.state().document().text().byte_to_line(caret);
    assert_eq!(line, 2, "the caret came out onto the line the function starts on");
    assert!(!laid_out_paragraphs(&harness).contains(&5), "and the block stayed collapsed");
}

#[test]
fn collapsing_recursively_hides_the_block_and_everything_inside_it() {
    // The study's ask: close a function as a whole, not one level at a time. The caret is on the
    // head of `add`, and collapsing it recursively takes the `if` inside it with it.
    let mut harness = folding_harness("recursive");
    let before = laid_out_paragraphs(&harness);
    assert!(before.contains(&5), "the body of the `if` inside `add` is on the page to start with");

    let offset = harness.state().document().text().line_to_byte(2);
    harness
        .state_mut()
        .document_mut()
        .apply(unluminous_core::Command::PlaceCaret { offset, extend: false });
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Fold(FoldAction::CollapseRecursively), &ctx);
    steady(&mut harness);
    steady(&mut harness);

    let after = laid_out_paragraphs(&harness);
    assert!(after.contains(&2), "the line `add` starts on is still there");
    assert!(!after.contains(&5), "the body of the `if` inside `add` is not");
    assert!(after.contains(&10), "and `fn subtract` still is");
    // The line numbers of what is below the fold are unchanged, which is the ticket's sixth point.
    assert_eq!(
        after.iter().filter(|paragraph| **paragraph >= 9).copied().collect::<Vec<_>>(),
        before.iter().filter(|paragraph| **paragraph >= 9).copied().collect::<Vec<_>>(),
        "every line below the fold keeps the number it had"
    );
    harness.snapshot(shot("folding_recursive_collapsed"));
}

#[test]
fn a_fold_stays_on_its_block_when_a_line_is_typed_above_it() {
    let mut harness = folding_harness("edited");
    harness.get_by_label("Collapse block at line 3").click();
    steady(&mut harness);
    steady(&mut harness);
    // A line typed at the very top of the file, which moves every byte below it.
    harness
        .state_mut()
        .document_mut()
        .apply(unluminous_core::Command::PlaceCaret { offset: 0, extend: false });
    harness
        .state_mut()
        .document_mut()
        .apply(unluminous_core::Command::Insert("// a new line\n".to_owned()));
    steady(&mut harness);
    steady(&mut harness);
    // The same function, now one line further down, and still collapsed: the arrow and the badge.
    assert_eq!(harness.get_all_by_label("Expand block at line 4").count(), 2);
    let showing = laid_out_paragraphs(&harness);
    assert!(showing.contains(&3), "the function's own line is still showing");
    assert!(!showing.contains(&6), "and its body is still hidden");
}

#[test]
fn a_picture_has_no_folding_entries_at_all() {
    // Unluminous's rule for a control that can never apply: absent, not dimmed.
    let mut harness = harness("");
    harness.get_by_label_contains("picture.png").click();
    steady(&mut harness);
    let state = harness.state().menu_state();
    assert!(!state.folding_applies);
    assert!(unluminous_app::app::actions::folding_menu(&state).is_empty());
    assert!(unluminous_app::app::actions::folding_here_menu(&state).is_empty());
}

// -------------------------------------------------------------------------------------- task-1922
//
// `Go to Line` and the bracket pair. WP4's navigation half: two ways of arriving somewhere in the
// file in front of you, each with a menu entry, a chord and a command.

/// A file with brackets in the code, in a comment and in a string, which is the whole difficulty.
fn bracket_folder() -> std::path::PathBuf {
    fixture(
        "unluminous-1922-brackets",
        &[("main.rs", "fn one() {\n    // a } in a comment\n    let a = \"}\";\n}\nfn two() {}\n")],
    )
}

#[test]
fn the_bracket_pair_skips_the_one_in_the_comment_and_the_one_in_the_string() {
    // The reason this is a command rather than a search for the character: a `}` inside `// }` and
    // one inside `"}"` are not brackets, and only a reading that knows comments from code can say
    // so. The tokens come from the colouring, which has already read this file at this revision.
    let mut harness = harness_in(&bracket_folder());
    did(&mut harness, "tab open main.rs --permanent");
    let text = harness.state().document().text().to_string();
    let opener = text.find('{').expect("the opening brace");
    let closer = text.rfind("}\nfn two").expect("the closing brace of the first function");

    let pair = did(&mut harness, &format!("editor bracket --offset {opener}"));
    assert_eq!(pair["from"], opener);
    assert_eq!(pair["to"], closer, "the two inside the body are not brackets");
    assert_eq!(pair["moved"], false, "without --go it reads and changes nothing");
    assert_eq!(harness.state().document().selection().head, 0);

    // Both offsets, because an agent handed only the answer would have to work out where it asked
    // from. And both as line and column, which is what a person reads.
    assert_eq!(pair["fromLine"], 1);
    assert_eq!(pair["toLine"], 4);

    // Asked from the closer it answers about the opener, so the pair is one answer either way.
    let back = did(&mut harness, &format!("editor bracket --offset {closer}"));
    assert_eq!(back["to"], opener);
}

#[test]
fn go_to_matching_bracket_moves_the_caret_and_says_so_when_there_is_nowhere_to_go() {
    let mut harness = harness_in(&bracket_folder());
    did(&mut harness, "tab open main.rs --permanent");
    let text = harness.state().document().text().to_string();
    let opener = text.find('{').expect("the opening brace");
    let closer = text.rfind("}\nfn two").expect("the closing brace");

    did(&mut harness, &format!("editor caret --line 1 --column {}", opener + 1));
    choose(&mut harness, Action::GoToMatchingBracket);
    assert_eq!(harness.state().document().selection().head, closer);

    // The command line reaches the same place through the same function.
    did(&mut harness, &format!("editor caret --line 1 --column {}", opener + 1));
    let went = did(&mut harness, "editor bracket --go");
    assert_eq!(went["moved"], true);
    assert_eq!(harness.state().document().selection().head, closer);

    // A caret that is not beside a bracket is told so rather than left wondering.
    did(&mut harness, "editor caret --line 1 --column 1");
    assert_eq!(refused(&mut harness, "editor bracket"), "not-found");
    choose(&mut harness, Action::GoToMatchingBracket);
    let said = harness.state().message.clone().unwrap_or_default();
    assert!(said.contains("bracket"), "the status bar should say why: {said}");
}

#[test]
fn go_to_line_takes_a_line_or_a_line_and_a_column() {
    let folder =
        fixture("unluminous-1922-go-to-line", &[("main.rs", "one\ntwo\nthree\nfour\nfive\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open main.rs --permanent");

    // The menu entry opens the prompt, seeded with the line the status bar is already showing, so
    // the chord followed by Enter is not a jump to somewhere else.
    choose(&mut harness, Action::GoToLine);
    let prompt = harness.state().prompt.clone().expect("the prompt is open");
    assert_eq!(prompt.title, "Go to Line");
    assert_eq!(prompt.value, "1");

    let mut typed = prompt;
    typed.value = "3".to_owned();
    harness.state_mut().run_prompt_for_test(typed.clone());
    steady(&mut harness);
    assert_eq!(did(&mut harness, "editor caret")["line"], 3);

    typed.value = "4:3".to_owned();
    harness.state_mut().run_prompt_for_test(typed.clone());
    steady(&mut harness);
    let at = did(&mut harness, "editor caret");
    assert_eq!(at["line"], 4);
    assert_eq!(at["column"], 3);

    // A line past the end is the end rather than a refusal, which is what every editor does and
    // what somebody typing 9999 means.
    typed.value = "9999".to_owned();
    harness.state_mut().run_prompt_for_test(typed.clone());
    steady(&mut harness);
    assert_eq!(did(&mut harness, "editor caret")["line"], 6);

    // And something that is not a line number says so in the status bar rather than moving.
    typed.value = "banana".to_owned();
    harness.state_mut().run_prompt_for_test(typed);
    steady(&mut harness);
    let said = harness.state().message.clone().unwrap_or_default();
    assert!(said.contains("banana"), "the refusal quotes what was typed: {said}");
}

#[test]
fn alt_and_an_arrow_moves_the_line_without_moving_the_caret_off_it() {
    // The one WP4 chord pair with no menu entry, so it is read before the panes are drawn — and the
    // key is taken out of the frame, or the same press would also move the caret up a line.
    let folder = fixture("unluminous-1922-move-lines", &[("main.rs", "one\ntwo\nthree\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open main.rs --permanent");
    did(&mut harness, "editor caret --line 2 --column 1");
    harness.state_mut().focus = unluminous_app::app::Focus::Editor;
    steady(&mut harness);

    harness.event(egui::Event::Key {
        key: egui::Key::ArrowUp,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers { alt: true, ..Modifiers::NONE },
    });
    steady(&mut harness);
    assert_eq!(harness.state().document().text().to_string(), "two\none\nthree\n");
    assert_eq!(
        did(&mut harness, "editor caret")["line"],
        1,
        "the caret went with the line rather than up off it"
    );
}

// -------------------------------------------------------------------------------------- task-1984
//
// Work is never lost: closing the window, and a tab whose save failed.

/// Type into a file, close the window, and the text is on the disk.
///
/// `task-1984` A2. The cross in the title bar, `Action::CloseWindow`, `Action::Quit` and `on_exit`
/// all wrote the settings and the project state and sent `ViewportCommand::Close` without once
/// asking `Document::is_modified`, so a person who typed and pressed the cross lost the edits with
/// nothing on the screen to say so -- and the project state written a line earlier recorded the file
/// as open, so it came back the next day showing the disk.
#[test]
fn closing_the_window_writes_every_modified_tab() {
    let folder = fixture(
        "unluminous-1984-close-saves",
        &[("one.md", "# One\n"), ("two.md", "# Two\n"), ("three.md", "# Three\n")],
    );
    let mut harness = harness_in(&folder);
    for name in ["one.md", "two.md", "three.md"] {
        did(&mut harness, &format!("tab open {name} --permanent"));
        did(&mut harness, "editor caret --line 1 --column 1");
        did(&mut harness, "editor insert edited");
    }
    assert_eq!(
        harness.state().files.iter().filter(|file| file.document.is_modified()).count(),
        3,
        "three tabs with unsaved changes is what this is about"
    );

    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::CloseWindow, &ctx);
    steady(&mut harness);

    // Every one of them, not only the tab with the keyboard: a person with three edited tabs
    // pressing the cross means all three, and there is no dialog here to ask them which.
    for name in ["one.md", "two.md", "three.md"] {
        let on_disk = std::fs::read_to_string(folder.join(name)).expect("the file is still there");
        assert!(
            on_disk.starts_with("edited"),
            "{name} should hold what was typed, and holds {on_disk:?}"
        );
    }
    assert!(
        harness.state().files.iter().all(|file| !file.document.is_modified()),
        "and nothing is left saying it has unsaved changes"
    );
}

/// A tab Unluminous could not write stays open, and says so in a notice a person has to dismiss.
///
/// `task-1984` A3: `save_before_closing` put the failed write in the status bar and `close_tab`
/// closed the tab on the next line, so a read only file, a full disk or a file in an encoding
/// Unluminous only reads took the typing with it. `tab close --discard` is still the way to close it
/// anyway, which is what makes refusing here the right answer rather than a dialog.
#[test]
fn a_tab_whose_save_failed_stays_open() {
    let folder = fixture("unluminous-1984-save-failed", &[("locked.md", "# Locked\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open locked.md --permanent");
    did(&mut harness, "editor caret --line 1 --column 1");
    did(&mut harness, "editor insert edited");

    // A file that cannot be written. Read only on both platforms, which is what a person meets far
    // more often than a full disk and is the one of the three a test can make.
    let file = folder.join("locked.md");
    let mut permissions = std::fs::metadata(&file).expect("the file").permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&file, permissions).expect("make it read only");

    let before = harness.state().files.len();
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::CloseTab, &ctx);
    steady(&mut harness);

    assert_eq!(harness.state().files.len(), before, "the tab is still there");
    assert!(
        harness.state().document().is_modified(),
        "and it still holds what was typed, rather than having been closed and lost"
    );
    let notice = harness
        .state()
        .toasts
        .notices()
        .iter()
        .map(|notice| notice.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(notice.contains("locked.md"), "the notice names the file: {notice:?}");

    // And the window does not close either, for the same reason.
    harness.state_mut().run_action(Action::CloseWindow, &ctx);
    steady(&mut harness);
    assert!(!harness.state().closing, "the window stays while a tab could not be written");

    let mut permissions = std::fs::metadata(&file).expect("the file").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&file, permissions).expect("put it back");
}

/// A rename across a project that cannot write one of the files leaves that file as it was.
///
/// `task-1984` A10. `task-1922` B7 made the files Unluminous remembers itself in atomic and left the
/// four that write a person's **code** as one `std::fs::write` each — a rename, Replace All, a file
/// move, and `Document::save_as`. A `write` truncates and then fills, so a crash or a full disk part
/// way through a rename across forty files leaves one of them at zero length, and the buffer it was
/// built from has already gone because the file was never open.
///
/// A read only file stands in for the crash: the write is refused rather than interrupted, which is
/// the same question asked of the same code — does the old file survive a write that did not finish.
#[test]
fn a_source_file_write_that_fails_leaves_the_file_as_it_was() {
    let folder = fixture(
        "unluminous-1984-atomic-source",
        &[
            ("first.rs", "pub fn draw_everything() {}\n"),
            ("second.rs", "pub fn other() { draw_everything(); }\n"),
        ],
    );
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open first.rs --permanent");
    did(&mut harness, "editor caret --line 1 --column 8");

    let second = folder.join("second.rs");
    let was = std::fs::read_to_string(&second).expect("read it before");
    let mut permissions = std::fs::metadata(&second).expect("the file").permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&second, permissions).expect("make it read only");

    // The rename walks the project and rewrites every file that names it. One of them cannot be
    // written, and what matters is what is left in that file afterwards.
    // `editor rename` is answered on a later frame -- it reads the project on a thread -- so the
    // reply is not what this is about. What is asserted is what is in the file afterwards.
    let ctx = harness.ctx.clone();
    let _ = harness
        .state_mut()
        .run_command_line("editor rename draw_the_lot --name draw_everything --json", &ctx);
    for _ in 0..60 {
        pump(&mut harness);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let now = std::fs::read_to_string(&second).expect("the file is still readable");
    assert_eq!(
        now, was,
        "a file that could not be written is left exactly as it was, rather than at zero length"
    );
    assert!(!now.is_empty(), "and in particular it is not empty");

    let mut permissions = std::fs::metadata(&second).expect("the file").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&second, permissions).expect("put it back");
}
