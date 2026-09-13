//! The command line, driven through a real window.
//!
//! These go through the whole of the command line path apart from the socket: the words are parsed
//! against `unluminous_cli::catalogue`, dispatched by `UnluminousApp::run_cli`, and what is checked
//! afterwards is the window's own state — not the reply's opinion of itself. A command that says it
//! opened a file and a window with that file open are two different claims, and only the second one
//! is worth testing.
//!
//! **Not one of the 38 tests here takes a picture.** What a command does to the rendering is
//! covered by the files beside this one, which set the same states up by hand; what is unproven, and
//! what these prove, is that the command line reaches those states at all.

mod common;

use common::*;

use egui::{vec2, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::app::actions::Action;
use unluminous_app::app::ViewMode;
use unluminous_app::settings;
use unluminous_app::UnluminousApp;
use unluminous_core::Command;

/// Build a project that initially belongs to an ancestor repository.
///
/// **Not [`fixture`], because the point of it is the repository rather than the file**: `git init`
/// runs in the folder *above* the project.
fn late_repository_project() -> (std::path::PathBuf, std::path::PathBuf) {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_nanos();
    let ancestor = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("_agent_output/task-1701-git-root-refresh/tests")
        .join(format!("{}-{unique}", std::process::id()));
    let project = ancestor.join("project");
    std::fs::create_dir_all(&project).expect("make the late repository project");
    let initialized = unluminous_git::command::run(&ancestor, &["init", "--initial-branch=main"]);
    assert!(initialized.ok, "initialize the ancestor: {}", initialized.message());
    std::fs::write(project.join("before.txt"), "before repository creation\n")
        .expect("write before.txt");
    (ancestor, project)
}

/// A project that becomes a repository while its window is open switches away from its ancestor.
#[test]
fn git_status_rediscovers_a_repository_created_after_the_window_opened() {
    let (ancestor, project) = late_repository_project();
    let mut harness = harness_in(&project);
    settle(&mut harness, "the ancestor repository", |app| {
        app.git.as_ref().is_some_and(|git| !git.is_busy())
    });
    let before = did(&mut harness, "git status --json");
    assert_eq!(before["rootRelation"], "ancestor");
    assert_eq!(
        std::path::PathBuf::from(before["root"].as_str().unwrap()).canonicalize().unwrap(),
        ancestor.canonicalize().unwrap()
    );

    let initialized = unluminous_git::command::run(&project, &["init", "--initial-branch=master"]);
    assert!(initialized.ok, "initialize the project: {}", initialized.message());
    std::fs::write(project.join("after.txt"), "after repository creation\n")
        .expect("write after.txt");
    let ctx = harness.ctx.clone();
    assert!(harness.state_mut().run_command_line("git status --json", &ctx).is_none());
    settle(&mut harness, "the project repository", |app| {
        app.git.as_ref().is_some_and(|git| {
            !git.is_busy()
                && unluminous_app::app::git::root_relation(git.repository.root(), &project)
                    == unluminous_app::app::git::RootRelation::Project
        })
    });

    let after = did(&mut harness, "git status --json");
    assert_eq!(after["rootRelation"], "project");
    assert_eq!(after["branch"], "master");
    assert_eq!(after["changed"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        std::path::PathBuf::from(after["root"].as_str().unwrap()).canonicalize().unwrap(),
        project.canonicalize().unwrap()
    );

    let window = did(&mut harness, "status --json");
    assert_eq!(window["git"]["rootRelation"], "project");
    assert_eq!(window["git"]["root"], after["root"]);
    did_while_waiting(&mut harness, "git action add --path after.txt");
    settle(&mut harness, "the project file to be staged", |app| {
        app.git.as_ref().is_some_and(|git| !git.is_busy())
    });
    assert_eq!(ask_git(&project, &["diff", "--cached", "--name-only"]), "after.txt");
}

/// Press at `from` and move through each of `path` before letting go.
///
/// [`drag`] moves once, which is enough for a divider: it starts the drag and ends it in the same
/// motion. The editing area needs more, because the frame a drag *starts* on is the frame the caret
/// is placed on, and it takes a second movement before there is anything selected — which is exactly
/// what a person does with the mouse and is the gesture `task-1666` reported.
fn drag_through(
    harness: &mut Harness<'static, UnluminousApp>,
    from: egui::Pos2,
    path: &[egui::Pos2],
) {
    let modifiers = Modifiers::default();
    harness.input_mut().events.push(egui::Event::PointerMoved(from));
    harness.run();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    harness.run();
    for at in path {
        harness.input_mut().events.push(egui::Event::PointerMoved(*at));
        harness.run();
    }
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: *path.last().unwrap_or(&from),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.run();
}

/// A folder holding one long source file, for the `task-1666` performance tests.
///
/// Written under a name of its own, which is what `git_folder` already does and what keeps two
/// tests from writing over one another.
fn a_folder_with_a_long_source_file(name: &str) -> std::path::PathBuf {
    let source: String = (0..600)
        .map(|i| format!("/// The {i}th one.\nfn line_{i}(value: usize) -> usize {{\n    value + {i}\n}}\n\n"))
        .collect();
    fixture(&format!("unluminous-performance/{name}"), &[("long.rs", &source)])
}

/// `task-1666`. **Dragging a selection must lay nothing out again and colour nothing again.**
///
/// This is the gesture the ticket reported, and the fault behind it was that moving the caret counted
/// as a change to the text: `refresh_layout` and `colour_the_file` were both keyed on
/// `Document::revision()`, which a caret move bumps. So every frame of a drag re-tokenised the file,
/// rebuilt every style span and laid the whole document out — about 650 ms a frame on a file the size
/// of `app/mod.rs`.
#[test]
fn dragging_a_selection_lays_nothing_out_again_and_colours_nothing_again() {
    let folder = a_folder_with_a_long_source_file("drag-selection");
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open long.rs --permanent");
    harness.run();
    // The file is coloured on the frame after it is opened, so let that happen before anything is
    // measured; otherwise the first drag frame would be charged with work the opening owed.
    harness.run();

    let laid_out = harness.state().files.active().cached.laid_out_revision;
    let coloured = harness.state().files.active().coloured_revision;
    let text_revision = harness.state().document().text_revision();
    let revision = harness.state().document().revision();
    assert!(coloured.is_some(), "a .rs file is coloured by the bundled plugin");

    let area = harness.state().editor_area();
    drag_through(
        &mut harness,
        area.left_top() + vec2(60.0, 30.0),
        &[
            area.left_top() + vec2(120.0, 90.0),
            area.left_top() + vec2(180.0, 170.0),
            area.left_top() + vec2(220.0, 260.0),
        ],
    );
    harness.run();

    assert!(
        !harness.state().document().selection().is_empty(),
        "the drag should have selected some text"
    );
    assert_eq!(
        harness.state().document().text_revision(),
        text_revision,
        "dragging a selection changes no text"
    );
    assert_eq!(
        harness.state().files.active().cached.laid_out_revision,
        laid_out,
        "so the document was not laid out again"
    );
    assert_eq!(
        harness.state().files.active().coloured_revision,
        coloured,
        "and it was not coloured again"
    );
    assert_ne!(
        harness.state().document().revision(),
        revision,
        "the window still knows it has something new to paint"
    );
}

/// The other half: typing really does change the text, so it is laid out again — but only the
/// paragraph that changed. Every other line keeps the position it already had.
#[test]
fn typing_a_letter_lays_out_the_line_it_was_typed_into_and_leaves_the_rest_alone() {
    let folder = a_folder_with_a_long_source_file("typing");
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open long.rs --permanent");
    harness.run();
    harness.run();

    let before: Vec<f32> = harness.state().layout().lines.iter().map(|line| line.y).collect();
    let count = before.len();
    assert!(count > 2000, "the fixture is meant to be long: {count} lines");

    // Into the middle of the file, so there is plenty above it and plenty below it.
    let middle = harness.state().document().text().len_bytes() / 2;
    harness.state_mut().command(Command::PlaceCaret { offset: middle, extend: false });
    harness.run();
    harness.state_mut().command(Command::Insert("X".to_owned()));
    harness.run();

    let after: Vec<f32> = harness.state().layout().lines.iter().map(|line| line.y).collect();
    assert_eq!(after.len(), count, "a letter typed into a line adds no lines");
    assert_eq!(after, before, "and moves none of them");
    assert!(
        harness.state().document().text().to_string().contains("X"),
        "the letter really was typed"
    );
}

#[test]
fn the_command_line_opens_a_file_into_a_tab() {
    let mut harness = harness_in(&sample_folder());
    let result = did(&mut harness, "tab open readme.md");
    assert!(result["path"].as_str().unwrap().ends_with("readme.md"));
    let app = harness.state();
    assert_eq!(app.files.active().name(), "readme.md");
    assert!(
        app.document().text().to_string().contains('#'),
        "the file's real text should be in the document"
    );
}

#[test]
fn a_file_that_is_not_there_is_refused_with_a_code_a_script_can_match_on() {
    let mut harness = harness_in(&sample_folder());
    assert_eq!(refused(&mut harness, "tab open nowhere.md"), "not-found");
    assert_eq!(refused(&mut harness, "tab show 99"), "not-found");
    assert_eq!(refused(&mut harness, "settings get appearance.font.colour"), "not-found");
    assert_eq!(refused(&mut harness, "editor undo"), "not-applicable");
}

#[test]
fn the_command_line_moves_between_tabs_by_number_and_by_name() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open readme.md --permanent");
    did(&mut harness, "tab open notes.txt --permanent");
    assert_eq!(harness.state().files.len(), 2);
    did(&mut harness, "tab show readme.md");
    assert_eq!(harness.state().files.active().name(), "readme.md");
    did(&mut harness, "tab next");
    assert_eq!(harness.state().files.active().name(), "notes.txt");
    did(&mut harness, "tab show 0");
    assert_eq!(harness.state().files.active().name(), "readme.md");
}

#[test]
fn the_command_line_types_into_the_document_and_undoes_it() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open notes.txt --permanent");
    let before = harness.state().document().text().to_string();
    did(&mut harness, "editor caret --line 1 --column 1");
    did(&mut harness, "editor insert MARKER");
    assert!(harness.state().document().text().to_string().starts_with("MARKER"));
    did(&mut harness, "editor undo");
    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "one undo should put the whole insertion back"
    );
    assert!(!harness.state().document().is_modified(), "the restored disk state is clean");
    did(&mut harness, "tab reload");
    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "a clean tab reloads without --discard"
    );
}

