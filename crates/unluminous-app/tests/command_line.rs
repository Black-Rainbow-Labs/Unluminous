//! The command line, driven through a real window.
//!
//! Most of these go through the whole of the command line path apart from the socket: the words are
//! parsed against `unluminous_cli::catalogue`, dispatched by `UnluminousApp::run_cli`, and what is
//! checked afterwards is the window's own state — not the reply's opinion of itself. A command that
//! says it opened a file and a window with that file open are two different claims, and only the
//! second one is worth testing.
//!
//! **Two of them do include the socket**, and they are the only tests anywhere that run the real
//! `unluminous-cli` program against a real window: one runs `status --section keyboard` down the
//! channel, and one spawns `mcp serve` and calls a tool through it. They are at the end of the file.
//!
//! **And one of them is a rule rather than a test of anything in particular.**
//! `every_catalogue_command_is_driven_both_ways` drives every command in the catalogue to a success
//! and to a refusal and then walks `catalogue::COMMANDS`, failing and naming any command nothing
//! drove. That is `unluminous-cli/src/documentation.rs`'s own mechanism applied to the window: a
//! command that exists and is not tested is a failing test. Nothing runs this suite on a push, so it
//! is the only thing that will notice a command being added untested.
//!
//! **Not one of the 46 tests here takes a picture.** What a command does to the rendering is
//! covered by the files beside this one, which set the same states up by hand; what is unproven, and
//! what these prove, is that the command line reaches those states at all.

mod common;

use common::*;

use std::sync::OnceLock;

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
    steady(harness);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    steady(harness);
    for at in path {
        harness.input_mut().events.push(egui::Event::PointerMoved(*at));
        steady(harness);
    }
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: *path.last().unwrap_or(&from),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    steady(harness);
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
    steady(&mut harness);
    // The file is coloured on the frame after it is opened, so let that happen before anything is
    // measured; otherwise the first drag frame would be charged with work the opening owed.
    steady(&mut harness);

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
    steady(&mut harness);

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
    steady(&mut harness);
    steady(&mut harness);

    let before: Vec<f32> = harness.state().layout().lines.iter().map(|line| line.y).collect();
    let count = before.len();
    assert!(count > 2000, "the fixture is meant to be long: {count} lines");

    // Into the middle of the file, so there is plenty above it and plenty below it.
    let middle = harness.state().document().text().len_bytes() / 2;
    harness.state_mut().command(Command::PlaceCaret { offset: middle, extend: false });
    steady(&mut harness);
    harness.state_mut().command(Command::Insert("X".to_owned()));
    steady(&mut harness);

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
    steady(&mut harness);
    assert!(harness.state().terminal.visible);
    did(&mut harness, "terminal hide");
    assert!(!harness.state().terminal.visible);
    did(&mut harness, "terminal height 400");
    assert_eq!(harness.state().panes.terminal_height, 400.0);
    let listed = did(&mut harness, "terminal list");
    assert_eq!(listed["count"], serde_json::json!(1));
    // **One folder an entry, whatever each answers**, so the list can be indexed by tab number -- `task-1950`.
    // A detached tab has no shell to be asked about and answers with an empty string, which is also what a
    // platform that will not say answers with. That a real shell answers with the folder somebody moved it to
    // is `services::shell_integration`'s own live test, against a real PowerShell.
    assert_eq!(listed["folders"].as_array().expect("a folder for every tab").len(), 1);
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
    steady(&mut harness);
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
    steady(&mut harness);
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
    steady(&mut harness);
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

/// `task-1945`: the window says whether the **operating system** is sending it the keys.
///
/// `status --section keyboard` says which surface inside the window Unluminous gave the keys to, and
/// that is a different question from whether the window is being sent any. A browser node's page is a
/// native child window; while it holds the focus every key press goes to the page, the title bar
/// cannot move the window because `egui-winit` drops `StartDrag`, and a resize is a request the window
/// manager throws away. Until this field there was no way to ask that from outside the window.
#[test]
fn the_window_says_whether_the_operating_system_is_sending_it_the_keys() {
    let mut harness = harness_in(&sample_folder());
    let window = did(&mut harness, "status --section window");
    assert!(window.get("focused").is_some(), "the window section carries the focus");
    assert!(window.get("maximised").is_some(), "and whether the window is maximised");
    // **And what the operating system itself says, which is a different question.** `focused` above
    // is `winit`'s own cache of two window messages, and `task-2009` is a window that was the
    // foreground window with the keyboard while that cache said it had no focus — with no symptom at
    // all except that the title bar would not move the window. Both are `null` here, because a test
    // window has no operating system window behind it to ask about.
    for asked in ["osForeground", "osKeyboard"] {
        assert!(window.get(asked).is_some(), "the window section carries {asked}");
        assert_eq!(
            window[asked],
            serde_json::Value::Null,
            "{asked} has no answer in a test window"
        );
    }

    let ids: Vec<egui::ViewportId> = harness.input().viewports.keys().copied().collect();
    for id in ids {
        if let Some(viewport) = harness.input_mut().viewports.get_mut(&id) {
            viewport.focused = Some(false);
        }
    }
    steady(&mut harness);
    let window = did(&mut harness, "status --section window");
    assert_eq!(
        window["focused"],
        serde_json::json!(false),
        "and it says so when something else has taken the keyboard"
    );
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
    // `task-1922` WP4's three, which are the reason this list is a list rather than a rule: a help
    // string added to `cli_settings::SETTINGS` and not added here fails this test, which is what it
    // is for.
    "What one indent is made of, which is what the Tab key types where nothing is selected. Tabs, which is what it has always typed. Indenting a selection is still one character a line, because unluminous-core's indent unit is a character.",
    "Whether a new line starts with the indentation of the line it was started from.",
    "Whether the trailing whitespace goes off every line when a file is written. Off. It never runs on a Markdown file, where two trailing spaces are a line break.",
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
    "Whether PowerShell is asked to report the folder it is in, so a tab reopens where you were rather than where it started. Off. PowerShell's Set-Location never moves the process's own current directory, so there is no other way to read it; turning this on adds one line to the prompt, after your own profile has set it up. A shell that already reports its folder is followed whatever this says.",
    "Whether the completion popup arrives as you type. Ctrl+Space works either way.",
    "What line breaks a file is written back with. `keep` writes it the way it was read, which is what leaves a one character edit as a one line diff. A new file gets the platform's own either way.",
    "Patterns Go to File, Find in Files, completion, Go to Definition and Find References leave out, beside the project's own .gitignore, which is read already. The explorer goes on showing everything.",
    "Whether Unluminous asks unluminous.com for a newer version when it opens, falling back to the public GitHub releases when the site does not answer. Off, and it asks nothing until somebody presses Check for Updates or runs `update check`. It never installs anything either way.",
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
    steady(harness);
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
    steady(&mut harness);
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
    steady(&mut harness);
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
        steady(&mut harness);
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
    // **Plain, not verbatim.** `BrowserLocation::local` canonicalises and then takes the prefix
    // Windows puts on a canonical path off again, because a rendered tab's file is compared against
    // the explorer's own rows and handed to other programs — `task-2009`.
    let first =
        unluminous_terminal::paths::plain(&first.canonicalize().expect("canonical first page"));
    assert_eq!(
        harness.state().files.active().browser.as_ref().and_then(|tab| tab.location.source_path()),
        Some(first.as_path())
    );
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::OpenInBrowser(second.clone()), &ctx);
    let second =
        unluminous_terminal::paths::plain(&second.canonicalize().expect("canonical second page"));
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
    steady(&mut harness);
    harness.state_mut().message = None;
    harness.get_by_label("Reload").click();
    steady(&mut harness);
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
    did(&mut harness, "modal accept 0");
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
    assert_eq!(refused(&mut harness, "modal accept 0"), "not-applicable");
    assert_eq!(did(&mut harness, "modal state")["open"], "command-palette");
    did(&mut harness, "modal cancel");
}

// =================================================================================================
// A value of a kind the command cannot use.
//
// `task-1922` B13. `Request::number` answers `None` both when a key is absent and when its value
// will not parse, so six commands took a word where they take a number, did nothing about it, and
// reported `ok`. Each of the six is here, with the same command given a real number beside it --
// because a refusal that also refused the working form would be a worse fault than the one it
// closes. The walk over the whole catalogue is `refuse_a_word_where_a_command_takes_a_number`,
// beside the other rule in the coverage section below.

/// Refuse `line`, insisting it was a usage refusal, and hand back the sentence.
///
/// [`refused`] answers with the code alone, and what is under test here is that the sentence names
/// the key and quotes what arrived: a refusal that said only "usage" would leave a caller exactly
/// where the silent success left them.
fn refusal(harness: &mut Harness<'static, UnluminousApp>, line: &str) -> String {
    let reply = run(harness, line);
    assert!(!reply.ok, "`{line}` should have been refused, and was not");
    assert_eq!(
        reply.error.as_ref().map(|failure| failure.code.as_str()),
        Some("usage"),
        "`{line}` should be a usage refusal: {}",
        reply.message
    );
    reply.message
}