#[test]
fn the_caret_lands_where_a_line_and_a_column_say() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open notes.txt --permanent");
    // The sample files are one line each, so the document is given some lines to aim at first.
    did(&mut harness, "editor set-text alpha\\nbravo\\ncharlie");
    let result = did(&mut harness, "editor caret --line 2 --column 3");
    assert_eq!(result["line"], serde_json::json!(2));
    assert_eq!(result["column"], serde_json::json!(3));
    let at = harness.state().caret_position();
    assert_eq!((at.line, at.column), (2, 3), "the status bar should agree");
    // Past the end of a line lands at the end of it rather than being refused.
    let far = did(&mut harness, "editor caret --line 1 --column 99");
    assert_eq!(far["column"], serde_json::json!(6), "the end of `alpha`");
}

#[test]
fn the_command_line_replaces_the_whole_document_in_one_undo_step() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open notes.txt --permanent");
    let before = harness.state().document().text().to_string();
    did(&mut harness, "editor set-text one\\ntwo");
    assert_eq!(harness.state().document().text().to_string(), "one\ntwo");
    did(&mut harness, "editor undo");
    assert_eq!(harness.state().document().text().to_string(), before);
}

#[test]
fn the_command_line_indents_the_selection_the_way_the_keys_do() {
    // The agent's half of `task-1747`, through the window's own dispatcher: the same command the
    // keys apply, so an indent done by an agent and the same thing done by hand are the same thing.
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open notes.txt --permanent");
    did(&mut harness, "editor set-text one\\ntwo\\nthree");
    did(&mut harness, "editor select --all");
    let result = did(&mut harness, "editor indent");
    assert_eq!(result["lines"], serde_json::json!(3), "all three lines, one step");
    assert_eq!(result["unit"], serde_json::json!("tab"));
    assert_eq!(
        harness.state().document().text().to_string(),
        "\tone\n\ttwo\n\tthree",
        "a tab at the start of each line"
    );
    did(&mut harness, "editor undo");
    assert_eq!(
        harness.state().document().text().to_string(),
        "one\ntwo\nthree",
        "one undo put every line back"
    );
    // The space half, which is what the Space key does.
    did(&mut harness, "editor select --all");
    did(&mut harness, "editor indent --space");
    assert_eq!(harness.state().document().text().to_string(), " one\n two\n three");
    // With nothing selected, the line the caret is on: the tab lands in front of the space that
    // line already carries.
    did(&mut harness, "editor caret --line 2 --column 1");
    did(&mut harness, "editor indent");
    assert_eq!(harness.state().document().text().to_string(), " one\n\t two\n three");
}

#[test]
fn a_view_mode_that_cannot_apply_to_this_file_is_refused_rather_than_silently_ignored() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open readme.md --permanent");
    did(&mut harness, "editor view preview");
    assert_eq!(harness.state().view_mode(), ViewMode::Preview);
    did(&mut harness, "tab open program.rs --permanent");
    assert_eq!(refused(&mut harness, "editor view preview"), "not-applicable");
    assert_eq!(harness.state().view_mode(), ViewMode::Raw, "and nothing changed");
}

#[test]
fn a_setting_changed_from_the_command_line_reaches_every_open_tab() {
    // The same rule `set_the_font_everywhere` exists for: the editor's font is one setting for the
    // window, so a change from the command line must not reach only the tab that happens to show.
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open readme.md --permanent");
    did(&mut harness, "tab open notes.txt --permanent");
    did(&mut harness, "settings set appearance.font.size 24");
    assert_eq!(harness.state().settings.font_size, 24.0);
    for file in harness.state().files.iter() {
        assert_eq!(
            file.document.active_style().size,
            24.0,
            "{} was left in the old size",
            file.name()
        );
    }
}