#[test]
fn window_size_refuses_a_width_that_is_not_a_number() {
    let mut harness = harness_in(&sample_folder());
    let said = refusal(&mut harness, "window size --width nonsense");
    assert!(said.contains("width"), "{said}");
    assert!(said.contains("nonsense"), "{said}");
    did(&mut harness, "window size --width 1100 --height 720");
}

#[test]
fn window_position_refuses_an_x_that_is_not_a_number() {
    let mut harness = harness_in(&sample_folder());
    let said = refusal(&mut harness, "window position --x nonsense");
    assert!(said.contains("x"), "{said}");
    assert!(said.contains("nonsense"), "{said}");
    did(&mut harness, "window position --x 40 --y 40");
}

#[test]
fn explorer_width_refuses_a_width_that_is_not_a_number() {
    let mut harness = harness_in(&sample_folder());
    let before = harness.state().panes.explorer_width;
    let said = refusal(&mut harness, "explorer width nonsense");
    assert!(said.contains("points"), "{said}");
    assert!(said.contains("nonsense"), "{said}");
    assert_eq!(harness.state().panes.explorer_width, before, "nothing moved");
    did(&mut harness, "explorer width 400");
    assert_eq!(harness.state().panes.explorer_width, 400.0);
}

#[test]
fn terminal_height_refuses_a_height_that_is_not_a_number() {
    let mut harness = harness_in(&sample_folder());
    let before = harness.state().panes.terminal_height;
    let said = refusal(&mut harness, "terminal height nonsense");
    assert!(said.contains("points"), "{said}");
    assert!(said.contains("nonsense"), "{said}");
    assert_eq!(harness.state().panes.terminal_height, before, "nothing moved");
    did(&mut harness, "terminal height 400");
    assert_eq!(harness.state().panes.terminal_height, 400.0);
}

#[test]
fn editor_caret_refuses_a_line_that_is_not_a_number() {
    let mut harness = harness_in(&sample_folder());
    did(&mut harness, "tab open notes.txt --permanent");
    // The sample files are one line each, so the document is given some lines to aim at first.
    did(&mut harness, "editor set-text alpha\\nbravo\\ncharlie");
    did(&mut harness, "editor caret --line 1 --column 1");
    let said = refusal(&mut harness, "editor caret --line nonsense");
    assert!(said.contains("line"), "{said}");
    assert!(said.contains("nonsense"), "{said}");
    let at = harness.state().caret_position();
    assert_eq!((at.line, at.column), (1, 1), "the caret did not move");
    did(&mut harness, "editor caret --line 2 --column 1");
    let at = harness.state().caret_position();
    assert_eq!((at.line, at.column), (2, 1), "a real number still moves it");
}

#[test]
fn explorer_tree_refuses_a_limit_that_is_not_a_number() {
    let mut harness = harness_in(&sample_folder());
    let said = refusal(&mut harness, "explorer tree --limit nonsense");
    assert!(said.contains("limit"), "{said}");
    assert!(said.contains("nonsense"), "{said}");
    let tree = did(&mut harness, "explorer tree --limit 5");
    assert!(tree["rows"].as_array().unwrap().len() <= 5, "the limit is still honoured");
}

// =================================================================================================
// Dispatch coverage as a rule.
//
// `task-1922` §5.4: this file drives every catalogue command to a success and to a refusal, and a
// walk of `catalogue::COMMANDS` fails naming any command it did not drive. It is the mechanism
// `unluminous-cli/src/documentation.rs` already uses to keep a command from existing without a
// section in the reference, applied to the window: a command that exists and is not tested is a
// failing test.
//
// **It is one test rather than a test a command.** The walk has to read what the drives wrote down,
// and `cargo test` gives every test its own thread and no order, so a walk in a test of its own
// would be asking about drives that may not have happened yet. So the drives are functions this one
// test calls in turn, and `common::commands_driven` is what it reads afterwards -- which every other
// test in this file also writes to, through `did`, `refused` and `drove`, so a command driven
// anywhere in the binary counts.

/// What a drive in the walk is, and what came of it.
///
/// Faults are collected rather than asserted one at a time. There are two hundred and fourteen
/// commands here: a walk that stopped at the first would take two hundred runs to settle, where one
/// that reports all of them takes a handful.
#[derive(Default)]
struct Coverage {
    faults: Vec<String>,
}

impl Coverage {
    /// Drive `line` and expect the window to do it.
    ///
    /// A command that answers on a **later** frame -- a screenshot, a reference search, one of the
    /// waits -- is a command the window accepted, so it counts as a drive: `run_command_line` hands
    /// back nothing for one of those, which is what `None` here means.
    fn works(&mut self, harness: &mut Harness<'static, UnluminousApp>, line: &str) {
        let ctx = harness.ctx.clone();
        match harness.state_mut().run_command_line(line, &ctx) {
            Some(reply) => {
                note_a_drive(&reply.command, reply.ok);
                if !reply.ok {
                    self.faults.push(format!("`{line}` was refused: {}", reply.message));
                }
            }
            None => note_a_driven_line(line, true),
        }
        harness.step();
    }

    /// Drive `line` and expect the window to refuse it, in a way a script can match on.
    fn refuses(&mut self, harness: &mut Harness<'static, UnluminousApp>, line: &str) {
        let ctx = harness.ctx.clone();
        match harness.state_mut().run_command_line(line, &ctx) {
            Some(reply) => {
                note_a_drive(&reply.command, reply.ok);
                if reply.ok {
                    self.faults
                        .push(format!("`{line}` should have been refused: {}", reply.message));
                }
                if reply.command.is_empty() {
                    self.faults.push(format!(
                        "`{line}` was refused before it reached the window, so it says nothing about \
                         the command: {}",
                        reply.message
                    ));
                }
            }
            None => self.faults.push(format!("`{line}` should have been refused, and was held")),
        }
        harness.step();
    }

    /// Drive `line` for its own sake, expecting nothing of it.
    ///
    /// For the handful of lines that are there to put the window into the state the next drive needs,
    /// where whether that one worked is not what is being asked.
    fn sets_up(&mut self, harness: &mut Harness<'static, UnluminousApp>, line: &str) {
        let ctx = harness.ctx.clone();
        if let Some(reply) = harness.state_mut().run_command_line(line, &ctx) {
            note_a_drive(&reply.command, reply.ok);
        }
        harness.step();
    }
}

/// The commands no test can drive to a success, and why each one cannot.
///
/// `task-1922` §5.4 asks for this to be named rather than for anything to be quietly skipped, and
/// the walk fails if a command is on this list **and** was driven anyway, so the list cannot grow
/// stale in the direction that hides something. Nothing is excluded from the **refusal** half: a
/// value a command has no name for is refused before the window dispatches anything, so every
/// command in the catalogue is refusable including the ones below.
const CANNOT_BE_MADE_TO_SUCCEED: &[(&str, &str)] = &[
    // The client answers these without a running Unluminous, so `run_cli` never sees one. They are
    // tested in `unluminous-cli` itself, where they live.
    ("instances", "the client answers it; it reads the instance files and never reaches a window"),
    ("launch", "the client answers it, by starting a second Unluminous process"),
    ("commands", "the client answers it, out of the catalogue it already holds"),
    ("version", "the client answers it, out of its own build information"),
    ("mcp.serve", "the client answers it, by becoming a Model Context Protocol server"),
    ("mcp.install", "the client answers it, by rewriting an agent's own configuration on this machine"),
    ("mcp.config", "the client answers it, out of the catalogue and this machine's paths"),
    ("mcp.tools", "the client answers it, out of the catalogue it already holds"),
    // And the four that do reach the window and must not be allowed to finish.
    ("quit", "it ends the window, and the walk has the rest of the catalogue still to drive"),
    // It is on a thread since `task-1984` L1, so the coverage walk sees a hold rather than a reply
    // -- and it would still ask the real GitHub, because this walk stands up no server.
    // `update_check_does_not_block_the_frame_it_arrives_on` and
    // `update_check_answers_with_what_the_releases_page_said` drive it against one on loopback.
    ("update.check", "it asks the releases page, and this walk stands up no server to answer it"),
    ("debug.install", "it downloads and installs a debug adapter onto this machine"),
    ("explorer.reveal", "it hands the path to the operating system's own file manager"),
    // Three that need something a window with no web view behind it has not got.
    ("browser.back", "it needs a rendered page with somewhere behind it, and a test window renders none"),
    ("browser.forward", "it needs a rendered page with somewhere ahead of it, and a test window renders none"),
    ("browser.reload", "it drives the one native view, which a test window has not got: `Browser::showing` is None, so the tab is never the one being rendered"),
];

/// Every command in the catalogue is driven to a success and to a refusal.
#[test]
fn every_catalogue_command_is_driven_both_ways() {
    let mut coverage = Coverage::default();
    refuse_a_value_no_command_has_a_name_for(&mut coverage);
    refuse_a_word_where_a_command_takes_a_number(&mut coverage);
    drive_the_window_and_the_tabs(&mut coverage);
    drive_the_editing_commands(&mut coverage);
    drive_the_panels_and_the_explorer(&mut coverage);
    drive_the_modals_and_the_settings(&mut coverage);
    drive_the_terminal_and_the_runs(&mut coverage);
    drive_the_canvas(&mut coverage);
    drive_a_paused_debugger(&mut coverage);
    drive_a_repository(&mut coverage);
    drive_a_contributed_tab(&mut coverage);
    drive_the_project(&mut coverage);

    let driven = commands_driven();
    let mut missing = Vec::new();
    for command in unluminous_cli::catalogue::COMMANDS {
        let wire = command.wire();
        let ways = driven.get(&wire).copied().unwrap_or_default();
        let excluded = CANNOT_BE_MADE_TO_SUCCEED.iter().find(|(name, _)| *name == wire);
        match excluded {
            Some((_, reason)) if ways.succeeded => missing.push(format!(
                "`{}` is on the list of what cannot be made to succeed -- {reason} -- and something \
                 drove it to a success anyway. Take it off the list.",
                command.typed()
            )),
            Some(_) => {}
            None if !ways.succeeded => missing.push(format!(
                "`{}` is never driven to a success by any test in this file. Add a drive, or add it \
                 to CANNOT_BE_MADE_TO_SUCCEED with the reason.",
                command.typed()
            )),
            None => {}
        }
        if !ways.refused {
            missing.push(format!(
                "`{}` is never driven to a refusal by any test in this file.",
                command.typed()
            ));
        }
    }

    let faults: Vec<String> = coverage.faults.into_iter().chain(missing).collect();
    assert!(faults.is_empty(), "{} faults:\n  {}", faults.len(), faults.join("\n  "));
}

/// A value a command has no name for is a usage refusal, whatever the command.
///
/// `task-1804`'s rule -- a key the schema offered but this command does not name is refused rather
/// than dropped -- asked of every command at once. It is the refusal half of the coverage above for
/// every one of them, which is why the exclusion list is only about successes: this is decided in
/// `unknown_argument_refusal`, before anything is dispatched, so it is safe to ask even of `quit`.
///
/// The request is built by hand rather than typed as a line, because the client's own parser refuses
/// an unknown flag before it sends anything -- which is right, and is a different rule tested in
/// `a_command_line_that_will_not_parse_is_refused_before_anything_happens`. What is under test here
/// is the window's half, reached the way the MCP server reaches it: an `arguments` object written by
/// something that is not `unluminous-cli`.
fn refuse_a_value_no_command_has_a_name_for(coverage: &mut Coverage) {
    let mut harness = harness_in(&dispatch_folder());
    for command in unluminous_cli::catalogue::COMMANDS {
        let mut arguments = serde_json::Map::new();
        arguments.insert("unluminous-no-such-value".to_owned(), serde_json::json!("x"));
        let request = unluminous_cli::protocol::Request::new("", &command.wire(), arguments);
        let ctx = harness.ctx.clone();
        let Some(reply) = harness.state_mut().run_cli_for_test(&request, &ctx) else {
            coverage.faults.push(format!(
                "`{}` held rather than refusing a value it has no name for",
                command.typed()
            ));
            continue;
        };
        note_a_drive(&reply.command, reply.ok);
        match reply.error {
            Some(failure) if failure.code == "usage" => {}
            Some(failure) => coverage.faults.push(format!(
                "`{}` refused a value it has no name for with `{}` rather than `usage`: {}",
                command.typed(),
                failure.code,
                failure.message
            )),
            None => coverage.faults.push(format!(
                "`{}` took a value it has no name for and called it a success: {}",
                command.typed(),
                reply.message
            )),
        }
        harness.step();
    }
}

/// Every name the catalogue declares a number is refused a word, whatever the command.
///
/// `task-1922` B13's rule asked of the whole catalogue at once, which is why the six above do not
/// have to be joined by a seventh when a name is marked a number later: this finds it. Like its
/// sibling above it needs no exclusions, because `wrong_number_refusal` decides before anything is
/// dispatched, so it is safe to ask even of `quit`.
///
/// The request is built by hand rather than typed as a line, for the same reason
/// `refuse_a_value_no_command_has_a_name_for` builds one: what is under test is the window's half,
/// reached the way the MCP server reaches it.
fn refuse_a_word_where_a_command_takes_a_number(coverage: &mut Coverage) {
    let mut harness = harness_in(&dispatch_folder());
    let mut asked = 0;
    for command in unluminous_cli::catalogue::COMMANDS {
        for name in unluminous_cli::catalogue::value_names(command) {
            match unluminous_cli::catalogue::kind_of(command, name) {
                Some(unluminous_cli::catalogue::Kind::Whole)
                | Some(unluminous_cli::catalogue::Kind::Number) => {}
                _ => continue,
            }
            asked += 1;
            let mut arguments = serde_json::Map::new();
            arguments.insert(name.to_owned(), serde_json::json!("nonsense"));
            let request = unluminous_cli::protocol::Request::new("", &command.wire(), arguments);
            let ctx = harness.ctx.clone();
            let Some(reply) = harness.state_mut().run_cli_for_test(&request, &ctx) else {
                coverage.faults.push(format!(
                    "`{}` held rather than refusing a word for {name}",
                    command.typed()
                ));
                continue;
            };
            note_a_drive(&reply.command, reply.ok);
            match reply.error {
                Some(failure) if failure.code == "usage" => assert!(
                    failure.message.contains(name) && failure.message.contains("nonsense"),
                    "`{}` refused a word for {name} without saying which or what: {}",
                    command.typed(),
                    failure.message
                ),
                Some(failure) => coverage.faults.push(format!(
                    "`{}` refused a word for {name} with `{}` rather than `usage`: {}",
                    command.typed(),
                    failure.code,
                    failure.message
                )),
                None => coverage.faults.push(format!(
                    "`{}` took a word for {name} and called it a success: {}",
                    command.typed(),
                    reply.message
                )),
            }
            harness.step();
        }
    }
    assert!(asked > 100, "only {asked} names are declared numbers, which cannot be right");
}

/// The project the walk drives, small enough that a tree and a file list are quick to read.
///
/// `src/main.rs` has a function called from another, a comment, a block that can be folded and a
/// bracket on line 1 whose partner is on line 5 -- four of the drives below want one of those.
fn dispatch_folder() -> std::path::PathBuf {
    fixture(
        "unluminous-dispatch-coverage",
        &[
            ("readme.md", "# Readme\n\nSome prose with teh word in it.\n"),
            ("notes.txt", "notes one\nnotes two\n"),
            (
                "src/main.rs",
                "fn helper(value: usize) -> usize {\n    // a comment\n    let total = value + 1;\n    total\n}\n\nfn main() {\n    let answer = helper(2);\n    println!(\"{answer}\");\n}\n",
            ),
            ("src/other.rs", "pub fn other() {}\n"),
            ("docs/one.md", "# One\n"),
        ],
    )
}