#[test]
fn a_setting_outside_its_limits_is_brought_inside_and_one_that_is_not_a_number_is_refused() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "settings set appearance.background.opacity 9");
    assert_eq!(harness.state().settings.opacity, 1.0, "clamped, not refused");
    assert_eq!(refused(&mut harness, "settings set appearance.font.size huge"), "usage");
    assert_eq!(refused(&mut harness, "settings set editor.line_numbers maybe"), "usage");
}

#[test]
fn the_command_line_puts_the_panes_where_it_is_told() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "explorer hide");
    assert!(!harness.state().explorer_visible);
    did(&mut harness, "explorer show");
    assert!(harness.state().explorer_visible);
    did(&mut harness, "explorer width 400");
    assert_eq!(harness.state().panes.explorer_width, 400.0);
    did(&mut harness, "explorer width 9999");
    assert_eq!(
        harness.state().panes.explorer_width,
        settings::EXPLORER_MAX,
        "a pane dragged past its limit comes back inside, however it was dragged"
    );
}

#[test]
fn the_explorer_filter_and_the_tree_are_readable_and_writable_from_the_command_line() {
    let mut harness = harness_in(&sample_folder());
    let result = did(&mut harness, "explorer filter notes");
    assert!(result["matches"].as_u64().unwrap() >= 1);
    assert_eq!(harness.state().filter, "notes");
    did(&mut harness, "explorer filter");
    assert!(harness.state().filter.is_empty(), "no text clears the box");
    let tree = did(&mut harness, "explorer tree --limit 5");
    assert!(tree["total"].as_u64().unwrap() > 0);
    assert!(tree["rows"].as_array().unwrap().len() <= 5, "the limit is honoured");
}

#[test]
fn go_to_file_is_opened_populated_and_accepted_from_the_command_line() {
    let mut harness = harness_in(&sample_folder());
    let opened = did(&mut harness, "modal open go-to-file --query readme");
    assert!(opened["results"].as_u64().unwrap() >= 1);
    assert!(harness.state().go_to_file.is_some(), "the modal is really open");
    let results = did(&mut harness, "modal results --limit 5");
    assert_eq!(results["results"][0]["name"], serde_json::json!("readme.md"));
    let accepted = did(&mut harness, "modal accept 0");
    assert!(accepted["path"].as_str().unwrap().ends_with("readme.md"));
    assert!(harness.state().go_to_file.is_none(), "accepting shuts it");
    assert_eq!(harness.state().files.active().name(), "readme.md");
}

#[test]
fn every_modal_reports_which_one_is_open_and_shuts_when_it_is_cancelled() {
    let mut harness = harness_in(&sample_folder());
    assert_eq!(did(&mut harness, "modal state")["open"], serde_json::Value::Null);
    for (name, extra) in [("go-to-file", ""), ("settings", ""), ("find-in-files", "")] {
        did(&mut harness, &format!("modal open {name} {extra}"));
        assert_eq!(
            did(&mut harness, "modal state")["open"],
            serde_json::json!(name),
            "{name} should say it is the one that is open"
        );
        did(&mut harness, "modal cancel");
        assert_eq!(did(&mut harness, "modal state")["open"], serde_json::Value::Null);
    }
}

#[test]
fn the_settings_modal_opens_on_the_page_it_is_asked_for() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "modal open settings --page terminal");
    assert!(harness.state().settings_window.open);
    assert_eq!(harness.state().settings_window.page, settings::Page::Terminal);
    // Every page the window has, including the one `task-1679` added: a page reachable by hand and
    // not from the command line would be the thing the rule at the top of `app/cli.rs` forbids.
    for (named, page) in [
        ("appearance", settings::Page::Appearance),
        ("editor", settings::Page::Editor),
        ("plugins", settings::Page::Plugins),
        ("mcp", settings::Page::Mcp),
    ] {
        did(&mut harness, &format!("modal open settings --page {named}"));
        assert_eq!(harness.state().settings_window.page, page, "--page {named}");
    }
    assert_eq!(refused(&mut harness, "modal open settings --page nonsense"), "usage");
}

#[test]
fn the_terminal_is_opened_and_put_away_from_the_command_line() {
    let mut harness = harness_in(&sample_folder());
    // A shell of its own is not started here: what a real shell has said by any given frame is not
    // something a test can know, which is the rule the terminal's own screenshot tests keep. What is
    // under test is that the commands reach the panel.
    harness.state_mut().new_detached_terminal_tab(6, 40);
    harness.run();
    assert!(harness.state().terminal.visible);
    did(&mut harness, "terminal hide");
    assert!(!harness.state().terminal.visible);
    did(&mut harness, "terminal height 400");
    assert_eq!(harness.state().panes.terminal_height, 400.0);
    let listed = did(&mut harness, "terminal list");
    assert_eq!(listed["count"], serde_json::json!(1));
    did(&mut harness, "terminal close");
    assert_eq!(did(&mut harness, "terminal list")["count"], serde_json::json!(0));
}

/// `task-1705`. The two verbs where targeting matters most — `send` and `read` — take `--tab`, and
/// naming a tab does not show it. Two detached tabs, each with its own content, and the second is the
/// one showing: the test speaks to the first without the second ever being selected.
///
/// A detached tab has no shell, so `send` has no bytes to deliver here — what is under test is the
/// addressing, which is the new logic, and the reply says which tab it went to. That a `send`
/// reaches the shell it names is what the live verification does with two real shells.
#[test]
fn the_command_line_speaks_to_a_terminal_tab_that_is_not_showing() {
    let mut harness = harness_in(&sample_folder());
    harness.state_mut().new_detached_terminal_tab(8, 60);
    harness.state_mut().new_detached_terminal_tab(8, 60);
    harness.run();
    // Each tab gets content of its own, reached by number, and the second is the one showing.
    harness
        .state_mut()
        .terminal
        .tabs
        .at_mut(0)
        .expect("the first tab")
        .feed(b"the first shell is here\r\n");
    harness
        .state_mut()
        .terminal
        .tabs
        .at_mut(1)
        .expect("the second tab")
        .feed(b"the second shell is here\r\n");
    harness.run();
    assert_eq!(
        harness.state().terminal.tabs.active_index(),
        1,
        "the second tab is the one showing"
    );

    // `send --tab 0` is addressed to the first tab and says so, and leaves the showing tab alone.
    let sent = did(&mut harness, "terminal send --tab 0 echo from the first");
    assert_eq!(sent["tab"], serde_json::json!(0), "the reply names the tab it went to");
    assert_eq!(
        harness.state().terminal.tabs.active_index(),
        1,
        "naming a tab to send to does not show it"
    );
    // And with no --tab, a send goes to the tab that is showing, which is the old behaviour.
    let sent_default = did(&mut harness, "terminal send echo from the showing one");
    assert_eq!(sent_default["tab"], serde_json::json!(1));

    // `read --tab 0` reads the first tab's screen without showing it, and the showing tab's content
    // is not what comes back.
    let read = did(&mut harness, "terminal read --tab 0");
    let text = read["text"].as_str().expect("the screen as text");
    assert!(text.contains("the first shell is here"), "{text:?}");
    assert!(!text.contains("the second shell is here"), "{text:?}");
    assert_eq!(harness.state().terminal.tabs.active_index(), 1, "reading a tab does not show it");
    // And a read with no --tab reads the tab that is showing.
    let read_default = did(&mut harness, "terminal read");
    assert!(read_default["text"]
        .as_str()
        .expect("the screen as text")
        .contains("the second shell is here"));

    // `--wait-for` is answered by the named tab: the text is on the first tab, not the showing one.
    let found = did(&mut harness, "terminal read --tab 0 --wait-for \"the first shell\"");
    assert_eq!(found["found"], serde_json::json!(true));

    // And the race the ticket is about: a wait for text that is on the showing tab but not on the
    // named tab is not answered by the showing tab. It is held, because the wait was pinned to the
    // first tab when it was asked and keeps looking there.
    let ctx = harness.ctx.clone();
    let held = harness
        .state_mut()
        .run_command_line("terminal read --tab 0 --wait-for \"the second shell\"", &ctx);
    assert!(
        held.is_none(),
        "the wait is held, because the second shell's text is not on the first tab"
    );

    // `select --tab` shows the named tab, and is still the only verb that does.
    did(&mut harness, "terminal select --tab 0");
    assert_eq!(harness.state().terminal.tabs.active_index(), 0);
    // `close --tab` closes the named tab.
    did(&mut harness, "terminal close --tab 0");
    assert_eq!(harness.state().terminal.tabs.count(), 1);
    assert_eq!(harness.state().terminal.tabs.active_index(), 0, "the one left is now the only one");

    // A --tab past the end is refused rather than reaching for the tab that is showing.
    assert_eq!(refused(&mut harness, "terminal send --tab 9 echo"), "not-found");
    assert_eq!(refused(&mut harness, "terminal read --tab 9"), "not-found");
}