/// `status`, the window itself, the browser tabs, the file tabs and the panes.
fn drive_the_window_and_the_tabs(coverage: &mut Coverage) {
    let mut harness = harness_in(&dispatch_folder());
    let c = coverage;

    c.works(&mut harness, "status --json");
    c.works(&mut harness, "mcp status --json");
    c.works(&mut harness, "window focus");
    c.works(&mut harness, "window size --width 1100 --height 720");
    c.works(&mut harness, "window position --x 40 --y 40");
    c.works(&mut harness, "window message Ready for the next step");

    // Before anything has been closed, so there is genuinely nothing to reopen.
    c.refuses(&mut harness, "tab reopen");
    c.works(&mut harness, "tab open readme.md");
    c.refuses(&mut harness, "tab open no-such-file.md");
    c.works(&mut harness, "tab list --json");
    c.works(&mut harness, "tab show readme.md");
    c.refuses(&mut harness, "tab show 99");
    c.works(&mut harness, "tab open src/main.rs --permanent");
    c.works(&mut harness, "tab next");
    c.works(&mut harness, "tab previous");
    c.works(&mut harness, "tab move 0");
    c.refuses(&mut harness, "tab move 0 --tab no-such.md");
    c.works(&mut harness, "tab save");
    c.works(&mut harness, "tab reload --discard");
    c.refuses(&mut harness, "tab close no-such-file.md");
    c.works(&mut harness, "tab open notes.txt");
    c.works(&mut harness, "tab close notes.txt");
    c.works(&mut harness, "tab reopen");
    c.works(&mut harness, "tab save-as copy.md");

    c.works(&mut harness, "pane list --json");
    c.works(&mut harness, "pane split");
    c.works(&mut harness, "pane focus 1");
    c.refuses(&mut harness, "pane focus 9");
    c.works(&mut harness, "pane width 0 0.35");
    c.refuses(&mut harness, "pane width 9 0.35");
    c.works(&mut harness, "pane move left");
    c.refuses(&mut harness, "pane move sideways");
    // `pane move left` takes the tab out of the pane it was in, and a pane with nothing in it is
    // not kept -- which is `OpenFiles::tidy`'s own invariant. So the area has to be split again
    // before there is anything for these two to undo.
    c.sets_up(&mut harness, "pane split");
    c.works(&mut harness, "pane unsplit");
    c.refuses(&mut harness, "pane unsplit-all");
    c.sets_up(&mut harness, "pane split");
    c.works(&mut harness, "pane unsplit-all");

    // A browser tab, which is an ordinary tab holding a page rather than a text document. Nothing is
    // fetched: there is no web view behind a test window, so what is under test is the tab.
    c.works(&mut harness, "browser open https://example.com/");
    c.works(&mut harness, "browser status --json");
    c.refuses(&mut harness, "browser open");
    c.sets_up(&mut harness, "tab open readme.md");
    c.refuses(&mut harness, "browser status --json");
    c.refuses(&mut harness, "browser reload");

    // Last, because it is answered on a later frame and leaves the window waiting for one.
    c.works(
        &mut harness,
        &format!("window screenshot {}", dispatch_folder().join("shot.png").display()),
    );
}

/// Everything that reads or changes the document: the caret, the text, the folds and the marks.
fn drive_the_editing_commands(coverage: &mut Coverage) {
    let mut harness = harness_in(&dispatch_folder());
    let c = coverage;

    // Before anything has jumped, so there is genuinely nowhere to go back to.
    c.sets_up(&mut harness, "tab open src/main.rs");
    c.refuses(&mut harness, "editor navigate-back");
    c.refuses(&mut harness, "editor navigate-forward");
    c.works(&mut harness, "editor definition helper --open --json");
    c.works(&mut harness, "editor navigate-back");
    c.works(&mut harness, "editor navigate-forward");
    // **`editor definition` has no refusal of its own**, and that is `task-1675`'s honesty rule
    // rather than an omission: a name nothing defines answers "no definition found", and a name
    // asked about from a file whose own language names no definers is still answered out of the
    // project index. Its refusal is the one every command has, in
    // `refuse_a_value_no_command_has_a_name_for`.

    c.works(&mut harness, "editor status --json");
    c.works(&mut harness, "editor text");
    c.works(&mut harness, "editor caret --line 3 --column 5");
    c.works(&mut harness, "editor select --all");
    c.works(&mut harness, "editor scroll --top");
    c.works(&mut harness, "editor complete --stem he --limit 5 --json");
    c.refuses(&mut harness, "editor complete --choose no_such_candidate_at_all");

    // **The four that read the file as it was written come before the ones that change it.** Line 1
    // column 34 is the brace that closes on line 5, and `total` is a word in the text: a drive that
    // had already commented a line out or duplicated one would be asking about a different file.
    c.works(&mut harness, "editor caret --line 1 --column 34");
    c.works(&mut harness, "editor bracket --json");
    c.works(&mut harness, "editor find total --json");
    c.works(&mut harness, "editor replace total sum --all --json");
    c.works(&mut harness, "editor references helper --json");
    c.works(&mut harness, "editor rename helper2 --name helper --json");

    c.works(&mut harness, "editor insert Hello");
    c.works(&mut harness, "editor undo");
    c.works(&mut harness, "editor redo");
    c.works(&mut harness, "editor indent");
    c.works(&mut harness, "editor dedent");
    c.works(&mut harness, "editor comment --toggle");
    c.works(&mut harness, "editor lines duplicate");
    c.refuses(&mut harness, "editor lines nonsense");
    c.works(&mut harness, "editor trim");
    c.works(&mut harness, "editor set-text hello");
    c.refuses(&mut harness, "editor set-text --from-file no-such-file.md");
    c.works(&mut harness, "editor undo");
    c.sets_up(&mut harness, "tab reload --discard");

    // A source file has no preview, which is the absent control rule reaching the command line.
    c.works(&mut harness, "editor view raw");
    c.refuses(&mut harness, "editor view preview");
    c.refuses(&mut harness, "editor preview --json");
    c.refuses(&mut harness, "editor preview-select --all");
    c.sets_up(&mut harness, "tab open readme.md");
    c.works(&mut harness, "editor view preview");
    c.works(&mut harness, "editor preview --json");
    c.works(&mut harness, "editor preview-select --all");
    c.sets_up(&mut harness, "editor view raw");

    c.sets_up(&mut harness, "tab open src/main.rs");
    c.works(&mut harness, "fold list --json");
    c.works(&mut harness, "fold collapse --all");
    c.works(&mut harness, "fold expand --all");
    c.works(&mut harness, "fold toggle --line 1");
    c.refuses(&mut harness, "fold toggle --line 9999");
    c.refuses(&mut harness, "fold collapse --line 9999");
    c.refuses(&mut harness, "fold expand --line 9999");
    c.refuses(&mut harness, "fold others --selection");
    c.sets_up(&mut harness, "editor select --from-line 3 --to-line 3");
    c.works(&mut harness, "fold others");
    c.sets_up(&mut harness, "fold expand --all");

    c.works(&mut harness, "highlight add --from-line 1 --to-line 2");
    c.refuses(&mut harness, "highlight add --text no_such_text_anywhere");
    c.works(&mut harness, "highlight list --json");
    c.works(&mut harness, "highlight clear --all");
    c.works(&mut harness, "highlight apply --json-text []");
    c.refuses(&mut harness, "highlight apply --from-file no-such.json");

    // The gestures, which hold until their frames have been drawn, so each goes through `drove`.
    for line in [
        "input move 300 220",
        "input click 300 220",
        "input key Escape",
        "input text hi",
        "input wheel -3",
        "input drag 300 220 --to-x 360 --to-y 260",
    ] {
        drove(&mut harness, line);
    }
    c.refuses(&mut harness, "input key NoSuchKeyName");
    c.refuses(&mut harness, "input wheel nonsense");
    c.refuses(&mut harness, "input click nonsense 220");
    c.refuses(&mut harness, "input move nonsense 220");
    c.refuses(&mut harness, "input drag nonsense 220");
    c.refuses(&mut harness, "input text");
}

/// The panels, the explorer and the actions.
fn drive_the_panels_and_the_explorer(coverage: &mut Coverage) {
    let folder = copy_out_of_the_repository(&dispatch_folder(), "unluminous-dispatch-explorer");
    let mut harness = harness_in(&folder);
    let c = coverage;

    c.works(&mut harness, "panel list --json");
    c.works(&mut harness, "panel dock terminal right");
    c.refuses(&mut harness, "panel dock nosuchpanel right");
    c.refuses(&mut harness, "panel dock terminal sideways");
    c.works(&mut harness, "panel size terminal --height 320");
    c.refuses(&mut harness, "panel size nosuchpanel --height 320");
    c.works(&mut harness, "panel zoom explorer 1.35");
    c.refuses(&mut harness, "panel zoom nosuchpanel 1.35");
    c.works(&mut harness, "panel reset");

    c.works(&mut harness, "explorer show");
    c.works(&mut harness, "explorer hide");
    c.works(&mut harness, "explorer toggle");
    c.works(&mut harness, "explorer width 320");
    c.works(&mut harness, "explorer filter one");
    c.sets_up(&mut harness, "explorer filter");
    c.works(&mut harness, "explorer expand src");
    c.refuses(&mut harness, "explorer expand no-such-folder");
    c.works(&mut harness, "explorer collapse src");
    c.refuses(&mut harness, "explorer collapse no-such-folder");
    c.works(&mut harness, "explorer tree --json");
    c.works(&mut harness, "explorer files --limit 20 --json");
    c.works(&mut harness, "explorer select readme.md");
    c.refuses(&mut harness, "explorer select no-such-file.md");
    c.sets_up(&mut harness, "tab open src/main.rs");
    c.works(&mut harness, "explorer select-open-file");
    c.works(&mut harness, "explorer new-folder made");
    c.refuses(&mut harness, "explorer new-folder made");
    c.works(&mut harness, "explorer new-file made/today.md");
    c.refuses(&mut harness, "explorer new-file made/today.md");
    c.works(&mut harness, "explorer move made/today.md docs");
    c.refuses(&mut harness, "explorer move no-such-file.md docs");
    c.works(&mut harness, "explorer delete docs/today.md");
    c.refuses(&mut harness, "explorer delete no-such-file.md");
    c.works(&mut harness, "explorer reload");

    c.works(&mut harness, "action list --json");
    c.works(&mut harness, "action find line numbers --json");
    c.works(&mut harness, "action run toggle-line-numbers");
    c.refuses(&mut harness, "action run no-such-action");

    c.works(&mut harness, "plugins list --json");
    c.works(&mut harness, "plugins show mermaid --json");
    c.refuses(&mut harness, "plugins show no-such-plugin --json");
    c.works(&mut harness, "plugins disable mermaid");
    c.works(&mut harness, "plugins enable mermaid");
    c.refuses(&mut harness, "plugins enable no-such-plugin");
    c.works(&mut harness, "plugins install mermaid");
    c.refuses(&mut harness, "plugins install no-such-plugin");
    c.works(&mut harness, "plugins reload --json");
    c.works(&mut harness, "plugins pane agent-tasks/board --show");
    c.refuses(&mut harness, "plugins pane no-such/pane --show");
    c.works(&mut harness, "plugins run agent-tasks board --json");
    c.refuses(&mut harness, "plugins run no-such-plugin board");
    c.works(&mut harness, "plugins view agent-tasks --json");
    c.refuses(&mut harness, "plugins view no-such-plugin");
    c.refuses(&mut harness, "plugins tab no-such/tab --open");

    std::fs::remove_dir_all(&folder).ok();
}