/// `task-1705`. The flag is the settled way of naming a tab and the positional is what the old
/// callers have, so when both are given the flag wins.
#[test]
fn the_tab_flag_wins_over_the_positional_when_both_are_given() {
    let mut harness = harness_in(&sample_folder());
    for _ in 0..3 {
        harness.state_mut().new_detached_terminal_tab(8, 60);
    }
    harness.run();
    // Three tabs, the third showing. `select 0 --tab 1` names the first tab with the positional and
    // the second with the flag, and it is the second that is shown.
    did(&mut harness, "terminal select 0 --tab 1");
    assert_eq!(
        harness.state().terminal.tabs.active_index(),
        1,
        "the flag names the tab, not the positional"
    );
}

#[test]
fn every_entry_on_every_menu_is_listed_and_can_be_run_by_name() {
    // The rule `task-1661` asks for, checked against the real menus rather than against a list.
    let mut harness = harness_in(&sample_folder());
    let listed = did(&mut harness, "action list");
    let actions = listed["actions"].as_array().expect("an array").clone();
    assert!(actions.len() > 30, "the menus hold more than that");
    for name in ["toggle-explorer", "toggle-line-numbers", "about", "view-preview", "git-commit"] {
        assert!(
            actions.iter().any(|entry| entry["name"] == serde_json::json!(name)),
            "{name} should be on the list"
        );
    }
    let before = harness.state().settings.line_numbers;
    did(&mut harness, "action run toggle-line-numbers");
    assert_eq!(harness.state().settings.line_numbers, !before);
}

#[test]
fn the_three_actions_that_would_open_a_file_chooser_are_refused_with_the_command_to_use_instead() {
    // A file chooser asked for from a script is a window nobody is looking at.
    let mut harness = harness_in(&sample_folder());
    for (name, instead) in
        [("open-file", "tab open"), ("open-folder", "project open"), ("save-as", "tab save-as")]
    {
        let reply = run(&mut harness, &format!("action run {name}"));
        assert!(!reply.ok, "{name} should be refused");
        assert!(
            reply.message.contains(instead),
            "{name}'s refusal should name `{instead}`, and said: {}",
            reply.message
        );
    }
}

#[test]
fn status_answers_for_every_part_of_the_window_at_once() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open readme.md --permanent");
    let status = did(&mut harness, "status");
    for part in ["project", "tabs", "editor", "explorer", "terminal", "modal", "settings", "git"] {
        assert!(status.get(part).is_some(), "status should carry {part}");
    }
    assert_eq!(status["tabs"].as_array().unwrap().len(), harness.state().files.len());
    assert!(status["project"].as_str().unwrap().contains("unluminous-screenshot-folder"));
}

// ---------------------------------------------------------------------------------------------
// task-1704: a reply proportionate to what was asked.

/// `status --section` answers with the part that was asked for and nothing else, and the whole
/// window is still the answer when no section is named.
#[test]
fn status_answers_for_the_section_that_was_asked_for() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open readme.md --permanent");

    let panes = did(&mut harness, "status --section panes");
    assert!(panes.get("panes").is_some(), "the section that was asked for");
    for absent in ["editor", "settings", "git", "explorer", "terminal", "tabs"] {
        assert!(panes.get(absent).is_none(), "nothing that was not asked for: {absent}");
    }

    // Several at once, in either case, because an agent writes `view` where the menu is `View`.
    let asked = did(&mut harness, "status --section Editor,GIT");
    assert!(asked.get("editor").is_some());
    assert!(asked.get("git").is_some());
    assert!(asked.get("settings").is_none());
    assert!(asked.get("panes").is_none());

    // The sentence is one line about the whole window and does not change with the section.
    let whole = did(&mut harness, "status");
    for part in
        ["project", "tabs", "editor", "explorer", "terminal", "modal", "settings", "git", "panes"]
    {
        assert!(whole.get(part).is_some(), "no section is still the whole window: {part}");
    }
    assert_eq!(
        whole["panes"]["count"], panes["panes"]["count"],
        "the section and the whole agree about the window"
    );

    // A section that is not a section is a question that was not asked.
    assert_eq!(refused(&mut harness, "status --section purple"), "usage");
}

/// A fold command that changed something answers with the summary, and the region list comes
/// along only when it is asked for.
#[test]
fn a_fold_change_answers_with_a_summary_and_the_list_is_opt_in() {
    let mut harness = folding_harness("proportionate");

    // `fold list` is the command whose job is the list, so it is what says how many blocks there
    // are, and it keeps returning the list whatever else changes.
    let listed = did(&mut harness, "fold list");
    let total = listed["regions"].as_array().expect("the list").len();
    assert!(total >= 4, "the file has several blocks: {total}");

    let collapsed = did(&mut harness, "fold collapse --all");
    assert_eq!(collapsed["total"], total);
    assert_eq!(collapsed["collapsed"], total);
    assert!(collapsed.get("regions").is_none(), "the list is not the answer to a change");

    let expanded = did(&mut harness, "fold expand --all --regions");
    assert_eq!(expanded["collapsed"], 0);
    let regions = expanded["regions"].as_array().expect("the list was asked for");
    assert_eq!(regions.len(), total);
    assert!(regions.iter().all(|region| region["collapsed"] == serde_json::json!(false)));

    // One block at a time, the way the study's agent asked for it, with no list either way.
    let one = did(&mut harness, "fold collapse --line 3");
    assert_eq!(one["collapsed"], 1);
    assert!(one.get("regions").is_none());
    let toggled = did(&mut harness, "fold toggle --line 3");
    assert_eq!(toggled["collapsed"], 0);
}

/// `editor complete` prints fifty rows and says how many there were. `task-1804` §7.4.
///
/// The one command `task-1704` left out of its own rule. `--stem a` on this project used to return
/// **1.28 MB** -- roughly 320,000 tokens, more than any model's context -- out of one keystroke's
/// worth of stem, because `--limit`'s documented default was "all of them".
///
/// What is asserted is the shape rather than a number of matches, so the test does not depend on how
/// many words happen to be in the fixture: the rows are capped, the total is reported, `--limit 0`
/// still gives everything, and a smaller `--limit` is still honoured.
#[test]
fn editor_complete_answers_in_a_payload_proportionate_to_the_question() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open program.rs --permanent");

    let all = did(&mut harness, "editor complete --stem a --limit 0");
    let total = all["total"].as_u64().expect("the total") as usize;
    assert_eq!(
        all["rows"].as_array().expect("the rows").len(),
        total,
        "--limit 0 is still every row, so nothing has been taken away"
    );

    let capped = did(&mut harness, "editor complete --stem a");
    assert_eq!(capped["total"], all["total"], "the count does not change with the cap");
    let shown = capped["rows"].as_array().expect("the rows").len();
    assert!(shown <= 50, "at most fifty rows without being asked: {shown}");
    assert_eq!(capped["shown"], serde_json::json!(shown), "and the reply says how many were shown");
    if total > 50 {
        assert_eq!(shown, 50, "fifty of them when there are more than fifty");
        assert!(
            capped["message"].as_str().unwrap_or_default().contains("--limit"),
            "and it says how to ask for the rest: {}",
            capped["message"]
        );
    }

    // An explicit smaller limit is still honoured, which is what it always did.
    let three = did(&mut harness, "editor complete --stem a --limit 3");
    assert!(three["rows"].as_array().expect("the rows").len() <= 3);
    assert_eq!(three["total"], all["total"]);
}