/// The modals, the settings, the themes and the project's recent list.
fn drive_the_modals_and_the_settings(coverage: &mut Coverage) {
    let mut harness = harness_in(&dispatch_folder());
    let c = coverage;

    c.works(&mut harness, "modal list --json");
    c.works(&mut harness, "modal state --json");
    // Before one is open, so there is genuinely none to move, size, reset or cancel.
    c.refuses(&mut harness, "modal cancel");
    c.refuses(&mut harness, "modal move --x 10");
    c.refuses(&mut harness, "modal size --width 800");
    c.refuses(&mut harness, "modal reset");
    c.works(&mut harness, "modal open go-to-file --query one");
    c.refuses(&mut harness, "modal open no-such-modal");
    c.works(&mut harness, "modal type one");
    c.works(&mut harness, "modal results --limit 5 --json");
    c.works(&mut harness, "modal move --x 60 --y 60");
    c.works(&mut harness, "modal size --width 900 --height 600");
    c.works(&mut harness, "modal reset");
    c.works(&mut harness, "modal choose 0");
    c.works(&mut harness, "modal accept");
    c.sets_up(&mut harness, "modal open go-to-file --query nothing-matches-this");
    c.refuses(&mut harness, "modal choose 0");
    c.refuses(&mut harness, "modal accept 0");
    c.works(&mut harness, "modal cancel");

    c.works(&mut harness, "settings list --json");
    c.works(&mut harness, "settings get appearance.font.size");
    c.refuses(&mut harness, "settings get no.such.key");
    c.works(&mut harness, "settings set appearance.font.size 20");
    c.refuses(&mut harness, "settings set no.such.key 1");
    c.works(&mut harness, "settings reset appearance.font.size");
    c.refuses(&mut harness, "settings reset no.such.key");
    c.works(&mut harness, "settings fonts --json");

    // The backgrounds, which are files in Unluminous's own folder rather than a setting on its own.
    // Added from a picture this walk writes, so nothing depends on what is on the machine.
    let picture = a_picture_to_add();
    c.works(&mut harness, "background list --json");
    c.works(&mut harness, &format!("background add {} --keep", picture.display()));
    c.refuses(&mut harness, "background add no-such-picture.png");
    c.works(&mut harness, "background use a-background.png");
    c.refuses(&mut harness, "background use no-such-picture.png");
    c.works(&mut harness, "background remove a-background.png");
    c.refuses(&mut harness, "background remove no-such-picture.png");

    // A project of its own, in a folder this walk owns. `--no-git`, so it needs no git on the machine.
    let made = new_project_folder();
    c.works(
        &mut harness,
        &format!("explorer new-project a-new-project --location {} --no-git", made.display()),
    );
    c.refuses(&mut harness, "explorer new-project a/b");

    c.works(&mut harness, "theme list --json");
    c.works(&mut harness, "theme show --json");
    c.refuses(&mut harness, "theme show no-such-theme --json");
    c.works(&mut harness, "theme set unluminous/dark");
    c.refuses(&mut harness, "theme set no-such-theme");

    c.works(&mut harness, "project recent --json");
}

/// The terminal tabs and the run configurations.
fn drive_the_terminal_and_the_runs(coverage: &mut Coverage) {
    let mut harness = harness_in(&dispatch_folder());
    let c = coverage;

    c.works(&mut harness, "terminal list --json");
    c.works(&mut harness, "terminal show");
    c.works(&mut harness, "terminal hide");
    c.works(&mut harness, "terminal toggle");
    c.works(&mut harness, "terminal height 400");
    c.refuses(&mut harness, "terminal select 9");
    c.refuses(&mut harness, "terminal close 9");
    c.refuses(&mut harness, "terminal rename build --tab 9");
    c.refuses(&mut harness, "terminal move 0 --tab 9");
    c.refuses(&mut harness, "terminal send hello --tab 9");
    c.refuses(&mut harness, "terminal read --tab 9");

    // **A detached tab rather than `terminal new`.** A real shell answers when it answers, which is
    // `new_detached_terminal_tab`'s own bargain and the terminal's screenshots' rule. `terminal new`
    // is driven once at the end, where nothing after it depends on what it printed.
    harness.state_mut().new_detached_terminal_tab(20, 60);
    steady(&mut harness);
    c.works(&mut harness, "terminal select 0");
    c.works(&mut harness, "terminal rename build");
    c.works(&mut harness, "terminal move 0");
    c.works(&mut harness, "terminal send hello");
    c.works(&mut harness, "terminal read");
    c.works(&mut harness, "terminal close 0");
    c.works(&mut harness, "terminal new");

    c.works(&mut harness, "run list --json");
    // Before one has been added, so there is genuinely nothing to start or read.
    c.refuses(&mut harness, "run output --tail 5");
    c.refuses(&mut harness, "run start");
    c.works(&mut harness, "run add echoing cmd /c echo hello");
    c.refuses(&mut harness, "run add echoing cmd /c echo hello");
    c.works(&mut harness, "run select echoing");
    c.refuses(&mut harness, "run select no-such-run");
    c.works(&mut harness, "run status --json");
    c.works(&mut harness, "run start echoing");
    for _ in 0..40 {
        harness.step();
    }
    c.works(&mut harness, "run output --tail 5");
    c.works(&mut harness, "run stop");
    c.works(&mut harness, "run rerun");
    for _ in 0..40 {
        harness.step();
    }
    c.refuses(&mut harness, "run rerun no-such-run");
    c.refuses(&mut harness, "run stop no-such-run");
    c.refuses(&mut harness, "run remove no-such-run");
    c.works(&mut harness, "run remove echoing");
}