/// `action list --menu` answers with the menu that was asked for and nothing else, and every
/// menu is still the answer when none is named.
#[test]
fn action_list_answers_for_the_menu_that_was_asked_for() {
    let mut harness = harness_in(&sample_folder());

    let view = did(&mut harness, "action list --menu view");
    let actions = view["actions"].as_array().expect("the entries");
    assert!(!actions.is_empty(), "the View menu is not empty");
    assert!(
        actions.iter().all(|entry| entry["menu"] == serde_json::json!("View")),
        "only the menu that was asked for: {:?}",
        actions
            .iter()
            .map(|entry| entry["menu"].as_str().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Several at once, and a submenu names its own rows.
    let asked = did(&mut harness, "action list --menu view,edit");
    let menus: Vec<&str> = asked["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["menu"].as_str().unwrap_or_default())
        .collect();
    assert!(menus.contains(&"View"));
    assert!(menus.contains(&"Edit"));
    assert!(menus.iter().all(|menu| *menu == "View" || *menu == "Edit"));

    // No menu named is still every menu.
    let all = did(&mut harness, "action list");
    assert!(all["actions"].as_array().unwrap().len() > actions.len());

    // A menu that is not a menu is a question that was not asked.
    assert_eq!(refused(&mut harness, "action list --menu purple"), "usage");
}

/// `settings list` keeps the value and the help apart whatever the value's length, so a path in
/// `debug.lldb` does not run into the sentence beside it.
#[test]
fn settings_list_keeps_a_long_value_and_its_help_apart() {
    let mut harness = harness_in(&sample_folder());
    let long = "C:\\jason\\AppData\\Local\\Unluminous\\adapters\\codelldb\\extension\\adapter\\codelldb.exe";
    did(&mut harness, &format!("settings set debug.lldb \"{long}\""));

    let result = did(&mut harness, "settings list");
    let rows: Vec<String> = result["lines"]
        .as_array()
        .expect("the rows")
        .iter()
        .map(|row| row.as_str().expect("a row").to_owned())
        .collect();
    let row =
        rows.iter().find(|row| row.starts_with("debug.lldb")).expect("the row for debug.lldb");
    assert!(
        row.contains(&format!("{long}  Where")),
        "the value and the help are two columns apart: {row}"
    );
    // Every row has the same seam: the help starts two spaces after the value, never on top of it.
    for row in &rows {
        // A contributed pane's three keys are dynamic — a pane is named `<plugin id>/<pane id>` and
        // which plugins are installed is not known until the manifests are read, so they cannot be
        // rows in `SETTINGS` and their help cannot be in the list above. `task-1794` added them
        // because Unluminous writes those keys into its own settings file and would not name them back.
        // Matched by their shape instead, so the seam is still asserted for every row there is.
        let help = match SETTINGS_HELP.iter().find(|help| row.ends_with(*help)) {
            Some(help) => (*help).to_owned(),
            None => {
                assert!(
                    row.starts_with("panes."),
                    "every row ends with its help, or is a contributed pane's: {row}"
                );
                let at = row.find("  How ").unwrap_or_else(|| {
                    panic!("a contributed pane's row should carry its own help: {row}")
                });
                row[at + 2..].to_owned()
            }
        };
        assert!(
            row.contains(&format!("  {help}")),
            "the help is set off from the value by two spaces: {row}"
        );
    }
}

/// The help of every setting, so the test can find the seam in a row without a second copy of the
/// settings list.
const SETTINGS_HELP: &[&str] = &[
    "The family the editor sets text in.",
    "The point size the editor sets text in, in every tab.",
    "How opaque the window is. Below 1 the desktop shows through.",
    "What every colour in the window is. A theme that names the nine token colours also colours code, in every language at once.",
    "One colour for everything the accent means: the caret, the open tab, an open folder.",
    "Which drawn marks the rail buttons and the explorer's folder arrow use.",
    "The family the window's own text is set in: the menus, the rail and the status bar.",
    "The point size the window's own text is set in. The editing area keeps its own.",
    "The point size the terminal sets its grid in.",
    "What each terminal tab runs. Empty means PowerShell on Windows and $SHELL elsewhere.",
    "Whether the editing area has a column of line numbers.",
    "Whether the completion popup arrives as you type. Ctrl+Space works either way.",
    "What line breaks a file is written back with. `keep` writes it the way it was read, which is what leaves a one character edit as a one line diff. A new file gets the platform's own either way.",
    "Patterns Go to File, Find in Files, completion, Go to Definition and Find References leave out, beside the project's own .gitignore, which is read already. The explorer goes on showing everything.",
    "Whether Unluminous asks the releases page for a newer version when it opens. Off, and it asks nothing until somebody presses Check for Updates or runs `update check`. It never installs anything either way.",
    "Whether resting the pointer on a name while the program is stopped shows its value. Show Value on the Debug menu works either way.",
    "Whether a plugin that asked for it draws depth: the soft shadows, gradients and pressed edges behind its own pane. Off, it draws flat.",
    "Whether this Unluminous serves MCP over HTTP. An agent that launches the server itself needs neither this nor a port.",
    "The port it serves on when it does.",
    "One tool an area, or one tool a command. `mcp tools --count` says what each costs.",
    "Which areas of the catalogue the MCP server offers, so an agent is not handed the whole of it. Empty means all of them. `mcp tools --count --areas editor,git` says what a choice costs; the whole catalogue is about 18 per cent of a 96k context window before a question is asked.",
    "Where the LLDB adapter lives, for Rust and native code. Empty means Unluminous looks for codelldb then lldb-dap on PATH. `tools/get-debug-adapter.ps1` fetches one and prints the line.",
    "Where js-debug lives, for JavaScript and TypeScript. There is no default: js-debug is a script rather than a program, so Unluminous has nothing to look for until it is told.",
    "How wide the file explorer is.",
    "How tall the terminal tile is.",
    "How much of the side by side view the source takes.",
    "How much of Find in Files the results take.",
];

#[test]
fn a_relative_path_is_relative_to_the_project_and_the_reply_says_which_path_it_used() {
    // The one rule about paths, and the reason it is safe: every reply reports the absolute path, so
    // a caller is never guessing about where a file came from or went.
    let mut harness = harness_in(&sample_folder());
    let result = did(&mut harness, "tab open notes.txt");
    let used = result["path"].as_str().expect("a path");
    assert!(std::path::Path::new(used).is_absolute(), "{used} should be absolute");
    assert!(used.starts_with(&sample_folder().to_string_lossy().to_string()));
}

#[test]
fn a_command_line_that_will_not_parse_is_refused_before_anything_happens() {
    let mut harness = harness_in(&sample_folder());
    let before = harness.state().files.active().name();
    assert_eq!(refused(&mut harness, "tab opne readme.md"), "usage");
    assert_eq!(refused(&mut harness, "tab open readme.md --purple"), "usage");
    assert_eq!(refused(&mut harness, "tab open"), "usage");
    assert_eq!(harness.state().files.active().name(), before, "and nothing was opened");
}

/// Send a request the way something other than `unluminous-cli` sends one: a JSON object down the
/// channel, read by `Request::from_json`.
///
/// The command line is not the only caller and the tests had only been driving it. An agent through
/// the MCP server writes the `arguments` object itself, from the usage lines, which is where the
/// two spellings of a name come from.
fn over_the_wire(
    harness: &mut Harness<'static, UnluminousApp>,
    command: &str,
    arguments: serde_json::Value,
) -> unluminous_cli::protocol::Reply {
    let ctx = harness.ctx.clone();
    let request = unluminous_cli::protocol::Request::from_json(&serde_json::json!({
        "token": "",
        "command": command,
        "arguments": arguments,
    }))
    .expect("a request that parses");
    let reply = harness
        .state_mut()
        .run_cli_for_test(&request, &ctx)
        .unwrap_or_else(|| panic!("`{command}` was not answered on the frame it was asked"));
    harness.run();
    reply
}

/// A flag spelled the way the usage line spells it takes effect, and a name no command has is
/// refused rather than dropped.
///
/// Both halves were real faults found by driving Unluminous through the MCP tools. `tab open` with
/// `--permanent` left the tab transient, so the next file opened replaced it; `run output` with
/// `--tail` returned the whole screen. Neither said anything, which is what made them expensive —
/// the reply was a success either way.
#[test]
fn a_value_named_the_way_the_usage_line_spells_it_takes_effect() {
    let mut harness = harness_in(&sample_folder());
    let readme = sample_folder().join("readme.md");
    let notes = sample_folder().join("notes.txt");

    let reply = over_the_wire(
        &mut harness,
        "tab.open",
        serde_json::json!({ "path": readme.to_string_lossy(), "--permanent": true }),
    );
    assert!(reply.ok, "{}", reply.message);
    let opened = over_the_wire(&mut harness, "tab.list", serde_json::json!({}));
    let tabs = opened.result["tabs"].as_array().expect("the tabs").clone();
    let readme_tab = tabs.iter().find(|tab| tab["name"] == "readme.md").expect("readme.md is open");
    assert_eq!(
        readme_tab["transient"], false,
        "--permanent is the same flag as permanent, so the tab is kept"
    );

    // And the proof that it means something: a transient tab is the one the next file replaces.
    over_the_wire(&mut harness, "tab.open", serde_json::json!({ "path": notes.to_string_lossy() }));
    let after = over_the_wire(&mut harness, "tab.list", serde_json::json!({}));
    let names: Vec<String> = after.result["tabs"]
        .as_array()
        .expect("the tabs")
        .iter()
        .map(|tab| tab["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(names.contains(&"readme.md".to_owned()), "the permanent tab survived: {names:?}");
}

#[test]
fn a_value_no_command_has_a_name_for_is_refused_rather_than_dropped() {
    let mut harness = harness_in(&sample_folder());
    let readme = sample_folder().join("readme.md");
    let reply = over_the_wire(
        &mut harness,
        "tab.open",
        serde_json::json!({ "path": readme.to_string_lossy(), "permanant": true }),
    );
    assert!(!reply.ok, "a misspelled name is not a success");
    let failure = reply.error.expect("a refusal carries an error");
    assert_eq!(failure.code, "usage");
    assert!(failure.message.contains("permanant"), "{}", failure.message);
    assert!(
        failure.message.contains("permanent"),
        "the refusal says what the command does take: {}",
        failure.message
    );
}

/// The other half of the same fault, and the worse one: a value that was dropped rather than read
/// returned *more* than was asked for and called it a success. `editor text` is the same shape as the
/// `run output --tail` that found it, and needs no process to prove it.
#[test]
fn a_range_is_read_however_the_names_are_spelled() {
    let mut harness = harness(&format!("{}\n{}\n{}\n{}\n", "one", "two", "three", "four"));
    let dashed = over_the_wire(
        &mut harness,
        "editor.text",
        serde_json::json!({ "--from-line": 2, "--to-line": 3 }),
    );
    let plain = over_the_wire(
        &mut harness,
        "editor.text",
        serde_json::json!({ "from-line": 2, "to-line": 3 }),
    );
    assert_eq!(dashed.result["fromLine"], 2, "the range was read: {}", dashed.result);
    assert_eq!(dashed.result["toLine"], 3);
    assert_eq!(
        dashed.result["text"], plain.result["text"],
        "--from-line 2 and from-line 2 are one request"
    );
    let text = dashed.result["text"].as_str().expect("the text");
    assert!(text.contains("two") && text.contains("three"), "{text:?}");
    assert!(!text.contains("one"), "the whole file is not what was asked for: {text:?}");
}

/// A file changed by something other than Unluminous is read again before it is answered about.
///
/// Found by driving Unluminous through the MCP tools while editing the same files from outside: the tab
/// went on showing the version it had read, `editor text` answered with it, and the explorer did not
/// list a file that was plainly on the disk. Both are the same fault — the window is the only writer
/// it knows about — and both are fixed by the rule the symbol index already follows for a closed
/// file: the disk-owned side is re-checked at the moment of use.
#[test]
fn a_file_changed_outside_unluminous_is_read_again_before_it_is_answered_about() {
    let folder = std::env::temp_dir().join("unluminous-changed-underneath");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the project");
    let path = folder.join("notes.md");
    std::fs::write(&path, "first\n").expect("write notes.md");

    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&path).expect("the file opens");
    harness.run();
    let first = over_the_wire(&mut harness, "editor.text", serde_json::json!({}));
    assert_eq!(first.result["text"], "first\n");

    // Something else rewrites it. Nothing tells Unluminous.
    std::fs::write(&path, "second\n").expect("rewrite notes.md");
    let second = over_the_wire(&mut harness, "editor.text", serde_json::json!({}));
    assert_eq!(
        second.result["text"], "second\n",
        "the read is answered from the file rather than from what the tab last held"
    );

    // A file that appears is listed, which is the same fault seen through the explorer.
    let another = folder.join("appeared.md");
    std::fs::write(&another, "new\n").expect("write appeared.md");
    let listed = over_the_wire(&mut harness, "explorer.files", serde_json::json!({}));
    let files: Vec<String> = listed.result["files"]
        .as_array()
        .expect("the files")
        .iter()
        .map(|path| path.as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        files.iter().any(|listed| listed.ends_with("appeared.md")),
        "a file written a moment ago is in the project: {files:#?}"
    );
    std::fs::remove_dir_all(&folder).ok();
}

/// Unsaved changes are never thrown away by the re-read. They are the person's, and there is no undo
/// for losing them — `tab reload --discard` is how somebody says they mean it.
#[test]
fn a_tab_with_unsaved_changes_is_not_reread_from_the_file() {
    let folder = std::env::temp_dir().join("unluminous-changed-underneath-dirty");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the project");
    let path = folder.join("notes.md");
    std::fs::write(&path, "first\n").expect("write notes.md");

    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&path).expect("the file opens");
    harness.run();
    over_the_wire(&mut harness, "editor.insert", serde_json::json!({ "text": "mine " }));
    assert!(harness.state().document().is_modified(), "the tab has unsaved changes");

    std::fs::write(&path, "second\n").expect("rewrite notes.md");
    let read = over_the_wire(&mut harness, "editor.text", serde_json::json!({}));
    let text = read.result["text"].as_str().expect("the text");
    assert!(text.contains("mine "), "the unsaved change is still there: {text:?}");
    assert!(!text.contains("second"), "and the file did not overwrite it: {text:?}");
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn every_command_in_the_catalogue_is_one_the_window_knows() {
    // The catalogue is shared, so the client will accept every command in it. This is the other half:
    // the window must not answer any of them with "there is no such command". Each is run with no
    // arguments at all, so most are refused — what is being checked is *how*.
    let mut harness = harness_in(&sample_folder());
    for command in unluminous_cli::catalogue::COMMANDS {
        if command.local {
            continue; // answered by the client; the window never sees it
        }
        // The ones that would take the window away from under the rest of the test.
        if matches!(command.wire().as_str(), "quit" | "explorer.reveal" | "window.screenshot") {
            continue;
        }
        let ctx = harness.ctx.clone();
        let request =
            unluminous_cli::protocol::Request::new("", &command.wire(), Default::default());
        let reply = match harness.state_mut().run_cli_for_test(&request, &ctx) {
            Some(reply) => reply,
            None => continue, // answered on a later frame, which is an answer
        };
        if let Some(failure) = reply.error {
            assert_ne!(
                failure.code,
                "unknown-command",
                "the window does not know `{}`, which the catalogue offers",
                command.typed()
            );
        }
        harness.run();
    }
}

/// `task-1756`: the agent command and explorer action both create ordinary rendered tabs.
#[test]
fn browser_tabs_open_through_the_shared_cli_and_action_paths() {
    let folder =
        std::env::temp_dir().join(format!("unluminous-browser-paths-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("make the browser project");
    let first = folder.join("index.html");
    let second = folder.join("other.htm");
    std::fs::write(&first, "<title>First</title>").expect("write first page");
    std::fs::write(&second, "<title>Second</title>").expect("write second page");
    let mut harness = harness_in(&folder);
    let opened =
        over_the_wire(&mut harness, "browser.open", serde_json::json!({ "address": "index.html" }));
    assert!(opened.ok, "the command opens local HTML: {:?}", opened.error);
    let first = first.canonicalize().expect("canonical first page");
    assert_eq!(
        harness.state().files.active().browser.as_ref().and_then(|tab| tab.location.source_path()),
        Some(first.as_path())
    );
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::OpenInBrowser(second.clone()), &ctx);
    let second = second.canonicalize().expect("canonical second page");
    assert_eq!(
        harness.state().files.active().browser.as_ref().and_then(|tab| tab.location.source_path()),
        Some(second.as_path())
    );
    let status = over_the_wire(&mut harness, "browser.status", serde_json::json!({}));
    assert_eq!(status.result["url"].as_str().map(|url| url.ends_with("/other.htm")), Some(true));

    // `File -> Open Web Address...` asks, and what is typed lands in a rendered tab through the same
    // path the command line and the explorer entry use.
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::OpenWebAddress, &ctx);
    let mut prompt = harness.state().prompt.clone().expect("the Open Web Address prompt");
    assert_eq!(prompt.title, "Open Web Address");
    prompt.value = "example.com/typed".to_owned();
    harness.state_mut().run_prompt_for_test(prompt);
    harness.state_mut().prompt = None;
    assert_eq!(
        harness.state().files.active().browser.as_ref().map(|tab| tab.current_url().to_owned()),
        Some("https://example.com/typed".to_owned()),
        "an address with no scheme typed into the prompt is read as an address"
    );
    let closed = over_the_wire(&mut harness, "tab.close", serde_json::json!({}));
    assert!(closed.ok, "the typed tab closes again: {:?}", closed.error);

    // The document commands refuse rather than answering about the empty document a rendered tab
    // holds behind its native view, and they say which command does answer.
    for (command, arguments) in [
        ("editor.text", serde_json::json!({})),
        ("editor.insert", serde_json::json!({ "text": "typed" })),
        ("editor.scroll", serde_json::json!({})),
        ("tab.save", serde_json::json!({})),
        ("tab.reload", serde_json::json!({})),
    ] {
        let reply = over_the_wire(&mut harness, command, arguments);
        assert!(!reply.ok, "{command} answers about a web page");
        assert!(
            reply.error.as_ref().is_some_and(|problem| problem.message.contains("browser")),
            "{command}: {:?}",
            reply.error
        );
    }
    let editor = over_the_wire(&mut harness, "editor.status", serde_json::json!({}));
    assert_eq!(editor.result["browser"], serde_json::json!(true));
    let tabs = over_the_wire(&mut harness, "tab.list", serde_json::json!({}));
    assert_eq!(
        tabs.result["tabs"].as_array().map(|tabs| tabs
            .iter()
            .filter(|tab| tab["browser"] == serde_json::json!(true))
            .count()),
        Some(2)
    );

    // One window renders one page at a time, so `browser back` on a tab the view is not pointed at
    // is refused rather than driving somebody else's page.
    let elsewhere = over_the_wire(&mut harness, "browser.status", serde_json::json!({}));
    assert_eq!(
        elsewhere.result["showing"],
        serde_json::json!(false),
        "no native view exists in a test window"
    );

    // The toolbar reaches the same host the command line does: with no native view behind it in a
    // test window, `Reload` comes back as the host's own refusal rather than doing nothing.
    harness.run();
    harness.state_mut().message = None;
    harness.get_by_label("Reload").click();
    harness.run();
    assert!(harness.state().message.is_some(), "the toolbar button reached the browser host");

    // Closing a rendered tab is an ordinary tab close: it neither asks to save nor leaves its root
    // registered, and the tab that is showing goes back to being a document.
    let closed = over_the_wire(&mut harness, "tab.close", serde_json::json!({}));
    assert!(closed.ok, "a rendered tab closes: {:?}", closed.error);
    assert!(
        !harness.state().files.active().is_browser()
            || harness.state().files.active().browser.as_ref().map(|tab| tab.id) != Some(2)
    );
}

// -------------------------------------------------------------------------------------- task-1922
//
// WP4's command line half: the two commands that have no editing `Command` behind them, and the
// three settings. Everything else WP4 added is driven from `editor_formatting.rs` and
// `navigation.rs`, where the text it changes can be read back.

#[test]
fn a_closed_tab_is_reopened_where_it_was_and_the_list_is_walked_backwards() {
    let folder = fixture(
        "unluminous-1922-reopen",
        &[("one.rs", "fn one() {}\n"), ("two.rs", "fn two() {}\n")],
    );
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open one.rs --permanent");
    did(&mut harness, "tab open two.rs --permanent");

    // Nothing closed yet, so there is nothing to reopen and the menu row is dimmed rather than
    // absent — it is `Undo`'s shape, not the absent control rule.
    let mut fresh = harness_in(&folder);
    assert_eq!(refused(&mut fresh, "tab reopen"), "not-applicable");

    did(&mut harness, "tab close two.rs");
    did(&mut harness, "tab close one.rs");
    assert!(harness.state().files.index_of(&folder.join("one.rs")).is_none());

    let back = did(&mut harness, "tab reopen");
    assert!(back["path"].as_str().unwrap().ends_with("one.rs"), "the newest first: {back}");
    assert_eq!(back["left"], 1);
    assert!(harness.state().files.index_of(&folder.join("one.rs")).is_some());

    // Run again and it reaches the one before that, which is what "the ten most recent" buys.
    let older = did(&mut harness, "tab reopen");
    assert!(older["path"].as_str().unwrap().ends_with("two.rs"), "{older}");
    assert_eq!(refused(&mut harness, "tab reopen"), "not-applicable");
}

#[test]
fn reopening_a_closed_tab_from_the_menu_reaches_the_same_file() {
    let folder = fixture("unluminous-1922-reopen-menu", &[("one.rs", "fn one() {}\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open one.rs --permanent");
    did(&mut harness, "tab close");
    choose(&mut harness, Action::ReopenClosedTab);
    assert!(harness.state().files.index_of(&folder.join("one.rs")).is_some());
}

#[test]
fn action_find_ranks_the_menu_entries_the_way_the_palette_does() {
    let mut harness = harness_in(&sample_folder());
    let found = did(&mut harness, "action find line numbers");
    let first = found["actions"][0]["name"].as_str().unwrap_or_default().to_owned();
    assert_eq!(first, "toggle-line-numbers");
    // The menu counts as well as the wording, so a command can be found by where it lives.
    let git = did(&mut harness, "action find git commit");
    let names: Vec<String> = git["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(names.iter().any(|name| name == "git-commit"), "{names:?}");
    // With nothing to look for it is `action list` cut to a readable number, in menu order.
    let all = did(&mut harness, "action find");
    assert_eq!(all["actions"].as_array().unwrap().len(), 20);
    let every = did(&mut harness, "action find --limit 0");
    let listed = did(&mut harness, "action list");
    assert_eq!(
        every["actions"].as_array().unwrap().len(),
        listed["actions"].as_array().unwrap().len(),
        "`--limit 0` is every entry `action list` has"
    );
}

#[test]
fn the_three_editing_settings_are_read_written_and_refused_by_name() {
    let mut harness = harness_in(&sample_folder());
    // What a fresh Unluminous has, which is what it did before any of them existed.
    assert_eq!(did(&mut harness, "settings get editor.indent")["value"], "tabs");
    assert_eq!(did(&mut harness, "settings get editor.auto_indent")["value"], "true");
    assert_eq!(did(&mut harness, "settings get editor.trim")["value"], "false");

    did(&mut harness, "settings set editor.indent spaces:2");
    assert_eq!(did(&mut harness, "settings get editor.indent")["value"], "spaces:2");
    did(&mut harness, "settings set editor.trim true");
    assert!(harness.state().settings.trim_on_save);
    did(&mut harness, "settings set editor.auto_indent false");
    assert!(!harness.state().settings.auto_indent);

    // A word this version has not got is refused with what it does take, which is the rule every
    // other named setting keeps.
    let reply = run(&mut harness, "settings set editor.indent four");
    assert!(!reply.ok);
    assert!(reply.message.contains("spaces:N"), "{}", reply.message);
    assert_eq!(refused(&mut harness, "settings set editor.auto_indent perhaps"), "usage");

    // And all three are in `settings list`, which is what an agent reads to find out they exist.
    let listed = did(&mut harness, "settings list");
    for name in ["editor.indent", "editor.auto_indent", "editor.trim"] {
        assert!(listed.get(name).is_some(), "{name} should be in `settings list`");
    }
}

#[test]
fn the_palette_is_a_modal_the_command_line_can_drive_like_go_to_file() {
    // `modal` is how an agent drives a dialog, so a dialog it has never heard of is a dialog an
    // agent can only look at. `action find` plus `action run` reaches the same command; this is the
    // window's own box being typed into, which is what a screenshot is taken of.
    let mut harness = harness_in(&sample_folder());
    let opened = did(&mut harness, "modal open command-palette");
    assert_eq!(opened["open"], "command-palette");
    assert_eq!(did(&mut harness, "modal state")["open"], "command-palette");

    did(&mut harness, "modal type line numbers");
    let rows = did(&mut harness, "modal results");
    assert_eq!(rows["results"][0]["name"], "toggle-line-numbers");

    let was = harness.state().settings.line_numbers;
    did(&mut harness, "modal accept --index 0");
    assert_eq!(harness.state().settings.line_numbers, !was);
    assert_eq!(
        did(&mut harness, "modal state")["open"],
        serde_json::Value::Null,
        "running a row shuts it"
    );

    // A row that cannot be used is refused with the reason and the palette stays open, which is
    // what the palette itself does: somebody looking for Redo wants to be told there is nothing to
    // redo, not told there is no such command.
    did(&mut harness, "modal open command-palette --query redo");
    assert_eq!(refused(&mut harness, "modal accept --index 0"), "not-applicable");
    assert_eq!(did(&mut harness, "modal state")["open"], "command-palette");
    did(&mut harness, "modal cancel");
}