/// The canvas, its six kinds of node, and the commands that reach into one.
fn drive_the_canvas(coverage: &mut Coverage) {
    let mut harness = harness_in(&dispatch_folder());
    let c = coverage;

    c.works(&mut harness, "space show");
    c.works(&mut harness, "space view --json");
    c.works(&mut harness, "space list");
    c.works(&mut harness, "space here");
    c.works(&mut harness, "space views");
    c.works(&mut harness, "space manage");
    c.works(&mut harness, "space camera --fit");
    c.works(&mut harness, "space new-view Rendering");
    c.works(&mut harness, "space open-view Rendering");
    c.refuses(&mut harness, "space open-view Nosuchview");
    c.works(&mut harness, "space rename-view Rendering Second");
    c.works(&mut harness, "space duplicate-view Second");
    c.works(&mut harness, "space delete-view Second");
    c.refuses(&mut harness, "space delete-view Nosuchview");
    c.sets_up(&mut harness, "space open-view Main");

    // **Detached nodes for the three that would start something.** A terminal node runs a real shell
    // and a browser node wants a web view; `new_detached_space_node` is `task-1904`'s own answer, and
    // it is what `tests/canvas_space.rs` builds every one of its nodes with.
    let terminal = harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Terminal,
        egui::pos2(40.0, 40.0),
    );
    let browser = harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Browser,
        egui::pos2(400.0, 40.0),
    );
    let tree = harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Folder,
        egui::pos2(40.0, 300.0),
    );
    steady(&mut harness);
    let editor = did(&mut harness, "space add editor --path src/main.rs")["node"]
        .as_u64()
        .expect("the node the command made");
    let chat = did(&mut harness, "space add chat")["node"].as_u64().expect("the chat node");
    let tasks = did(&mut harness, "space add tasks")["node"].as_u64().expect("the tasks node");
    c.refuses(&mut harness, "space add nonsense");

    for (line, refusal) in [
        (format!("space move {terminal} --x 60"), "space move 99 --x 60"),
        (format!("space size {terminal} --width 500"), "space size 99 --width 500"),
        (format!("space title {terminal} named"), "space title 99 named"),
        (format!("space focus {terminal}"), "space focus 99"),
        (format!("space font {terminal} --size 16"), "space font 99 --size 16"),
        (format!("space zoom {terminal} --bigger"), "space zoom 99 --bigger"),
        (format!("space send {terminal} hello"), "space send 99 hello"),
        (format!("space read {terminal}"), "space read 99"),
        (format!("space restart {terminal}"), "space restart 99"),
        (format!("space editor {editor} src/other.rs"), "space editor 99 src/other.rs"),
        (format!("space folder {tree} rows"), "space folder 99 rows"),
        (format!("space chat {chat} state"), "space chat 99 state"),
        (
            format!("space address {browser} https://example.com/"),
            "space address 99 https://example.com/",
        ),
        (format!("space browser {browser} url"), "space browser 99 url"),
        (format!("space here --node {terminal}"), "space here --node 99"),
    ] {
        c.works(&mut harness, &line);
        c.refuses(&mut harness, refusal);
    }

    c.works(&mut harness, &format!("space connect {terminal} {editor}"));
    c.refuses(&mut harness, "space connect 99 98");
    let connections = did(&mut harness, "space connections --json");
    let connection = connections["connections"][0]["connection"]
        .as_u64()
        .expect("the connection that was just made");
    c.works(&mut harness, &format!("space disconnect {connection}"));
    c.refuses(&mut harness, "space disconnect 99");
    c.works(&mut harness, &format!("space remove {tasks}"));
    c.refuses(&mut harness, "space remove 99");
    c.works(&mut harness, "space hide");
}

/// The debugger, against a session with no adapter behind it.
///
/// Fifteen of the nineteen `debug` commands answer "nothing is being debugged" until something is, so
/// the walk drives them against [`paused_harness`] -- a session fed the DAP messages a real adapter
/// would have sent, stopped at line 4 of a real file with three locals in scope. It is
/// `tests/debugging.rs`'s own fixture, which is why `task-1922` moved it into `common`.
fn drive_a_paused_debugger(coverage: &mut Coverage) {
    let mut fresh = harness_in(&dispatch_folder());
    let c = coverage;

    // The refusals first, in a window where nothing is being debugged, because that is what most of
    // them refuse for.
    c.works(&mut fresh, "debug adapters --json");
    c.works(&mut fresh, "debug status --json");
    c.refuses(&mut fresh, "debug output --tail 5");
    c.refuses(&mut fresh, "debug frames --json");
    c.refuses(&mut fresh, "debug variables --json");
    c.refuses(&mut fresh, "debug evaluate items");
    c.refuses(&mut fresh, "debug hover --line 3 --column 9");
    c.refuses(&mut fresh, "debug set-value Locals/count 7");
    c.refuses(&mut fresh, "debug set-expression self.count 7");
    c.refuses(&mut fresh, "debug watch add attempts");
    c.refuses(&mut fresh, "debug stop");
    c.refuses(&mut fresh, "debug continue");
    c.refuses(&mut fresh, "debug step-over");
    c.refuses(&mut fresh, "debug step-into");
    c.refuses(&mut fresh, "debug step-out");
    c.refuses(&mut fresh, "debug run-to src/main.rs 3");
    c.refuses(&mut fresh, "debug start");
    c.works(&mut fresh, "debug breakpoint add src/main.rs 3");
    c.works(&mut fresh, "debug breakpoint list --json");
    c.works(&mut fresh, "debug breakpoint disable src/main.rs 3");
    c.works(&mut fresh, "debug breakpoint enable src/main.rs 3");
    c.works(&mut fresh, "debug breakpoint remove src/main.rs 3");
    c.works(&mut fresh, "debug breakpoint clear");
    c.refuses(&mut fresh, "debug breakpoint nonsense");

    let mut harness = paused_harness("dispatch-coverage");
    c.works(&mut harness, "debug output --tail 5");
    c.works(&mut harness, "debug frames --json");
    c.works(&mut harness, "debug variables --json");
    c.works(&mut harness, "debug watch add attempts");
    c.works(&mut harness, "debug watch list --json");
    c.works(&mut harness, "debug watch remove attempts");
    c.refuses(&mut harness, "debug watch nonsense");
    c.works(&mut harness, "debug evaluate attempts");
    c.works(&mut harness, "debug hover --line 4 --column 9");
    c.works(&mut harness, "debug set-value Locals/attempts 7");
    c.works(&mut harness, "debug set-expression attempts 9");
    // **Each of these five resumes the program**, so the session is running again the moment it
    // answers and the next one is correctly refused. `stop_the_session_again` is the adapter saying
    // it has stopped once more, which is what a real one sends after a step.
    let stopped_at = debug_folder("dispatch-coverage").join("main.rs");
    for line in ["debug step-over", "debug step-into", "debug step-out", "debug continue"] {
        c.works(&mut harness, line);
        stop_the_session_again(&mut harness, &stopped_at, 4, "step");
    }
    c.works(&mut harness, "debug run-to main.rs 5");
    stop_the_session_again(&mut harness, &stopped_at, 5, "breakpoint");
    c.works(&mut harness, "debug start");
    c.works(&mut harness, "debug stop");
}

/// The git commands, against a real repository built in a temporary folder.
fn drive_a_repository(coverage: &mut Coverage) {
    let mut nothing = harness_in(&dispatch_folder());
    let c = coverage;
    // A folder that is not a repository refuses four of the five, which is the state most people
    // meet first.
    c.refuses(&mut nothing, "git status --json");
    c.refuses(&mut nothing, "git branches --json");
    c.refuses(&mut nothing, "git switch main");

    let mut harness = git_harness("dispatch-coverage");
    settle(&mut harness, "the repository", |app| {
        app.git.as_ref().is_some_and(|git| !git.is_busy())
    });
    c.works(&mut harness, "git status --json");
    c.works(&mut harness, "git actions --json");
    c.works(&mut harness, "git branches --json");
    c.works(&mut harness, "git action annotate");
    c.refuses(&mut harness, "git action no-such-action");
    let current = did(&mut harness, "git branches --json")["current"]
        .as_str()
        .expect("the branch it is on")
        .to_owned();
    c.works(&mut harness, &format!("git switch {current}"));
    c.refuses(&mut harness, "git switch no-such-branch");
}

/// `plugins tab`, which needs a plugin that contributes one.
///
/// **From a manifest written for this walk**, because no plugin that ships contributes a tab -- the
/// reason `a_contributed_tab_opens_in_the_editing_area_beside_the_file_tabs` already gives. Unluminous's
/// tab machinery is part of the plugin contract, so it is driven through a manifest that asks for one
/// rather than left undriven until some plugin happens to want a tab again.
fn drive_a_contributed_tab(coverage: &mut Coverage) {
    let folder = copy_out_of_the_repository(&dispatch_folder(), "unluminous-dispatch-plugin-tab");
    let settings = folder.join(".unluminous-settings");
    let plugin = settings.join("plugins").join("agent-tasks");
    std::fs::create_dir_all(&plugin).expect("a plugin folder");
    std::fs::write(
        plugin.join("plugin.conf"),
        "plugin.id = agent-tasks\nplugin.name = Agent-Tasks\nplugin.kind = ui\n\
         ui.provider = agent-tasks\ntab.id = board\ntab.label = Agent-Tasks\n",
    )
    .expect("a manifest that contributes a tab");
    let mut harness = harness_in(&folder);
    harness.state_mut().use_store(unluminous_app::services::store::Store::at(&settings));
    steady(&mut harness);
    coverage.works(&mut harness, "plugins tab agent-tasks/board --open");
    coverage.works(&mut harness, "plugins tab agent-tasks/board --close");
    std::fs::remove_dir_all(&folder).ok();
}

/// Opening another project, which takes the window away from the one it was on.
///
/// Last, and in a window of its own, for that reason.
fn drive_the_project(coverage: &mut Coverage) {
    let mut harness = harness_in(&dispatch_folder());
    coverage.refuses(&mut harness, "project open no-such-folder-anywhere");
    let second = fixture("unluminous-dispatch-coverage-second", &[("second.md", "# Second\n")]);
    coverage.works(&mut harness, &format!("project open {}", second.display()));
}

// =================================================================================================
// The socket, and the Model Context Protocol server, driven end to end.
//
// `task-1922` §5.4. Every other test in this file calls `run_command_line` or `run_cli_for_test`
// directly, which is the whole command line path **apart from the socket** -- and the socket is
// where the token, the queue, the deadline and the frame that answers live. These two are the only
// tests anywhere that run the real `unluminous-cli` program against a real window, so each is kept
// small and made to fail for one reason.

/// Where the built `unluminous-cli` is, beside the test binary that is running.
///
/// **Not `CARGO_BIN_EXE_unluminous-cli`**, which cargo sets only for the tests of the package that
/// declares the binary -- `unluminous-cli`'s own -- and this is `unluminous-app`'s. The test binary
/// is at `target/<profile>/deps/<name>-<hash>.exe`, so the program is two folders up, which is the
/// same place `cargo build` puts it in whichever profile this run is using.
///
/// It is **built on demand** when it is not there, because `cargo test -p unluminous-app` has no
/// reason to build another package's binary and a test that quietly verified nothing on a fresh
/// checkout would be worse than one that waits. `cargo test` holds the build lock only while it is
/// building, so a build started from inside a test that is already running does not deadlock
/// against it.
fn the_command_line_program() -> Option<std::path::PathBuf> {
    static BUILT: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let name = format!("unluminous-cli{}", std::env::consts::EXE_SUFFIX);
            let beside = std::env::current_exe()
                .ok()?
                .parent()?
                .parent()
                .map(|profile| profile.join(&name))?;
            if beside.exists() {
                return Some(beside);
            }
            let manifest =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
            let built = std::process::Command::new(
                std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()),
            )
            .args(["build", "-p", "unluminous-cli", "--manifest-path"])
            .arg(&manifest)
            .status();
            match built {
                Ok(status) if status.success() && beside.exists() => Some(beside),
                _ => None,
            }
        })
        .clone()
}

/// A window with its command channel open, and the folder its instance file was written into.
///
/// `UnluminousApp::open_control_channel` says it is called from `main.rs` and nowhere else, because
/// a test must not open a port, write an instance file into the person's settings folder, or leave
/// a listener behind. Two of those three are answered here: `UNLUMINOUS_INSTANCES` moves the
/// instance file into a folder of this test's own, which `unluminous_cli::instances::folder` reads
/// and the child process is given as well, and `Server`'s own `Drop` takes the file away again. The
/// third stands -- the listener goes with the process, as it does in the released binary -- and it
/// is a loopback port on an address the operating system chose, which is what `bind` guarantees.
/// Held for the length of each of the two tests below, so only one of them has a channel open.
///
/// **Two windows in one process cannot both advertise.** An instance file is named after the process
/// id, and both of these windows are this one test binary, so the second to start would write over
/// the first's file and the first to finish would take it away. `UNLUMINOUS_INSTANCES` is a process
/// wide variable as well, and each of these tests points it somewhere of its own. So they take it in
/// turns, which costs a few seconds and is the only thing that makes either of them honest when
/// `cargo test` runs them on two threads.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn a_window_on_a_port(name: &str) -> (Harness<'static, UnluminousApp>, std::path::PathBuf) {
    let instances = std::env::temp_dir().join(format!("unluminous-instances/{name}"));
    std::fs::remove_dir_all(&instances).ok();
    std::fs::create_dir_all(&instances).expect("a folder for the instance file");
    // Read by `instances::folder` in this process when the channel opens, and handed to the child
    // below so both halves look in the same place. Safe in this edition, and set to the same value
    // by every test here, so two running at once cannot disagree about it.
    std::env::set_var("UNLUMINOUS_INSTANCES", &instances);

    let folder = fixture(
        &format!("unluminous-socket/{name}"),
        &[("readme.md", "# Readme\n"), ("notes.txt", "notes\n")],
    );
    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    harness.state_mut().open_control_channel(&ctx);
    steady(&mut harness);
    // Asked the way anything else would ask: `mcp status` reports what is really happening rather
    // than what the settings say, and whether there is a command channel at all is one of its
    // fields.
    assert_eq!(
        did(&mut harness, "mcp status --json")["controlChannel"],
        serde_json::json!(true),
        "the window should be listening; it is what the rest of this test drives"
    );
    (harness, instances)
}

/// Draw frames until `finished` answers, so the window can pick a request up and answer it.
///
/// A request reaches the window only at the top of a frame -- `pump_control` is called from
/// `UnluminousApp::ui` -- so a test that spawned a program and then waited for it would wait for
/// ever. The wait is bounded so a fault is a failure rather than a run that never ends.
fn step_until(
    harness: &mut Harness<'static, UnluminousApp>,
    what: &str,
    mut finished: impl FnMut() -> bool,
) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < until {
        if finished() {
            return;
        }
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("{what} did not happen within thirty seconds");
}

/// The real `unluminous-cli`, over the real socket, answered by a real window.
///
/// `status --section keyboard` because it is the one answer that is about the window rather than
/// about a document: `holder`, `textBox`, `node` and `pane` are four different questions and a
/// window with nothing set up still answers all four. What is checked is the reply's shape, not its
/// wording.
#[test]
fn the_command_line_program_drives_a_real_window_over_the_socket() {
    // Declared first, so it is released last -- after the harness, and so after the `Server`
    // inside it has taken its instance file away again.
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|held| held.into_inner());
    let Some(program) = the_command_line_program() else {
        panic!("unluminous-cli could not be found beside the test binary and could not be built");
    };
    let (mut harness, instances) = a_window_on_a_port("status");
    let pid = std::process::id().to_string();

    let mut child = std::process::Command::new(&program)
        .args(["--instance", &pid, "status", "--section", "keyboard", "--json"])
        .env("UNLUMINOUS_INSTANCES", &instances)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("start unluminous-cli");

    // The child blocks on the window's answer and the window only answers on a frame, so the frames
    // are drawn here while it waits.
    step_until(&mut harness, "unluminous-cli answering", || {
        matches!(child.try_wait(), Ok(Some(_)))
    });
    let answer = child.wait_with_output().expect("read what unluminous-cli said");
    let said = String::from_utf8_lossy(&answer.stdout).to_string();
    assert!(
        answer.status.success(),
        "unluminous-cli failed: {said}{}",
        String::from_utf8_lossy(&answer.stderr)
    );

    let reply: serde_json::Value =
        serde_json::from_str(&said).unwrap_or_else(|problem| panic!("{problem}: {said}"));
    assert_eq!(reply["ok"], serde_json::json!(true), "{said}");
    assert_eq!(reply["command"], serde_json::json!("status"), "{said}");
    let keyboard = &reply["result"]["keyboard"];
    assert!(keyboard.is_object(), "the keyboard section is the whole of the answer: {said}");
    for field in ["holder", "textBox"] {
        assert!(!keyboard[field].is_null(), "the keyboard section has no {field}: {said}");
    }
    // The one section that was asked for and nothing else, which is the rule `status --section`
    // keeps and is the half a test calling `run_cli` directly already covers. Here it is proof the
    // whole answer travelled down the socket rather than being assembled by the client.
    assert!(reply["result"]["tabs"].is_null(), "only the section asked for comes back: {said}");

    std::fs::remove_dir_all(&instances).ok();
}

/// The Model Context Protocol server, spawned against the same window, listed and called.
///
/// `mcp serve` is the one command held back from the tools it generates, and it is also the one
/// nothing could test without a second process: the server reads its requests off standard input
/// and drives the window down the same socket `unluminous-cli` uses. So what is under test is the
/// whole of that path -- a `tools/list` answered out of the catalogue, then a `tools/call` that
/// becomes a request on the window's queue -- and the state change is read back through the window
/// rather than believed from the reply.
#[test]
fn the_mcp_server_lists_its_tools_and_calls_one_against_a_real_window() {
    // Declared first, so it is released last -- after the harness, and so after the `Server`
    // inside it has taken its instance file away again.
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|held| held.into_inner());
    let Some(program) = the_command_line_program() else {
        panic!("unluminous-cli could not be found beside the test binary and could not be built");
    };
    let (mut harness, instances) = a_window_on_a_port("mcp");
    let pid = std::process::id().to_string();

    let mut child = std::process::Command::new(&program)
        .args(["mcp", "serve", "--instance", &pid])
        .env("UNLUMINOUS_INSTANCES", &instances)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("start the MCP server");

    // Read on a thread of its own, because the window has to go on drawing while the server thinks.
    let stdout = child.stdout.take().expect("the server's standard output");
    let (lines, from_the_server) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            if lines.send(line).is_err() {
                return;
            }
        }
    });

    let mut ask = |request: serde_json::Value| {
        use std::io::Write;
        let standard_input = child.stdin.as_mut().expect("the server's standard input");
        writeln!(standard_input, "{request}").expect("write a request");
        standard_input.flush().expect("flush the request");
    };

    ask(serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }));
    ask(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "unluminous_tab",
            "arguments": { "command": "open", "arguments": { "path": "notes.txt" } }
        }
    }));

    let mut answers: Vec<serde_json::Value> = Vec::new();
    step_until(&mut harness, "the MCP server answering both requests", || {
        while let Ok(line) = from_the_server.try_recv() {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                answers.push(value);
            }
        }
        answers.len() >= 2
    });
    child.kill().ok();
    child.wait().ok();

    let listed = answers.iter().find(|answer| answer["id"] == 1).expect("an answer to tools/list");
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("the tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"unluminous_tab"), "no tool for the tab area: {names:?}");

    let called = answers.iter().find(|answer| answer["id"] == 2).expect("an answer to tools/call");
    assert_eq!(called["result"]["isError"], serde_json::json!(false), "{called}");

    // **Read back through the window**, which is the half a reply cannot stand in for: a tool that
    // says it opened a file and a window with that file open are two different claims.
    steady(&mut harness);
    let open: Vec<String> = harness
        .state()
        .files
        .paths()
        .iter()
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().to_string()))
        .collect();
    assert!(open.contains(&"notes.txt".to_owned()), "the tool call opened nothing: {open:?}");

    std::fs::remove_dir_all(&instances).ok();
}

// -------------------------------------------------------------------------------------- task-1984
//
// `update check` is answered on a thread, against a server a test can stand up.

/// `update check` does not stop the window drawing, and it reads its own `--timeout`.
///
/// **`task-1984` L1.** It used to call the blocking `update::ask` inside `run_cli`, which runs at the
/// top of a frame — so one command stopped the window drawing for up to ten seconds against an
/// endpoint that is slow or unreachable, which is the one thing `unluminous_git::Worker`'s comment
/// says never to do: it looks exactly like a crash, and the control channel is read at the top of a
/// frame too, so `unluminous-cli status` stopped answering as well. `Check::start` in the same file
/// was already doing it the right way for the About box.
///
/// **Measured rather than asserted about the shape**: the first half points the check at a server
/// that accepts the connection and never answers, and insists the command comes back in under a
/// second. On the code as it was that call took the whole timeout.
///
/// `UNLUMINOUS_RELEASES` is what makes this possible at all — a scripted endpoint on loopback,
/// which is the seam `UNLUMINOUS_HOME` and `UNLUMINOUS_INSTANCES` already are.
#[test]
fn update_check_does_not_block_the_frame_it_arrives_on() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|held| held.into_inner());

    // A releases endpoint that accepts and says nothing, which is what a slow GitHub looks like.
    let silent = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = silent.local_addr().expect("the address").port();
    let held_open = std::thread::spawn(move || {
        let taken = silent.accept();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        drop(taken);
    });
    std::env::set_var("UNLUMINOUS_RELEASES", format!("http://127.0.0.1:{port}/releases/latest"));

    let mut harness = harness("# a file\n");
    let ctx = harness.ctx.clone();
    let began = std::time::Instant::now();
    let answered = harness.state_mut().run_command_line("update check --timeout 20000", &ctx);
    let took = began.elapsed();

    assert!(
        answered.is_none(),
        "the answer belongs to a later frame, as every slow command's does"
    );
    assert!(
        took < std::time::Duration::from_millis(1000),
        "the command came back in {took:?}; on the code as it was it held the frame for the whole \
         timeout, which is what makes a window look like it has crashed"
    );
    // And the window really is still drawing while the check is in flight.
    for _ in 0..4 {
        pump(&mut harness);
    }
    let began = std::time::Instant::now();
    let status = did(&mut harness, "status");
    assert!(
        began.elapsed() < std::time::Duration::from_millis(500),
        "the control channel is read at the top of a frame too, so a blocked frame stops `status` \n         answering as well -- and it answered at once"
    );
    assert!(status.get("window").is_some(), "and answered with a window: {status}");

    std::env::remove_var("UNLUMINOUS_RELEASES");
    let _ = held_open.join();
}

/// And the answer, when the releases page does answer, is the one it gave.
#[test]
fn update_check_answers_with_what_the_releases_page_said() {
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|held| held.into_inner());

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("the address").port();
    let served = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            use std::io::{Read as _, Write as _};
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer);
            let body =
                r#"{"tag_name":"v99.0.0","html_url":"https://example.invalid/99","body":"newer"}"#;
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                     connection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        }
    });
    std::env::set_var("UNLUMINOUS_RELEASES", format!("http://127.0.0.1:{port}/releases/latest"));

    let mut harness = harness("# a file\n");
    let ctx = harness.ctx.clone();
    assert!(harness.state_mut().run_command_line("update check --timeout 20000", &ctx).is_none());

    // The answer is kept on the window, so the About box after this shows what the check found —
    // one place a check's answer lives, which is `run_cli`'s rule.
    settle(&mut harness, "the releases page to answer", |app| {
        app.update_line().is_some_and(|line| line != "Checking...")
    });
    assert_eq!(
        harness.state().update_line().as_deref(),
        Some("99.0.0 is available"),
        "the scripted releases page named 99.0.0"
    );

    std::env::remove_var("UNLUMINOUS_RELEASES");
    let _ = served.join();
}

/// Every command the catalogue says is answered on a later frame really is.
///
/// **`task-1984` L6.** `Command::answered_later` is a declaration in `unluminous-cli`, because what
/// really decides it is which arm of `UnluminousApp::run_cli` returns `Outcome::Hold` and that lives
/// here, in the crate the catalogue cannot see. `docs/protocol.md` is generated against the
/// declaration, and the declaration was wrong in both directions before this: it named `launch`,
/// which the client answers itself and no window ever sees, and it omitted `status`, `git status`,
/// the five `input` commands and `space browser <node> shot`.
///
/// Driven rather than read, for the ones a test window can drive: `run_command_line` answers `None`
/// for a command the window held, which is the property itself.
#[test]
fn every_command_that_holds_its_answer_says_so_in_the_catalogue() {
    let mut harness = harness_in(&dispatch_folder());
    let ctx = harness.ctx.clone();

    // **Declared means *can* hold, not *always* holds**, which is the property a client has to
    // handle: `status` and `git status` hold only while git is still reading the repository, and
    // `run output` only while the words it is waiting for have not been written. So what is driven
    // here is the ones that hold every time in a window with no project behind it, and the rest are
    // driven by the suites they belong to.
    let holds = [
        "input move 10 10",
        "input click 10 10",
        "input drag 10 10 --to-x 20 --to-y 20",
        "input key Escape",
        "input text hello",
        "input wheel -1",
    ];
    for line in holds {
        let first = line.split_whitespace().take(2).collect::<Vec<&str>>().join(" ");
        let command = unluminous_cli::catalogue::find(&first)
            .or_else(|| unluminous_cli::catalogue::find(line.split_whitespace().next().unwrap()))
            .unwrap_or_else(|| panic!("`{first}` is a command"));
        assert!(
            command.answered_later(),
            "`{}` holds its answer and the catalogue says it does not",
            command.typed()
        );
        let answered = harness.state_mut().run_command_line(line, &ctx);
        note_a_driven_line(line, true);
        assert!(
            answered.is_none(),
            "`{line}` was answered on the frame it arrived on, so `answered_later` is wrong about it"
        );
        pump(&mut harness);
    }

    // And the other way: a command that answers at once must not claim to hold, or a client would
    // wait for a reply that has already been sent.
    for line in ["tab list", "editor status", "panel list", "theme list"] {
        let first = line.split_whitespace().take(2).collect::<Vec<&str>>().join(" ");
        let command = unluminous_cli::catalogue::find(&first).expect("a command");
        assert!(
            !command.answered_later(),
            "`{}` answers at once and the catalogue says it holds",
            command.typed()
        );
        assert!(
            harness.state_mut().run_command_line(line, &ctx).is_some(),
            "`{line}` really does answer at once"
        );
        pump(&mut harness);
    }
}

/// A picture on disk for `background add` to copy in, written once.
///
/// A one pixel PNG rather than anything on the machine, so the walk depends on nothing it did not make
/// — `common::sample_folder`'s own bargain.
fn a_picture_to_add() -> std::path::PathBuf {
    let folder = std::env::temp_dir().join("unluminous-command-line-backgrounds");
    std::fs::create_dir_all(&folder).expect("make the folder");
    let at = folder.join("a-background.png");
    let bytes: [u8; 67] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    std::fs::write(&at, bytes).expect("write the picture");
    at
}

/// An empty folder for `explorer new-project` to make a project in.
fn new_project_folder() -> std::path::PathBuf {
    let folder = std::env::temp_dir().join("unluminous-command-line-projects");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("make the folder");
    folder
}
