//! The window's own furniture: the title bar, the status bar, the explorer, the tabs, the menus,
//! the modals, the Markdown preview, and who holds the keyboard.
//!
//! Everything a person sees that is not the text: the window buttons and what a maximise does to
//! them, the project's name, the line and column, the file tree and its filter, its menu and the
//! rows it draws, making, renaming, deleting and moving a file, dragging a tab, the three view modes
//! and the preview behind them, the File, View and Edit menus, the About box, the Settings window,
//! `Go to File`, `Find in Files`, a modal being dragged and resized, pictures in a tab and in a
//! preview, a Markdown link opening in a browser, and the several windows a person can have open.
//!
//! The keyboard tests are here for a reason that is easy to lose: a window button that holds egui's
//! focus is pressed by a space, so typing one closed the window. That is furniture behaving badly
//! rather than a fault in the editing area.
//!
//! **42 of the 138 tests here take a picture**, and the rest drive the window and read its state
//! back.

mod common;

use common::*;

use egui::{vec2, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::app::actions::Action;
use unluminous_app::app::ViewMode;
use unluminous_app::components::about_dialog::About;
use unluminous_app::components::title_bar::MenuPlacement;
use unluminous_app::theme::size;
use unluminous_app::UnluminousApp;
use unluminous_core::Command;

/// **Every accepted image on this platform is named by some test.** `task-1922`.
///
/// A picture nobody takes any more is a picture nobody looks at, and it stays in the repository
/// being read as evidence of something. The review found two: `agent_tasks_pane.png` and
/// `agent_tasks_detail.png`, neither of which any test had mentioned since the board's own tab was
/// split out of it.
///
/// **It reads the test source rather than recording what ran**, because `cargo test <filter>` runs a
/// subset and a test that added up what it had seen would fail on every filtered run. All of the
/// source, because the tests are twelve files and this one is in one of them.
///
/// The rule is a substring search rather than a parse. A name counts as used when `"the_name"`
/// appears in the source, or when the name can be cut in two so that `"head{` and `"tail"` both do --
/// which is how the twenty `mermaid_<type>` images are named, from `shot(&format!("mermaid_{name}"))`
/// and the list of types beside it. Pairing up quotation marks was tried first and is wrong here: the
/// files hold raw strings of sample source code with quotation marks inside them, and one of those
/// puts every literal after it on the wrong side of the count.
#[test]
fn every_accepted_image_is_named_by_a_test() {
    let source = every_test_source();
    let named = |stem: &str| {
        if source.contains(&format!("\"{stem}\"")) {
            return true;
        }
        // `shot(&format!("<head>{...}"))` with `<tail>` as one of the words beside it.
        (1..stem.len()).any(|at| {
            source.contains(&format!("\"{}{{", &stem[..at]))
                && source.contains(&format!("\"{}\"", &stem[at..]))
        })
    };

    let platform = shot("");
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(platform.trim_end_matches('/'));
    let mut orphans: Vec<String> = std::fs::read_dir(&folder)
        .expect("the accepted images")
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".png")?.to_owned();
            let scratch = [".new", ".diff", ".old"].iter().any(|end| stem.ends_with(end));
            (!scratch && !named(&stem)).then_some(stem)
        })
        .collect();
    orphans.sort();
    assert!(
        orphans.is_empty(),
        "these accepted images are named by no test, so nothing takes them any more: {orphans:?}"
    );
}

/// Every `.rs` file under `tests/`, run together into one string.
///
/// Read off the disk rather than through `include_str!`, because the tests are twelve files and a
/// list of them written out here would be a list whose next entry is the one somebody forgets.
fn every_test_source() -> String {
    fn read(folder: &std::path::Path, into: &mut String) {
        for entry in std::fs::read_dir(folder).expect("the test sources").flatten() {
            let path = entry.path();
            if path.is_dir() {
                read(&path, into);
            } else if path.extension().is_some_and(|end| end == "rs") {
                into.push_str(&std::fs::read_to_string(&path).expect("read a test source"));
            }
        }
    }
    let mut source = String::new();
    read(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"), &mut source);
    source
}

#[test]
fn the_title_bar_names_the_project_and_carries_the_window_buttons_and_the_text_tools() {
    // `task-1658` took the open file's name out of the bar, because it is on its own tab already, and
    // put the project's name after the menus instead. The text tools moved in beside the window
    // buttons at the same time.
    let mut harness = harness("");
    harness.get_by_label_contains("readme.md").click();
    harness.run();
    for button in ["Close", "Minimise", "Maximise"] {
        harness.get_by_label(button);
    }
    let title_bar =
        egui::Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(WINDOW[0], size::TITLE_BAR));
    for tool in ["Text options", "Raw Markdown", "Side by side", "Markdown preview"] {
        let at = harness.get_by_label(tool).rect();
        assert!(
            title_bar.contains_rect(at),
            "{tool} should be in the title bar, and it is at {at:?}"
        );
    }
    harness.snapshot(shot("title_bar"));
}

/// Every window command this frame, as text, whatever kind it is.
fn viewport_commands(harness: &Harness<'static, UnluminousApp>) -> Vec<String> {
    harness
        .output()
        .viewport_output
        .values()
        .flat_map(|viewport| viewport.commands.iter())
        .map(|command| format!("{command:?}"))
        .collect()
}

/// Press and release the primary button at `pos`, `times` over, in one frame.
///
/// Two of them inside egui's double click window is what `double_clicked` reads. `click_at` further
/// down sends one click and moves the pointer first; this one exists because a double click has to be
/// two presses with nothing between them.
fn click_repeatedly_at(
    harness: &mut Harness<'static, UnluminousApp>,
    pos: egui::Pos2,
    times: usize,
) {
    for _ in 0..times {
        for pressed in [true, false] {
            harness.input_mut().events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::default(),
            });
        }
    }
    harness.step();
}

#[test]
fn the_green_button_goes_full_screen_on_macos_and_maximises_everywhere_else() {
    // The green button on macOS puts a window in a space of its own, which is what it does in every
    // other application there. Off macOS the same button still maximises, because that is what the
    // platform it is drawn for means by it. Two behaviours, so two commands.
    let mut mac = harness("");
    mac.state_mut().menu_placement = MenuPlacement::Native;
    mac.run();
    mac.get_by_label("Maximise").click();
    // `step` and not `run`: `run` keeps painting until the window settles, and the commands read
    // below are the ones a single frame sent, so the frame that handled the click has to be the last.
    mac.step();
    let sent = viewport_commands(&mac);
    assert!(
        sent.iter().any(|command| command == "Fullscreen(true)"),
        "the green button should go full screen on macOS, and it sent {sent:?}"
    );
    // And it is full screen rather than both: a maximise as well would fill the desktop first and
    // then leave it, which is the flicker this separation exists to avoid.
    assert!(
        !sent.iter().any(|command| command.starts_with("Maximized")),
        "the green button on macOS should not also maximise, and it sent {sent:?}"
    );

    let mut windows = harness("");
    windows.state_mut().menu_placement = MenuPlacement::InWindow;
    windows.run();
    windows.get_by_label("Maximise").click();
    windows.step();
    let sent = viewport_commands(&windows);
    assert!(
        sent.iter().any(|command| command == "Maximized(true)"),
        "the maximise button should maximise off macOS, and it sent {sent:?}"
    );
    assert!(
        !sent.iter().any(|command| command.starts_with("Fullscreen")),
        "the maximise button off macOS should not go full screen, and it sent {sent:?}"
    );
}

#[test]
fn a_double_click_on_the_bar_maximises_on_macos_rather_than_going_full_screen() {
    // What `task-1771` shipped, and what the green button's change had to leave alone: the bar's own
    // double click fills the desktop the window is already on, macOS included. The point at the
    // middle of the bar is the draggable room, clear of every control at either end.
    let mut harness = harness("");
    harness.state_mut().menu_placement = MenuPlacement::Native;
    harness.run();
    let middle = egui::pos2(WINDOW[0] / 2.0, size::TITLE_BAR / 2.0);
    click_repeatedly_at(&mut harness, middle, 2);

    let sent = viewport_commands(&harness);
    assert!(
        sent.iter().any(|command| command.starts_with("Maximized")),
        "a double click on the bar should maximise, and it sent {sent:?}"
    );
    assert!(
        !sent.iter().any(|command| command.starts_with("Fullscreen")),
        "a double click must not go full screen, and it sent {sent:?}"
    );
}

#[test]
fn an_edited_file_is_marked_as_unsaved_in_three_places() {
    let mut harness = harness("");
    harness.get_by_label_contains("readme.md").click();
    harness.run();
    assert!(!harness.state().document().is_modified(), "just opened, so nothing to save");
    harness.input_mut().events.push(egui::Event::Text(" edited".to_owned()));
    harness.run();
    assert!(harness.state().document().is_modified());
    // The dot appears in the title bar, on the file's row in the explorer and in the status bar. The
    // screenshot is how those are checked; this asserts the state that drives all three.
    harness.snapshot(shot("unsaved"));
}

#[test]
fn the_status_bar_counts_the_line_and_column_from_one() {
    let mut harness = harness("first line\nsecond line");
    harness.state_mut().command(Command::MoveDocumentStart { extend: false });
    harness.run();
    assert_eq!(
        harness.state().caret_position(),
        unluminous_app::components::status_bar::Position { line: 1, column: 1 }
    );
    harness.state_mut().command(Command::MoveDocumentEnd { extend: false });
    harness.run();
    assert_eq!(
        harness.state().caret_position(),
        unluminous_app::components::status_bar::Position { line: 2, column: 12 },
        "the caret is after the eleventh character of the second line"
    );
}

#[test]
fn the_column_counts_characters_rather_than_bytes() {
    // Five accented letters, each a letter plus a combining accent, so eleven bytes and five characters.
    let mut harness = harness("e\u{0301}e\u{0301}e\u{0301}e\u{0301}e\u{0301}");
    harness.state_mut().command(Command::MoveDocumentEnd { extend: false });
    harness.run();
    assert_eq!(harness.state().document().text().len_bytes(), 15);
    assert_eq!(
        harness.state().caret_position(),
        unluminous_app::components::status_bar::Position { line: 1, column: 6 },
        "five characters along, so column six, not column sixteen"
    );
}

#[test]
fn the_filter_box_narrows_the_list_to_matching_files() {
    let mut harness = harness("");
    let all = harness.state().tree.file_count();
    assert_eq!(all, 9, "the sample folder holds nine files");
    assert_eq!(
        harness.state().tree.openable_count(),
        8,
        "every one but the archive can be opened, including the Rust file and the picture"
    );
    harness.state_mut().filter = "two".to_owned();
    harness.run();
    let matches = harness.state().tree.matching("two");
    assert_eq!(matches.len(), 1, "only two.md matches");
    harness.snapshot(shot("filter"));
}

/// `task-28`: "There should be a toggle on the left, under the folder icon, for file view. When that is
/// pressed, it shows/hides the file pane on the right of folder pane (file pane has all the tabs)."
///
/// The pane holding the tabs is the editing area. The button is the third in the rail's top group, under the
/// folder, and it is called `Editor` because that is what the `View` menu's two entries call it and no two
/// controls in one window may share a name.
#[test]
fn the_editing_area_can_be_hidden_from_the_rail_and_the_explorer_takes_the_width() {
    let mut harness = harness("Some writing in a file.");
    assert!(harness.state().editor_visible, "it starts showing");
    let full = harness.state().panel_area(unluminous_app::app::dock::Panel::Explorer).width();

    // The button a person presses, found by its tooltip.
    harness.get_by_label("Editing Area").click();
    harness.run();
    assert!(!harness.state().editor_visible, "the button hid it");
    // No tab strip, because there is nothing for it to be the top of. `sample_folder` always has a
    // real `notes.txt` in the explorer, which stays visible with the editing area hidden, so the
    // tab this harness opened is the one thing to check for: its own `untitled` label.
    assert!(
        harness.query_by_label("untitled").is_none(),
        "no tab is drawn while the editing area is hidden"
    );
    let widened = harness.state().panel_area(unluminous_app::app::dock::Panel::Explorer).width();
    assert!(widened > full, "the explorer took the room: {full} then {widened}");

    // And back, from the `View` menu this time, which is the other control for the same action.
    did(&mut harness, "action run toggle-editor");
    harness.run();
    assert!(harness.state().editor_visible, "the menu entry brought it back");
    assert_eq!(
        harness.state().panel_area(unluminous_app::app::dock::Panel::Explorer).width(),
        full,
        "and the explorer is the width it was"
    );
}

/// Hiding the editing area can never leave a window with nothing in it. Stated in both directions, because both
/// are reachable: hide the explorer then the editing area, or hide the editing area then the explorer.
#[test]
fn the_window_always_has_either_a_panel_or_the_editing_area() {
    let mut harness = harness("");
    // Hide every panel, then ask for the editing area to go: the explorer comes back instead of nothing.
    did(&mut harness, "action run toggle-explorer");
    harness.run();
    assert!(!harness.state().explorer_visible);
    did(&mut harness, "action run toggle-editor");
    harness.run();
    assert!(!harness.state().editor_visible, "the editing area is hidden as asked");
    assert!(
        harness.state().explorer_visible,
        "and the explorer came back, so there is something to look at"
    );

    // The other way round: with the editing area hidden, hiding the explorer brings the editing area back.
    did(&mut harness, "action run toggle-explorer");
    harness.run();
    assert!(!harness.state().explorer_visible, "the explorer is hidden as asked");
    assert!(harness.state().editor_visible, "and the editing area came back");
}

#[test]
fn the_explorer_can_be_hidden_and_brought_back() {
    let mut harness = harness("");
    assert!(harness.state().explorer_visible);
    let editor_with = harness.state().editor_area().width();
    harness.get_by_label("Hide the explorer").click();
    harness.run();
    assert!(!harness.state().explorer_visible);
    let editor_without = harness.state().editor_area().width();
    assert!(
        editor_without > editor_with,
        "hiding the explorer should give the editor its width: {editor_with} then {editor_without}"
    );
    harness.snapshot(shot("explorer_hidden"));
    // The rail is what brings it back. There used to be a small button floating over the editing area
    // for this, and the rail replaced it: it is in the same place whether the explorer is showing or
    // not, which a button drawn only when the pane is hidden is not.
    harness.get_by_label("Project").click();
    harness.run();
    assert!(harness.state().explorer_visible);
}

#[test]
fn the_formatting_controls_are_behind_the_font_button_and_all_reachable_by_name() {
    let mut harness = harness("some text");
    // Shut, the strip holds one control. This is the half of `task-1657` that took the formatting
    // off the top of the window: nine controls that are set rarely no longer take its whole width.
    harness.get_by_label("Text options");
    assert!(
        harness.query_by_label("Bold").is_none(),
        "the formatting is behind the button until the button is pressed"
    );
    open_text_options(&mut harness);
    for name in [
        "Bold",
        "Italic",
        "Underline",
        "Strikethrough",
        "Left",
        "Center",
        "Right",
        "Justify",
        "Single",
        "One and a half",
        "Double",
    ] {
        harness.get_by_label(name);
    }
    for colour in ["White", "Red", "Green", "Blue", "Amber"] {
        harness.get_by_label(colour);
    }
    harness.snapshot(shot("text_options"));
}

#[test]
fn a_code_file_has_no_text_tools_and_nothing_below_them_moves() {
    // Everything in the tools is about how prose is shown, and Unluminous saves plain text and carries no
    // formatting to disk, so bold on a `.rs` file is a decoration that lasts until the file is
    // reopened. They are not drawn at all for one.
    //
    // They used to sit in a strip of their own, forty four points tall, so switching between a `.md`
    // file and a `.rs` one moved the tabs, the explorer and the editing area up and down by forty four
    // points. `task-1658` moved them into the title bar, whose height never changes, and this is the
    // half of that which is worth a test: the window below them does not move.
    let mut prose = harness("");
    prose.get_by_label("readme.md").click();
    prose.run();
    let with_tools = prose.state().editor_area().top();
    prose.get_by_label("Text options");

    let mut code = harness("");
    code.get_by_label("program.rs").click();
    code.run();
    assert!(
        code.query_by_label("Text options").is_none(),
        "a Rust file has no formatting to offer"
    );
    assert!(code.query_by_label("Raw Markdown").is_none(), "and nothing to preview either");
    let without_tools = code.state().editor_area().top();
    assert!(
        (with_tools - without_tools).abs() < 0.5,
        "the editing area should start in the same place either way: {with_tools} against {without_tools}"
    );
    code.snapshot(shot("code_no_toolbar"));
}

#[test]
fn moving_between_file_tabs_does_not_type_a_tab_into_the_file_it_leaves() {
    // Control and Tab is `Next Tab` on the View menu, and finding an action for a key press does not
    // consume it, so the editing area saw the same press and inserted a tab character. Both files
    // came out marked as having unsaved changes that nobody had made — found while retaking the
    // documentation captures for `task-1657`, and the same shape of fault as `task-1656`.
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    harness.state_mut().open_path_permanently(&folder.join("notes.txt")).expect("the file opens");
    harness.run();
    let before: Vec<String> =
        harness.state().files.iter().map(|file| file.document.text().to_string()).collect();

    harness.key_press_modifiers(Modifiers::CTRL, egui::Key::Tab);
    harness.run();
    harness.key_press_modifiers(Modifiers::CTRL | Modifiers::SHIFT, egui::Key::Tab);
    harness.run();

    for (index, file) in harness.state().files.iter().enumerate() {
        assert_eq!(file.document.text().to_string(), before[index], "tab {index} was typed into");
        assert!(!file.document.is_modified(), "tab {index} should have no unsaved changes");
    }
}

#[test]
fn a_text_file_keeps_the_formatting_and_loses_the_view_modes() {
    // The two questions are asked separately. A `.txt` file is prose, so the formatting is worth
    // offering; it is not Markdown, so there is nothing to preview.
    let mut harness = harness("");
    harness.get_by_label("notes.txt").click();
    harness.run();
    harness.get_by_label("Text options");
    for mode in ["Raw Markdown", "Side by side", "Markdown preview"] {
        assert!(
            harness.query_by_label(mode).is_none(),
            "{mode} has nothing to show for a .txt file"
        );
    }
}

/// Reproduce the design as closely as the application can, so that this image and `design/intial-design-screenshot.png` can be
/// put side by side.
///
/// It uses the project's own `sample` folder rather than a folder built in a temporary directory, because
/// the design shows that folder's files: `chapters` holding `one.md` and `two.md`, `notes` holding
/// `todo.txt`, and `welcome.md` open in the editor.
#[test]
fn the_window_matches_the_design() {
    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the workspace root is two levels above the crate")
        .join("sample");
    assert!(sample.join("welcome.md").is_file(), "{} should hold welcome.md", sample.display());
    // Copied out of the repository first. `sample/` is inside Unluminous's own git repository, and the
    // status bar now says which branch is checked out and how many files have changed, so opening it
    // where it lies made the picture depend on what happened to be edited that day.
    let sample = copy_out_of_the_repository(&sample, "unluminous-screenshot-sample");

    let folder = sample.clone();
    let mut harness = builder().with_size(vec2(1264.0, 751.0)).build_eframe(move |cc| {
        let mut app = UnluminousApp::new(folder);
        app.prepare(&cc.egui_ctx);
        // A plugin's decoration is rasterised on the processor, and `vello_cpu` picks the widest SIMD it
        // has. Pinned here so an accepted image is a property of the code rather than of the machine that
        // took it — the same reason the terminal's screenshots feed fixed bytes to a session with no shell.
        app.draw_deterministically();
        app
    });
    harness.run();
    // Open the two folders and the file, through the explorer, as a person would.
    harness.get_by_label_contains("chapters").click();
    harness.run();
    harness.get_by_label_contains("notes").click();
    harness.run();
    harness.get_by_label_contains("welcome.md").click();
    harness.run();

    assert!(
        harness.state().document().text().to_string().starts_with("# Unluminous"),
        "welcome.md should be open in the editor"
    );
    assert_eq!(
        harness.state().tree.file_count(),
        5,
        "four Markdown or text files plus one Rust file"
    );
    assert_eq!(harness.state().tree.openable_count(), 5, "all five hold text, so all five open");
    assert_eq!(
        harness.state().caret_position(),
        unluminous_app::components::status_bar::Position { line: 1, column: 1 }
    );
    harness.snapshot(shot("design_comparison"));
}

#[test]
fn a_new_window_starts_on_the_raw_markdown() {
    let harness = harness("# heading");
    assert_eq!(harness.state().view_mode(), ViewMode::Raw);
    assert!(harness.state().view_mode().shows_source());
    assert!(!harness.state().view_mode().shows_preview());
}

#[test]
fn the_three_view_mode_buttons_are_reachable_by_name_and_switch_between_the_modes() {
    let mut harness = harness(MARKDOWN);
    for (name, expected) in [
        ("Side by side", ViewMode::SideBySide),
        ("Markdown preview", ViewMode::Preview),
        ("Raw Markdown", ViewMode::Raw),
    ] {
        harness.get_by_label(name).click();
        harness.run();
        assert_eq!(harness.state().view_mode(), expected, "clicking {name} should switch to it");
    }
}

#[test]
fn raw_markdown_shows_the_source_as_it_is_on_disk() {
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Raw Markdown").click();
    harness.run();
    // The editing area holds the source, marks and all.
    assert!(harness.state().document().text().to_string().contains("**bold**"));
    assert_eq!(harness.state().editor_area().width(), harness.state().editor_area().width());
    harness.snapshot(shot("view_raw"));
}

#[test]
fn the_preview_removes_the_marks_and_applies_them() {
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let preview = harness.state().preview_text();
    assert!(!preview.contains("**bold**"), "the marks are not shown, got {preview:?}");
    assert!(preview.contains("bold"), "the words are");
    assert!(!preview.contains("# Unluminous preview"), "the heading loses its hash");
    assert!(preview.contains("Unluminous preview"));
    assert!(preview.contains('\u{2022}'), "a bullet list gets bullets");
    assert!(preview.contains("the design"), "a link shows its text");
    assert!(!preview.contains("https://example.com/design"), "and hides its address");
    assert!(preview.contains("code keeps its spacing"), "a code block keeps its lines");
    // The source itself is untouched: the preview is worked out from it, not instead of it.
    assert!(harness.state().document().text().to_string().contains("**bold**"));
    harness.snapshot(shot("view_preview"));
}

/// The three things `task-1685` added, in a document short enough that all of them are on the
/// screen at once. `MARKDOWN` is the everything document and is taller than the window.
const MARKDOWN_TABLE: &str = "\
## What the crates hold

| Crate | Lines | Tests |
| ----- | ----: | :---: |
| core | 9132 | 412 |
| terminal | 3004 | 88 |
| app | 17133 | 507 |

Some prose with `inline code` in it, under the table.

```rust
fn main() {
    let greeting = \"hello\";
    println!(\"{greeting}\");
}
```

- [x] a box that is ticked
- [ ] one that is not";

/// **A pipe table is drawn as a table**, which is the first thing `task-1685` asks for.
///
/// The pipes become a box of rules, the columns line up because the whole table is set in the code
/// font, and the words that were in the cells are still there to be read and copied.
#[test]
fn a_table_in_the_preview_is_drawn_in_a_box() {
    let mut harness = harness(MARKDOWN_TABLE);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let preview = harness.state().preview_text();
    assert!(!preview.contains('|'), "the pipes are the box, not the text: {preview}");
    assert!(preview.contains('\u{250C}'), "a top left corner: {preview}");
    assert!(preview.contains("Crate") && preview.contains("9132"), "the cells survive");
    // Every line of the table is the same width, which is what a table means.
    let rows: Vec<&str> = preview
        .lines()
        .filter(|line| line.starts_with('\u{2502}') && line.ends_with('\u{2502}'))
        .collect();
    assert!(rows.len() >= 3, "a head and two rows: {rows:?}");
    assert!(
        rows.windows(2).all(|pair| pair[0].chars().count() == pair[1].chars().count()),
        "{rows:?}"
    );
    harness.snapshot(shot("preview_table"));
}

/// **A tick box is a tick box**, and a quote inside a quote is two bars deep.
#[test]
fn the_preview_reads_the_things_the_old_parser_could_not() {
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let preview = harness.state().preview_text();
    assert!(preview.contains("\u{2611}  a box that is ticked"), "{preview}");
    assert!(preview.contains("\u{2610}  one that is not"), "{preview}");
    assert!(preview.contains("\u{2502}  \u{2502}  and one quoted inside it"), "{preview}");
    assert!(
        preview.contains("wrapped over two lines"),
        "a hand-wrapped paragraph is one paragraph: {preview}"
    );
}

/// **A fence names a language and the plugin that reads it colours the code.**
///
/// The same two calls `colour_the_file` makes for a `.rs` file, reached through the
/// `CodeHighlighter` seam, so a fence of Rust in a document looks like a Rust file.
#[test]
fn a_fence_of_rust_is_coloured_by_the_plugin_that_reads_rust() {
    let mut harness = harness(MARKDOWN_TABLE);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let preview = harness.state().preview_text();
    let at = preview.find("fn main").expect("the fence is in the preview");
    let keyword = harness.state().preview_style_at(at + 1);
    let name = harness.state().preview_style_at(at + 4);
    assert_ne!(
        keyword.color, name.color,
        "`fn` and `main` are different kinds of thing, so they are different colours"
    );
}

/// **A code block asks for a panel behind it**, which is the whole of "code blocks aren't easy to
/// read": a fence with no ground under it does not read as a block.
#[test]
fn the_preview_puts_code_on_a_panel_and_inline_code_on_a_chip() {
    let mut harness = harness(MARKDOWN_TABLE);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let panels = harness.state().preview_panels();
    assert!(panels.iter().any(|panel| panel.kind == unluminous_core::PanelKind::Code));
    assert!(panels.iter().any(|panel| panel.kind == unluminous_core::PanelKind::Table));
    assert!(!harness.state().preview_code_spans().is_empty(), "`inline code` gets a chip");
}

/// **Text in the preview can be selected with the pointer and copied**, which is the ticket's
/// second complaint. The preview is read only; reading includes taking a copy of what you read.
#[test]
fn text_in_the_preview_can_be_selected_by_dragging() {
    let mut harness = harness(MARKDOWN_TABLE);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let area = harness.state().editor_area();
    // The line of prose under the table, which is what a person would drag across.
    let line = area.top() + 303.0;
    drag(
        &mut harness,
        egui::Pos2::new(area.left() + 24.0, line),
        egui::Pos2::new(area.left() + 340.0, line),
    );
    let selected = harness.state().preview_selected_text().unwrap_or_default();
    assert!(!selected.is_empty(), "a drag across a line should have selected something");
    assert!(
        harness.state().preview_holds_the_selection(),
        "and the copy should be about the preview rather than the source"
    );
    harness.snapshot(shot("preview_selection"));
}

/// **And selecting all of it works from the menu**, so a whole page can be taken in one go.
#[test]
fn the_whole_preview_can_be_selected_and_copied() {
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let area = harness.state().editor_area();
    let at = egui::Pos2::new(area.left() + 30.0, area.top() + 60.0);
    click_at(&mut harness, at);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::SelectAll, &ctx);
    harness.run();
    let selected = harness.state().preview_selected_text().unwrap_or_default();
    assert!(selected.contains("Unluminous preview"), "the heading is in it");
    assert!(selected.contains("The last paragraph."), "and so is the last line");
}

/// **`Ctrl/Cmd+C` copies the preview**, and it has to be claimed before the source pane is drawn or
/// the source would take the event and copy its own selection instead. egui delivers a copy as an
/// `Event::Copy` rather than as a key press, which is why this is not simply the `Copy` menu entry.
#[test]
fn the_copy_key_in_the_preview_copies_the_preview_and_not_the_source() {
    let mut harness = harness(MARKDOWN_TABLE);
    harness.get_by_label("Side by side").click();
    harness.run();
    let source = harness.state().editor_area();
    // Something selected in the source, so a copy that went to the wrong half would be visible.
    harness.state_mut().command(Command::SelectAll);
    harness.run();
    click_at(&mut harness, egui::Pos2::new(source.right() + 60.0, source.top() + 60.0));
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::SelectAll, &ctx);
    harness.run();

    harness.input_mut().events.push(egui::Event::Copy);
    harness.step();
    let copied = harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(copied.contains("What the crates hold"), "the preview was copied, got {copied:?}");
    assert!(!copied.contains("| ----- |"), "and not the source, got {copied:?}");
}

/// **A click in the source takes the copy back**, so the two halves of the side-by-side view never
/// argue about what `Copy` means.
#[test]
fn a_click_in_the_source_takes_the_copy_back_from_the_preview() {
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Side by side").click();
    harness.run();
    let source = harness.state().editor_area();
    let preview = egui::Pos2::new(source.right() + 60.0, source.top() + 60.0);
    click_at(&mut harness, preview);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::SelectAll, &ctx);
    harness.run();
    assert!(harness.state().preview_holds_the_selection());
    click_at(&mut harness, egui::Pos2::new(source.left() + 30.0, source.top() + 30.0));
    assert!(
        !harness.state().preview_holds_the_selection(),
        "pressing in the source is what says the copy is about the source"
    );
}

#[test]
fn the_preview_lays_out_headings_taller_than_body_text() {
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let layout = harness.state().preview_layout();
    assert!(!layout.lines.is_empty(), "the preview should have been laid out");
    let heading = layout.lines[0].height;
    let body = layout
        .lines
        .iter()
        .find(|line| line.runs.iter().any(|run| line.run_clusters(run).len() > 20))
        .map(|line| line.height)
        .expect("one line of body text");
    assert!(
        heading > body,
        "the heading line ({heading}) should be taller than a body line ({body})"
    );
}

#[test]
fn side_by_side_shows_the_source_and_the_preview_at_once() {
    let mut harness = harness(MARKDOWN);
    let full_width = harness.state().editor_area().width();
    harness.get_by_label("Side by side").click();
    harness.run();
    let half = harness.state().editor_area().width();
    assert!(
        half < full_width,
        "the editing area should give up half its width to the preview: {full_width} then {half}"
    );
    assert!(!harness.state().preview_text().is_empty(), "the preview should have been worked out");
    harness.snapshot(shot("view_side_by_side"));
}

#[test]
fn the_preview_follows_the_source_as_it_is_edited() {
    let mut harness = harness("# first");
    harness.get_by_label("Side by side").click();
    harness.run();
    assert!(harness.state().preview_text().contains("first"));
    harness.state_mut().command(Command::MoveDocumentEnd { extend: false });
    harness.input_mut().events.push(egui::Event::Text(" and second".to_owned()));
    harness.run();
    assert!(
        harness.state().preview_text().contains("first and second"),
        "the preview should have been worked out again, got {:?}",
        harness.state().preview_text()
    );
}

#[test]
fn the_preview_cannot_be_typed_into() {
    let mut harness = harness("# heading");
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let before = harness.state().document().text().to_string();
    // Typing with only the preview showing must not reach the document.
    harness.input_mut().events.push(egui::Event::Text("XXX".to_owned()));
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "the preview is read only, so nothing should have been inserted"
    );
}

// The File menu.

/// The menus, drawn inside the window.
///
/// On macOS Unluminous puts them in the bar along the top of the screen instead, which egui cannot draw and this
/// harness cannot see, so the test asks for the bar inside the window. That is not a special case for the
/// test: it is what Windows uses, and both bars are built from the same list of menus.
#[test]
fn the_file_menu_holds_new_window_open_save_and_recent_projects() {
    let mut harness = harness("");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.state_mut().recent = vec![
        std::path::PathBuf::from("/tmp/unluminous-recent-one"),
        std::path::PathBuf::from("/tmp/unluminous-recent-two"),
    ];
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    for entry in ["New Window", "Open File", "Open Folder", "Save", "Save As", "Close Window"] {
        harness.get_by_label(entry);
    }
    // The recent projects are listed under a heading of their own inside the File menu.
    harness.get_by_label("unluminous-recent-one");
    harness.get_by_label("unluminous-recent-two");
    harness.get_by_label("Forget Recent Projects");
    harness.snapshot(shot("file_menu"));
}

#[test]
fn opening_a_folder_shows_it_in_the_explorer() {
    let mut harness = harness("");
    // The picker itself is the operating system's, so the action is run directly rather than clicking
    // through a modal dialog that a test cannot answer.
    let other = std::env::temp_dir().join("unluminous-other-folder");
    std::fs::create_dir_all(other.join("inner")).expect("make the other folder");
    std::fs::write(other.join("alpha.md"), "# alpha\n").expect("write alpha.md");
    std::fs::write(other.join("inner/beta.txt"), "beta\n").expect("write inner/beta.txt");

    assert_ne!(harness.state().tree.root(), other.as_path());
    harness.state_mut().open_folder(&other);
    harness.run();

    assert_eq!(harness.state().tree.root(), other.as_path());
    let names: Vec<String> =
        harness.state().tree.rows().iter().map(|row| row.entry.name.clone()).collect();
    assert_eq!(names, vec!["inner", "alpha.md"], "the new folder's contents are listed");
    assert_eq!(harness.state().tree.file_count(), 2, "including the file in the sub folder");
    harness.snapshot(shot("opened_folder"));
    std::fs::remove_dir_all(&other).ok();
}

/// `task-28`: expanding a folder with a lot of files in it froze the window until it was force quit.
///
/// Two of the three causes were the read, and `services::file_tree` and `services::file_kind` have their own
/// tests for those. This is the third: every row in the tree was drawn every frame, and each row allocates a
/// rectangle and interacts, so a folder of thousands of files was thousands of widgets a frame after the read
/// had finished.
///
/// What is asserted is that a row far below the visible band is **not in the widget tree**, and that
/// scrolling to it puts it there. A count of widgets would be the more direct measurement and it is not
/// available: the accessibility tree holds the whole window, so the number would move whenever anything else
/// in the window gained a control.
#[test]
fn the_explorer_draws_only_the_rows_that_are_on_screen() {
    let mut harness = harness("");
    let folder = std::env::temp_dir().join(format!("unluminous-many-rows-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("make the folder");
    // Named so they sort in the order they are numbered, because the explorer sorts by name and
    // `entry100` sorts before `entry2`.
    for index in 0..600 {
        std::fs::write(folder.join(format!("entry{index:04}.txt")), "text\n")
            .expect("write a file");
    }
    harness.state_mut().open_folder(&folder);
    harness.run();
    assert_eq!(harness.state().tree.rows().len(), 600, "every file is in the tree");

    // The first row is drawn and a row hundreds below it is not, which is the whole of the change.
    assert!(harness.query_by_label("entry0000.txt").is_some(), "the first row is on screen");
    assert!(
        harness.query_by_label("entry0599.txt").is_none(),
        "the last row is hundreds of rows below the panel, so it is not drawn"
    );

    // And it is reachable: the window's own reveal is what a person or `unluminous-cli` asks for, and it has to
    // work for a row that was never drawn, which is the one thing virtualisation could have taken away.
    let last = folder.join("entry0599.txt");
    harness.state_mut().open_path_permanently(&last).expect("the file opens");
    harness.run();
    harness.run();
    assert!(
        harness.query_by_label("entry0599.txt").is_some(),
        "revealing a row scrolls to it and draws it, whether or not it was on screen before"
    );
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn opening_a_folder_clears_the_filter_and_brings_the_explorer_back() {
    let mut harness = harness("");
    harness.state_mut().filter = "two".to_owned();
    harness.state_mut().explorer_visible = false;
    harness.run();
    let folder = sample_folder();
    harness.state_mut().open_folder(&folder);
    harness.run();
    assert!(
        harness.state().filter.is_empty(),
        "a filter from the old folder should not carry over"
    );
    assert!(harness.state().explorer_visible, "opening a folder shows the explorer");
}

#[test]
fn a_file_that_is_not_text_is_listed_and_does_nothing_when_clicked() {
    let mut harness = harness("");
    let before = harness.state().document().text().to_string();
    harness.get_by_label_contains("bundle.zip").click();
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "clicking a file that is not text should do nothing"
    );
    harness.snapshot(shot("unopenable_file"));
}

/// The file types improvement: a file Unluminous has no special handling for opens as plain text.
#[test]
fn a_rust_file_opens_as_plain_text() {
    let mut harness = harness("");
    harness.get_by_label_contains("program.rs").click();
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        "fn main() {}\n",
        "the Rust file should have been loaded"
    );
    assert_eq!(harness.state().view_mode(), ViewMode::Raw, "there is nothing to preview in it");
    assert!(
        !harness.state().layout().lines.is_empty(),
        "the Rust file's text should have been laid out"
    );
    harness.snapshot(shot("plain_text_file"));
}

#[test]
fn save_as_and_save_are_reachable_without_the_menu() {
    // `Save` on a document that has never been saved writes into the folder the explorer is showing, which
    // is the behaviour the status bar reports. This checks the action rather than the dialog.
    let folder = std::env::temp_dir().join("unluminous-save-action");
    std::fs::create_dir_all(&folder).expect("make the folder");
    let text = "saved through the File menu";
    let owned = folder.clone();
    let mut harness = builder().with_size(vec2(WINDOW[0], WINDOW[1])).build_eframe(move |cc| {
        let mut app = UnluminousApp::with_text(owned, text);
        app.prepare(&cc.egui_ctx);
        // A plugin's decoration is rasterised on the processor, and `vello_cpu` picks the widest SIMD it
        // has. Pinned here so an accepted image is a property of the code rather than of the machine that
        // took it — the same reason the terminal's screenshots feed fixed bytes to a session with no shell.
        app.draw_deterministically();
        app
    });
    harness.run();
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Save, &ctx);
    harness.run();
    let written = folder.join("untitled.md");
    assert!(written.is_file(), "Save should have written {}", written.display());
    assert_eq!(std::fs::read_to_string(&written).expect("read it back"), text);
    std::fs::remove_dir_all(&folder).ok();
}

/// The version and the build date this build really has would put a different picture in front of
/// the comparison every time the binary was rebuilt, so the test fixes them. `About::current` is
/// covered by a unit test beside the component.
fn a_fixed_about() -> About {
    About {
        developer: "Jason McAffee".to_owned(),
        version: "0.2.0".to_owned(),
        built: "2026-08-25 10:45pm".to_owned(),
        // Nothing has been asked, which is what a fresh window has. `task-1804` §6.
        update: None,
    }
}

#[test]
fn the_about_box_names_the_developer_the_version_and_the_build_date() {
    let mut harness = harness("Text behind the About box.");
    assert!(harness.state().about.is_none());
    open_about(&mut harness);
    assert!(harness.state().about.is_some(), "Unluminous then About Unluminous should open it");

    harness.state_mut().about = Some(a_fixed_about());
    harness.run();
    harness.get_by_label("Developed by Jason McAffee");
    harness.get_by_label("Version: 0.2.0");
    harness.get_by_label("Build Date: 2026-08-25 10:45pm");
    harness.snapshot(shot("about"));
}

#[test]
fn the_about_box_closes_on_its_button() {
    let mut harness = harness("");
    open_about(&mut harness);
    harness.get_by_label("Done").click();
    harness.run();
    assert!(harness.state().about.is_none());
}

#[test]
fn the_about_box_closes_on_escape() {
    let mut harness = harness("");
    open_about(&mut harness);
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.state().about.is_none());
}

#[test]
fn opening_the_about_box_shuts_whatever_else_was_open() {
    // One modal at a time is the rule every other entry point already keeps, and the About box is
    // reached from a menu rather than from `modal open`, which is where it would be easy to forget.
    let mut harness = harness("");
    open_settings(&mut harness);
    assert!(harness.state().settings_window.open);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::About, &ctx);
    harness.run();
    assert!(harness.state().about.is_some());
    assert!(!harness.state().settings_window.open, "the Settings window went away");
}

/// The About box offers a check, and Unluminous asks for nothing until it is pressed.
///
/// `task-1804` §6. **The assertion that matters is the second one**: a test can watch a button
/// appear, and what this feature has to be right about is that opening the window, opening the
/// About box and reading it send nothing at all. `update.check` is off in a fresh Unluminous, and
/// `UnluminousApp::update` is `None` until somebody asks -- so `None` is the evidence.
#[test]
fn the_about_box_offers_a_check_and_nothing_is_asked_until_it_is_pressed() {
    let mut harness = harness("");
    assert!(
        !harness.state().settings.update_check.at_start(),
        "a fresh Unluminous does not ask the releases page anything"
    );
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::About, &ctx);
    harness.run();
    assert!(harness.state().about.is_some());
    // The version and the build date this build really has would put a different picture in front
    // of the comparison every time the binary was rebuilt, which is what `a_fixed_about` is for.
    harness.state_mut().about = Some(a_fixed_about());
    harness.run();
    harness.get_by_label("Check for Updates");
    // Nothing has been asked, so the box says nothing about updates.
    assert_eq!(harness.state().update_line(), None, "opening the box asked nothing");
    harness.snapshot(shot("about_updates"));
}

/// The setting is what makes the window ask as it opens, and it is off unless it is set.
#[test]
fn the_update_setting_is_read_and_written_and_is_off_until_it_is_set() {
    let mut harness = harness("");
    assert_eq!(did(&mut harness, "settings get update.check")["value"], serde_json::json!("off"));
    did(&mut harness, "settings set update.check start");
    assert!(harness.state().settings.update_check.at_start());
    assert_eq!(did(&mut harness, "settings get update.check")["value"], serde_json::json!("start"));
    // A value this version has not got is refused with what it does take, rather than being taken
    // as "off" -- which would be a settings file that quietly stopped meaning what it said.
    assert_eq!(refused(&mut harness, "settings set update.check weekly"), "usage");
    did(&mut harness, "settings set update.check off");
    assert!(!harness.state().settings.update_check.at_start());
}

// task-1804 §4.2: the two newest plugins are configurable by an agent as well as by a person.

/// **The gap the ticket measured, closed.** `settings list` named 23 keys and not one belonged to
/// the Agent-Chat or the Database plugin, so a person could configure both from a Settings page and
/// an agent could configure neither.
///
/// It is asserted through `settings` rather than through a `chat` and a `database` area, because the
/// fault was general: every `ui` plugin keeps its configuration in its own `settings.conf`, and a
/// pair of areas would have answered for two of them and left the next one exactly where these were.
#[test]
fn a_plugins_own_configuration_is_listed_read_and_written_through_settings() {
    let store = scratch_folder("plugin-settings-store");
    let mut harness = harness("");
    harness.state_mut().use_store(unluminous_app::services::store::Store::at(&store));
    harness.run();

    // Nothing yet: a plugin nobody has configured has no file, and what is listed is what is in one.
    // A key made up here would be a default this window claimed on the plugin's behalf.
    let named = |harness: &mut Harness<'static, UnluminousApp>| -> Vec<String> {
        did(harness, "settings list")["lines"]
            .as_array()
            .expect("the rows")
            .iter()
            .map(|row| {
                row.as_str()
                    .unwrap_or_default()
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    };
    assert!(
        !named(&mut harness).iter().any(|name| name.starts_with("plugins.agent-chat.")),
        "a plugin with no file names nothing"
    );

    // A chat row's program, which is one of the four things §4.2 says a person can set and an
    // agent could not.
    did(&mut harness, "settings set plugins.agent-chat.provider.0.program claude");
    assert_eq!(
        did(&mut harness, "settings get plugins.agent-chat.provider.0.program")["value"],
        serde_json::json!("claude")
    );
    assert!(
        named(&mut harness).iter().any(|name| name == "plugins.agent-chat.provider.0.program"),
        "and now it is listed"
    );

    // And a data source, which is the other half of the same complaint.
    did(&mut harness, "settings set plugins.database.source.0.engine sqlite");
    assert_eq!(
        did(&mut harness, "settings get plugins.database.source.0.engine")["value"],
        serde_json::json!("sqlite")
    );

    // Written into the **plugin's** own file, not the window's, and merged rather than replacing it.
    let file = store.join("plugins").join("agent-chat").join("settings.conf");
    let text = std::fs::read_to_string(&file).expect("the plugin's own file");
    assert!(text.contains("provider.0.program"), "{text}");
    did(&mut harness, "settings set plugins.agent-chat.tools true");
    let text = std::fs::read_to_string(&file).expect("still there");
    assert!(text.contains("provider.0.program"), "the first key survived the second: {text}");
    assert!(text.contains("tools"), "{text}");

    // An empty value takes the key out, so a setting goes back to the plugin's own default rather
    // than being pinned to nothing.
    did(&mut harness, "settings set plugins.agent-chat.tools \"\"");
    assert!(
        !named(&mut harness).iter().any(|name| name == "plugins.agent-chat.tools"),
        "cleared, so the plugin's own default is what applies"
    );

    // A plugin that is not installed is a setting that is not there, rather than a file written into
    // a folder nobody asked for.
    assert_eq!(refused(&mut harness, "settings get plugins.nothing.at-all"), "not-found");
    assert_eq!(refused(&mut harness, "settings set plugins.nothing.at-all yes"), "not-found");
}

#[test]
fn the_settings_window_opens_from_the_edit_menu_and_holds_the_font_and_the_background() {
    let mut harness = harness("Text behind the settings window.");
    assert!(!harness.state().settings_window.open);
    open_settings(&mut harness);
    assert!(harness.state().settings_window.open, "Edit then Settings should open it");

    // The two sections `tasks/improvements.md` asks for, and the page list on the left.
    harness.get_by_label("Editor font family");
    harness.get_by_label("Editor font size");
    harness.get_by_label("Background opacity");
    harness.get_by_label("Appearance");
    harness.get_by_label("Terminal");
    harness.snapshot(shot("settings_appearance"));
}

#[test]
fn the_settings_window_closes_on_the_close_button() {
    let mut harness = harness("");
    open_settings(&mut harness);
    harness.get_by_label("Done").click();
    harness.run();
    assert!(!harness.state().settings_window.open);
}

#[test]
fn the_terminal_page_holds_the_font_size_and_the_shell() {
    let mut harness = harness("");
    open_settings(&mut harness);
    harness.get_by_label("Terminal").click();
    harness.run();
    harness.get_by_label("Terminal font size");
    // `task-1670`: the shell is a setting because the default cannot be right for everybody, and this
    // is where a person who wants `cmd.exe` back asks for it.
    harness.get_by_label("Terminal shell");
    assert!(
        harness.query_by_label("Background opacity").is_none(),
        "that is on the Appearance page"
    );
    harness.snapshot(shot("settings_terminal"));
}

#[test]
fn the_mcp_page_holds_the_install_buttons_the_server_and_the_configuration_to_copy() {
    // Two things are pinned before the page is drawn, and both are what make a picture of it the
    // same on every machine. `UNLUMINOUS_HOME` is where the installers look for an agent's own
    // configuration, so a folder of its own is what makes the buttons read `Install for ...`
    // whatever this machine happens to have; `UNLUMINOUS_CLI_BIN` is the path written into the
    // configuration blocks, which would otherwise be wherever this checkout is.
    let home = std::env::temp_dir().join("unluminous-mcp-page-home");
    std::fs::remove_dir_all(&home).ok();
    std::fs::create_dir_all(&home).expect("make the folder");
    std::env::set_var("UNLUMINOUS_HOME", &home);
    std::env::set_var("UNLUMINOUS_CLI_BIN", r"C:\Program Files\Unluminous\unluminous-cli.exe");

    let mut harness = harness("");
    open_settings(&mut harness);
    harness.get_by_label("MCP").click();
    harness.run();

    // `task-1679` asks for all four, and this is where a person finds each of them.
    harness.get_by_label("Install for Claude Code");
    harness.get_by_label("Install for Codex");
    harness.get_by_label("MCP port");
    harness.get_by_label("MCP tool shape");
    harness.get_by_label("Claude Code configuration");
    // One block at a time, and the other client's is a click away.
    harness.get_by_label("Codex").click();
    harness.run();
    harness.get_by_label("Codex configuration");
    harness.get_by_label("Claude Code").click();
    harness.run();
    harness.get_by_label("Copy");
    assert!(
        !harness.state().settings.mcp_enabled,
        "the HTTP endpoint is off until somebody turns it on"
    );
    harness.snapshot(shot("settings_mcp"));

    std::env::remove_var("UNLUMINOUS_HOME");
    std::env::remove_var("UNLUMINOUS_CLI_BIN");
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn ticking_the_mcp_box_is_a_setting_and_not_a_listener_in_a_test() {
    // A window a test builds never opens a command channel, so it never opens an MCP endpoint
    // either — the rule `open_control_channel` already keeps, for the same reason: a test must not
    // open a port or leave a listener behind when it ends. What the tick box does here is change
    // the setting, which is what is checked.
    let home = std::env::temp_dir().join("unluminous-mcp-tick-home");
    std::fs::remove_dir_all(&home).ok();
    std::fs::create_dir_all(&home).expect("make the folder");
    std::env::set_var("UNLUMINOUS_HOME", &home);

    let mut harness = harness("");
    open_settings(&mut harness);
    harness.get_by_label("MCP").click();
    harness.run();
    harness.get_by_label("Also serve over HTTP on this machine").click();
    harness.run();
    assert!(harness.state().settings.mcp_enabled, "the tick box should have set it");
    assert!(!harness.state().is_serving_mcp(), "a test window must not have opened a port");

    std::env::remove_var("UNLUMINOUS_HOME");
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn the_shell_typed_into_the_settings_is_what_a_new_terminal_runs() {
    let mut harness = harness("");
    open_settings(&mut harness);
    harness.get_by_label("Terminal").click();
    harness.run();
    harness.get_by_label("Terminal shell").click();
    harness.run();
    harness.get_by_label("Terminal shell").type_text("/no/such/program/at/all");
    harness.run();
    assert_eq!(harness.state().settings.terminal_shell, "/no/such/program/at/all");

    // Which is what a terminal opened afterwards tries to run — it cannot start, and the tile says so
    // in the shell's own words, which is how the setting is seen to have reached the tab at all.
    harness.state_mut().terminal.visible = true;
    harness.state_mut().new_terminal_tab();
    harness.run();
    let reason = harness.state().terminal.tabs.last_error.clone().expect("a reason");
    assert!(reason.contains("/no/such/program/at/all"), "it said {reason:?}");
}

#[test]
fn a_tab_can_be_dragged_along_the_strip_to_rearrange_it() {
    let mut harness = harness("");
    let folder = sample_folder();
    for name in ["notes.txt", "readme.md", "program.rs"] {
        harness.state_mut().open_path_permanently(&folder.join(name)).expect("the file opens");
        harness.run();
    }
    let names = |harness: &Harness<'static, UnluminousApp>| -> Vec<String> {
        harness.state().files.iter().map(|file| file.name()).collect()
    };
    assert_eq!(names(&harness), vec!["notes.txt", "readme.md", "program.rs"]);
    let first = harness.get_by_label("Tab: notes.txt").rect();
    let last = harness.get_by_label("Tab: program.rs").rect();
    drag(&mut harness, first.center(), egui::pos2(last.right() - 4.0, last.center().y));
    assert_eq!(
        names(&harness),
        vec!["readme.md", "program.rs", "notes.txt"],
        "the tab should have been carried to the end of the strip"
    );
    assert_eq!(
        harness.state().files.active().name(),
        "notes.txt",
        "and be showing where it landed"
    );
    harness.snapshot(shot("tab_dragged_along_the_strip"));
}

#[test]
fn a_tab_can_be_dragged_from_one_pane_into_the_other() {
    let mut harness = harness("");
    let folder = sample_folder();
    for name in ["notes.txt", "readme.md"] {
        harness.state_mut().open_path_permanently(&folder.join(name)).expect("the file opens");
        harness.run();
    }
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::SplitRight, &ctx);
    harness.run();
    assert_eq!(harness.state().files.pane_count(), 2);
    assert_eq!(harness.state().files.pane_of(0), 0, "notes.txt stays on the left");
    assert_eq!(harness.state().files.pane_of(1), 1, "readme.md went into the new pane");
    // Carry notes.txt out of the pane on the left and into the pane on the right.
    let tab = harness.get_by_label("Tab: notes.txt").rect();
    let target = harness.get_by_label("Tab: readme.md").rect();
    drag(&mut harness, tab.center(), egui::pos2(target.right() - 4.0, target.center().y));
    assert_eq!(
        harness.state().files.pane_count(),
        1,
        "the pane it was carried out of held nothing else, so it went with it"
    );
    let names: Vec<String> = harness.state().files.iter().map(|file| file.name()).collect();
    assert_eq!(names, vec!["readme.md", "notes.txt"]);
}

#[test]
fn right_clicking_the_project_name_opens_the_same_menu_a_folder_does() {
    let mut harness = harness("");
    let heading =
        harness.get_by_label(&sample_folder().file_name().unwrap().to_string_lossy()).rect();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: heading.center(),
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    harness.run();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: heading.center(),
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: Modifiers::default(),
    });
    harness.run();
    let opened = harness.state().explorer_menu.clone();
    let (_, path, directory, _) =
        opened.expect("the project's name should open the explorer's menu");
    assert_eq!(path, sample_folder(), "the menu is about the project folder");
    assert!(directory, "and it is a folder, so `New -> File` makes a file inside it");
    harness.run();
    harness.snapshot(shot("project_name_menu"));
}

// ------------------------------------------------------------------- task-1693

/// The gutter's marks line up with the letters at any size, which is what `task-1693` reported was
/// wrong: a mark centred in the line box sits low, by more the larger the type, because all of a
/// line's extra leading is added below its glyphs.
#[test]
fn the_gutter_lines_up_with_the_text_at_a_large_font_size() {
    let mut harness = harness("one\ntwo\nthree\nfour\nfive\n");
    collapse(&mut harness);
    did(&mut harness, "settings set appearance.font.size 34");
    harness.run();
    assert!(harness.state().settings.line_numbers);
    // The letters of a line fill much less than the line, which is where the drift came from. Asked
    // of the layout rather than of the picture, so it is a number a reader can check.
    let layout = harness.state().layout().clone();
    let line = &layout.lines[2];
    let band = line.ascent + line.descent;
    assert!(
        band < line.height - 4.0,
        "at 34 points a line is {} tall and its letters only {band}, which is the drift",
        line.height
    );
    harness.snapshot(shot("gutter_large_font"));
}

/// A right click in the empty space below the rows opens the project folder's menu, with everything
/// that is about a particular file dimmed rather than taken away — `task-1693`, in its own words.
#[test]
fn the_explorers_menu_opens_from_the_empty_space_below_the_rows() {
    let mut harness = harness("");
    // Well below the last row and well above the footer, which is where a person aims when they mean
    // "somewhere in this panel".
    let at = egui::pos2(150.0, 470.0);
    right_click_at(&mut harness, at);
    let opened = harness.state().explorer_menu.clone();
    let (_, path, directory, aimed) =
        opened.expect("the empty space should open the explorer's menu");
    assert_eq!(path, sample_folder(), "it is the project folder");
    assert!(directory);
    assert_eq!(aimed, unluminous_app::app::actions::Aim::AtEmptySpace);
    harness.run();
    // The entry somebody who right clicked the empty space came for. `File` is asked for by the
    // menu's own row rather than by name, because `File` is also a menu in the bar.
    harness.get_by_label("Folder");
    // And the ones that are about a particular file are dimmed rather than absent.
    let entries = unluminous_app::app::actions::explorer_menu(
        sample_folder().as_path(),
        true,
        false,
        unluminous_app::app::actions::Aim::AtEmptySpace,
    );
    let live = |name: &str| {
        entries.iter().any(|entry| {
            matches!(
                entry,
                unluminous_app::app::actions::Entry::Item { name: found, enabled, .. }
                    if found == name && *enabled
            )
        })
    };
    assert!(!live("Rename..."), "Rename is dimmed, because nothing was clicked");
    assert!(!live("Delete"), "and so is Delete");
    assert!(live("Reload from Disk"), "what is about the folder stays live");
    harness.get_by_label("Rename...");
    harness.snapshot(shot("explorer_empty_space_menu"));
}

/// The file that is showing and the row the explorer's cursor is on are two marks, not one. They
/// were drawn identically until `task-1693`, so a right click on a second file left two rows looking
/// equally open — and the second one stayed that way after the first tab was closed.
#[test]
fn the_open_file_and_the_explorers_cursor_are_drawn_differently() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open readme.md");
    did(&mut harness, "explorer select notes.txt");
    harness.run();
    assert_eq!(
        harness.state().files.active().path(),
        Some(folder.join("readme.md").as_path()),
        "readme is the file that is showing"
    );
    assert_eq!(harness.state().selected, Some(folder.join("notes.txt")));
    harness.snapshot(shot("explorer_open_and_cursor"));
}

/// A maximised window offers no resize grips. `components::resize_edges` records why that matters
/// more than tidiness: a resize the window manager refuses latches a flag inside winit that no later
/// move or resize can clear.
#[test]
fn a_maximised_window_offers_no_resize_grips() {
    let mut harness = harness("");
    for grip in ["top", "bottom", "left", "right"] {
        harness.get_by_label(&format!("Resize window: {grip}"));
    }
    let ids: Vec<egui::ViewportId> = harness.input().viewports.keys().copied().collect();
    for id in ids {
        if let Some(viewport) = harness.input_mut().viewports.get_mut(&id) {
            viewport.maximized = Some(true);
        }
    }
    harness.run();
    for grip in ["top", "bottom", "left", "right", "top left", "bottom right"] {
        assert!(
            harness.query_by_label(&format!("Resize window: {grip}")).is_none(),
            "a maximised window has no {grip} grip to offer"
        );
    }
}

/// `New -> Folder`, which the explorer had no way to do at all.
#[test]
fn the_explorer_can_make_a_folder() {
    let folder = std::env::temp_dir().join("unluminous-new-folder-test");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the project");
    std::fs::write(folder.join("readme.md"), "# here\n").expect("write a file");
    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::NewFolder(folder.clone()), &ctx);
    harness.run();
    let title = harness.state().prompt.clone().expect("a prompt asking for the name").title;
    assert_eq!(title, "New Folder");
    let mut prompt = harness.state().prompt.clone().expect("the prompt");
    prompt.value = "services".to_owned();
    harness.state_mut().run_prompt_for_test(prompt);
    harness.state_mut().prompt = None;
    harness.run();
    assert!(folder.join("services").is_dir(), "the folder was made");
    assert_eq!(harness.state().selected, Some(folder.join("services")));

    // And from the command line, which is the other half of every feature in Unluminous.
    did(&mut harness, "explorer new-folder deep/inside");
    assert!(folder.join("deep/inside").is_dir(), "the folders above it are made too");
    did(&mut harness, "explorer new-file deep/inside/note.md");
    assert!(folder.join("deep/inside/note.md").is_file());
}

/// A file another program makes appears in the explorer without anybody asking, which is what
/// `task-1693` reported was missing: an agent's new file was invisible until the tree was reloaded.
#[test]
fn a_file_made_by_another_program_appears_in_the_explorer() {
    let folder = std::env::temp_dir().join("unluminous-watch-test");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the project");
    std::fs::write(folder.join("readme.md"), "# here\n").expect("write a file");
    let mut harness = harness_in(&folder);
    assert!(harness.query_by_label("made-by-an-agent.md").is_none());

    // A second's wait, because a folder's modification time has whole-second resolution on some file
    // systems and the tree has only just read it.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(folder.join("made-by-an-agent.md"), "# new\n").expect("write the new file");
    // The window asks on a timer, so it takes a few frames rather than one.
    for _ in 0..8 {
        harness.step();
        if harness.query_by_label("made-by-an-agent.md").is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    harness.get_by_label("made-by-an-agent.md");
}

/// The plus beside the project's name is gone. It never made a file — it asked the window to save —
/// and `task-1673` asks for it to go.
#[test]
fn there_is_no_new_file_button_beside_the_project_name() {
    let harness = harness("");
    assert!(harness.query_by_label("New file").is_none());
    assert!(harness.query_by_label("Hide the explorer").is_some(), "the other button stays");
}

// Several windows, each on its own project.

#[test]
fn the_recent_projects_are_remembered_across_windows() {
    // The store is pointed at a folder of its own, so this neither reads nor writes the settings of the
    // person running the tests.
    let store_folder = std::env::temp_dir().join("unluminous-recent-projects-test");
    std::fs::remove_dir_all(&store_folder).ok();
    let first = std::env::temp_dir().join("unluminous-project-one");
    let second = std::env::temp_dir().join("unluminous-project-two");
    std::fs::create_dir_all(&first).expect("make the first project");
    std::fs::create_dir_all(&second).expect("make the second project");

    let mut harness = harness("");
    harness.state_mut().use_store(unluminous_app::services::store::Store::at(&store_folder));
    harness.state_mut().open_folder(&first);
    harness.state_mut().open_folder(&second);
    harness.run();

    let recent = harness.state().recent.clone();
    assert!(recent[0].ends_with("unluminous-project-two"), "the newest is first, got {recent:?}");
    assert!(recent.iter().any(|path| path.ends_with("unluminous-project-one")));

    // A second window reads the same list, which is what makes it a list of recent projects rather than of
    // this window's projects.
    let mut second_window = harness_in(&first);
    second_window.state_mut().use_store(unluminous_app::services::store::Store::at(&store_folder));
    second_window.run();
    assert!(
        second_window.state().recent.iter().any(|path| path.ends_with("unluminous-project-two")),
        "the other window's project should be in this window's list"
    );

    std::fs::remove_dir_all(&store_folder).ok();
    std::fs::remove_dir_all(&first).ok();
    std::fs::remove_dir_all(&second).ok();
}

#[test]
fn a_setting_is_written_and_read_back_by_the_next_window() {
    let store_folder = std::env::temp_dir().join("unluminous-settings-across-windows");
    std::fs::remove_dir_all(&store_folder).ok();

    let mut first_window = harness("");
    first_window.state_mut().use_store(unluminous_app::services::store::Store::at(&store_folder));
    let mut settings = first_window.state().settings.clone();
    settings.font_size = 32.0;
    settings.opacity = 0.5;
    first_window.state_mut().set_settings(settings);
    first_window.run();
    // Written once the pointer is up, which it is, so the next frame writes the file.
    first_window.run();

    let mut next = harness("");
    next.state_mut().use_store(unluminous_app::services::store::Store::at(&store_folder));
    next.run();
    assert_eq!(next.state().settings.font_size, 32.0, "the size should have come back");
    assert_eq!(next.state().settings.opacity, 0.5);
    std::fs::remove_dir_all(&store_folder).ok();
}

#[test]
fn a_box_that_takes_typing_keeps_the_keyboard_while_the_terminal_is_open() {
    // The explorer's filter box is clicked into while the terminal has the keyboard. Without this the
    // terminal would take every key press and the filter box could never be typed into.
    let mut harness = with_terminal("", 10, 60);
    assert_eq!(harness.state().focus, unluminous_app::app::Focus::Editor);
    harness.state_mut().focus = unluminous_app::app::Focus::Terminal;
    harness.run();

    harness.get_by_label("Filter files").click();
    harness.run();
    harness.get_by_label("Filter files").type_text("two");
    harness.run();
    harness.run();

    assert_eq!(harness.state().filter, "two", "what was typed should have reached the filter box");
}

/// Click at a point in the window, which is how the editing area is given the keyboard back.
fn click_at(harness: &mut Harness<'static, UnluminousApp>, at: egui::Pos2) {
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        });
    }
    harness.run();
}

// The next group is `task-1656`. The editing area used to read the frame's key and text events
// without asking whether another widget had the keyboard, and egui leaves the events a `TextEdit`
// consumed in that list, so typing `note` into the explorer's filter box put `note` at the caret in
// the open file as well and marked it as having unsaved changes. Each test drives a different box,
// because the fault was in the editing area rather than in any one of them.

#[test]
fn typing_in_the_explorers_filter_box_leaves_the_document_alone() {
    let mut harness = harness("");
    harness.get_by_label_contains("readme.md").click();
    harness.run();
    let before = harness.state().document().text().to_string();
    assert!(!harness.state().document().is_modified(), "just opened, so nothing to save");

    harness.get_by_label("Filter files").click();
    harness.run();
    harness.get_by_label("Filter files").type_text("note");
    harness.run();
    // A key press as well as text, because the two arrive as different events and the editing area
    // used to act on both: backspace deleted a character of the file rather than of the filter.
    harness.key_press(egui::Key::Backspace);
    harness.run();
    harness.run();

    assert_eq!(harness.state().filter, "not", "what was typed should have reached the filter box");
    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "and nothing should have reached the file behind it"
    );
    assert!(
        !harness.state().document().is_modified(),
        "so the file should not be marked as having unsaved changes"
    );
}

#[test]
fn clicking_back_into_the_document_takes_the_keyboard_back() {
    // The other half of the guard: it has to let go. Without this a filter that had been typed into
    // once would leave the document unable to be typed into at all.
    let mut harness = harness("");
    harness.get_by_label("Filter files").click();
    harness.run();
    harness.get_by_label("Filter files").type_text("two");
    harness.run();

    let middle = harness.state().editor_area().center();
    click_at(&mut harness, middle);
    harness.input_mut().events.push(egui::Event::Text("typed".to_owned()));
    harness.run();

    assert_eq!(harness.state().filter, "two", "the filter keeps what was typed into it");
    assert_eq!(
        harness.state().document().text().to_string(),
        "typed",
        "and the document takes typing again once it has been clicked into"
    );
}

#[test]
fn undo_in_a_text_box_does_not_undo_the_document() {
    // Control and Z used to clear the filter box and undo an edit in the file behind it with the one
    // press, because the menu's keyboard watcher reads the same events the box had just taken.
    let mut harness = harness("original");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.run();
    harness.input_mut().events.push(egui::Event::Text(" plus more".to_owned()));
    harness.run();
    let typed = harness.state().document().text().to_string();
    assert_ne!(typed, "original", "the document should have been typed into first");

    // Nothing is typed into the filter box first, deliberately. If it were, the undo would be
    // undoing the box's own insert and the assertion would hold whether or not the watcher had been
    // fixed. Focusing the box and pressing the shortcut is what tells the two apart.
    harness.get_by_label("Filter files").click();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Z);
    harness.run();

    assert_eq!(
        harness.state().document().text().to_string(),
        typed,
        "undo belongs to the box that has the keyboard, not to the document"
    );
}

#[test]
fn select_all_in_a_text_box_does_not_select_the_document() {
    let mut harness = harness("some writing to look at");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.run();
    harness.get_by_label("Filter files").click();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    harness.run();

    assert!(
        harness.state().document().selection().is_empty(),
        "select all should have selected the filter box, leaving the document alone"
    );
}

#[test]
fn the_rest_of_the_menu_still_works_while_a_text_box_has_the_keyboard() {
    // Only undo, redo and select all belong to the box. Everything else on every menu keeps working,
    // which is what stops the guard from being too broad: control and S in a search box saves the
    // file in every other editor and has to save it here.
    let folder = std::env::temp_dir().join("unluminous-save-while-filtering");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the folder");
    let text = "saved while the filter box had the keyboard";
    let owned = folder.clone();
    let mut harness = builder().with_size(vec2(WINDOW[0], WINDOW[1])).build_eframe(move |cc| {
        let mut app = UnluminousApp::with_text(owned, text);
        app.prepare(&cc.egui_ctx);
        // A plugin's decoration is rasterised on the processor, and `vello_cpu` picks the widest SIMD it
        // has. Pinned here so an accepted image is a property of the code rather than of the machine that
        // took it — the same reason the terminal's screenshots feed fixed bytes to a session with no shell.
        app.draw_deterministically();
        app
    });
    harness.run();
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.run();

    harness.get_by_label("Filter files").click();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::S);
    harness.run();

    let written = folder.join("untitled.md");
    assert!(written.is_file(), "Save should still have written {}", written.display());
    assert_eq!(std::fs::read_to_string(&written).expect("read it back"), text);
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn typing_a_new_name_in_the_rename_prompt_leaves_the_document_alone() {
    let folder = std::env::temp_dir().join("unluminous-rename-prompt-keyboard");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the folder");
    std::fs::write(folder.join("before.md"), "# before\n").expect("write it");
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("before.md")).expect("the file opens");
    harness.run();
    let before = harness.state().document().text().to_string();

    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::RenamePath(folder.join("before.md")), &ctx);
    harness.run();
    // The prompt asks for the keyboard as it opens, so this types without clicking, which is what a
    // person does.
    harness.get_by_label("Name").type_text("after");
    harness.run();

    assert!(
        harness.state().prompt.as_ref().is_some_and(|prompt| prompt.value.ends_with("after")),
        "what was typed should have reached the prompt: {:?}",
        harness.state().prompt.as_ref().map(|prompt| prompt.value.clone())
    );
    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "and nothing should have reached the file being renamed"
    );
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn typing_in_the_plugin_search_leaves_the_document_alone() {
    let mut harness = harness("");
    harness.get_by_label_contains("readme.md").click();
    harness.run();
    let before = harness.state().document().text().to_string();

    harness.state_mut().settings_window.open();
    harness.state_mut().settings_window.page = unluminous_app::settings::Page::Plugins;
    harness.run();
    harness.run();
    harness.get_by_label("Search plugins").click();
    harness.run();
    harness.get_by_label("Search plugins").type_text("rust");
    harness.run();
    harness.run();

    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "searching the plugins should not type into the file behind the settings window"
    );
    assert!(!harness.state().document().is_modified());
}

#[test]
fn enter_in_the_commit_message_is_a_new_line_and_the_command_key_commits() {
    // The one modal whose body owns Enter. Every other one is confirmed by it; here it has to stay
    // a new line, which is what `modal::Confirm::CommandEnter` is for.
    let mut harness = git_harness("commit-enter");
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .run_action(Action::Git(unluminous_app::app::actions::GitAction::Commit), &ctx);
    harness.run();
    settle(&mut harness, "the history the panel asks for", |app| {
        app.git.as_ref().is_some_and(|git| git.message.is_none() && !git.history.is_empty())
    });

    harness.get_by_label("Commit message").click();
    harness.run();
    harness.get_by_label("Commit message").type_text("the first line");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    harness.get_by_label("Commit message").type_text("the second");
    harness.run();
    harness.run();
    assert_eq!(
        harness.state().git.as_ref().map(|git| git.panel.message.clone()),
        Some("the first line\nthe second".to_owned()),
        "Enter in the message is a new line"
    );
    assert!(
        harness.state().git.as_ref().is_some_and(|git| git.panel.open),
        "and the panel is still open"
    );

    // The command key with it is what presses `COMMIT`, which is the reference editor's own chord for the same
    // dialog. Something has to be staged first, or the button is dimmed and there is nothing to
    // press.
    let root = harness.state().tree.root().to_path_buf();
    let ctx = harness.ctx.clone();
    harness.state_mut().open_path_permanently(&root.join("version.ts")).expect("the file opens");
    nudge(&mut harness);
    harness
        .state_mut()
        .run_action(Action::Git(unluminous_app::app::actions::GitAction::Add(None)), &ctx);
    settle(&mut harness, "the file to be staged", |app| {
        app.git.as_ref().is_some_and(|git| {
            git.snapshot
                .status
                .entry("version.ts")
                .is_some_and(unluminous_git::status::Entry::staged)
        })
    });
    // The panel is still open from above — `Git -> Commit` is a toggle, so asking for it again
    // would put it away.
    if let Some(state) = harness.state_mut().git.as_mut() {
        state.panel.message = "task-1682: committed from the keyboard".to_owned();
    }
    nudge(&mut harness);
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Enter);
    settle(&mut harness, "the commit", |app| {
        app.git.as_ref().is_some_and(|git| git.snapshot.status.entry("version.ts").is_none())
    });
    assert_eq!(
        ask_git(&root, &["log", "--format=%s", "-n1"]),
        "task-1682: committed from the keyboard",
    );
}

#[test]
fn typing_a_commit_message_leaves_the_document_alone() {
    // The worst of the boxes, because a commit message is a paragraph rather than a word: every
    // character of it used to be inserted into the file that was open behind the panel.
    let mut harness = git_harness("keyboard");
    let ctx = harness.ctx.clone();
    let before = harness.state().document().text().to_string();
    harness
        .state_mut()
        .run_action(Action::Git(unluminous_app::app::actions::GitAction::Commit), &ctx);
    harness.run();
    settle(&mut harness, "the history the panel asks for", |app| {
        app.git.as_ref().is_some_and(|git| git.message.is_none() && !git.history.is_empty())
    });

    harness.get_by_label("Commit message").click();
    harness.run();
    harness.get_by_label("Commit message").type_text("a message, not an edit");
    harness.run();
    harness.run();

    assert_eq!(
        harness.state().git.as_ref().map(|git| git.panel.message.clone()),
        Some("a message, not an edit".to_owned()),
        "what was typed should have reached the commit message"
    );
    assert_eq!(
        harness.state().document().text().to_string(),
        before,
        "and nothing should have reached the file behind the panel"
    );
    assert!(!harness.state().document().is_modified());
}

#[test]
fn the_recent_projects_menu_opens_a_project_without_closing_this_one() {
    // Opening a recent project starts another window on it, which is what the reference editor does, so the project that
    // is open stays open. The other window is a second process; this checks that this window is left alone
    // and that the entry is there to be chosen.
    let mut harness = harness("");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    let other = std::env::temp_dir().join("unluminous-recent-open-test");
    std::fs::create_dir_all(&other).expect("make the other project");
    harness.state_mut().recent = vec![other.clone()];
    harness.run();
    let before = harness.state().tree.root().to_path_buf();

    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label("unluminous-recent-open-test");
    harness.snapshot(shot("recent_projects_menu"));

    assert_eq!(
        harness.state().tree.root(),
        before,
        "the project that is open should not have been replaced by looking at the menu"
    );
    std::fs::remove_dir_all(&other).ok();
}

/// The keyboard shortcuts belonging to the menus.
///
/// On macOS these never reach the window, because the bar along the top of the screen takes them first and
/// sends an action instead. Inside the window, which is what Windows uses and what this harness draws, they
/// are watched for and turned into the same actions. This tests that path.
#[test]
fn the_menu_shortcuts_work_from_the_keyboard() {
    let mut harness = harness("some writing to look at");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.run();

    // Command and comma opens the settings.
    assert!(!harness.state().settings_window.open);
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Comma);
    harness.run();
    assert!(harness.state().settings_window.open, "command and comma should open the settings");
    harness.get_by_label("Done").click();
    harness.run();

    // Command and one, two and three switch between the three ways of looking at the file.
    for (key, expected) in [
        (egui::Key::Num2, ViewMode::SideBySide),
        (egui::Key::Num3, ViewMode::Preview),
        (egui::Key::Num1, ViewMode::Raw),
    ] {
        harness.key_press_modifiers(Modifiers::COMMAND, key);
        harness.run();
        assert_eq!(
            harness.state().view_mode(),
            expected,
            "command and {key:?} should switch the view"
        );
    }

    // Command and zero puts the explorer away and brings it back.
    assert!(harness.state().explorer_visible);
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Num0);
    harness.run();
    assert!(!harness.state().explorer_visible);
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Num0);
    harness.run();
    assert!(harness.state().explorer_visible);

    // Command and A selects the whole document, which used to be handled by the editing surface and is now
    // the Edit menu's entry.
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::A);
    harness.run();
    assert_eq!(harness.state().document().selected_text(), "some writing to look at");
}

#[test]
fn control_and_backtick_opens_the_terminal_and_puts_it_away() {
    let mut harness = harness("");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.run();
    assert!(!harness.state().terminal.visible);

    // The control key, not the Apple key, which is the shortcut every editor with a terminal uses.
    harness.key_press_modifiers(Modifiers::CTRL, egui::Key::Backtick);
    harness.run();
    assert!(harness.state().terminal.visible, "control and backtick should open the terminal");
    assert_eq!(harness.state().terminal.tabs.count(), 1);

    harness.key_press_modifiers(Modifiers::CTRL, egui::Key::Backtick);
    harness.run();
    assert!(!harness.state().terminal.visible);
}

#[test]
fn a_shell_that_will_not_start_says_so_rather_than_leaving_an_empty_tile() {
    let mut harness = harness("");
    harness.state_mut().settings.terminal_shell = "/no/such/program/at/all".to_owned();
    harness.state_mut().terminal.visible = true;
    harness.state_mut().new_terminal_tab();
    harness.run();

    assert_eq!(harness.state().terminal.tabs.count(), 0, "there is nothing to run");
    assert!(
        harness.state().terminal.visible,
        "the tile stays open, because it is the only place the reason can be read"
    );
    let reason = harness.state().terminal.tabs.last_error.clone().expect("a reason");
    assert!(
        reason.contains("/no/such/program/at/all"),
        "the reason should name the program, it said {reason:?}"
    );
    harness.snapshot(shot("terminal_will_not_start"));
}

// ---------------------------------------------------------------------------------------------
// task-1649: the gutter, the tabs, the menus, git and the plugins.
// ---------------------------------------------------------------------------------------------

#[test]
fn the_gutter_numbers_the_lines_and_a_wrapped_paragraph_is_numbered_once() {
    // A paragraph long enough to wrap, then two short ones. The wrapped one must carry one number
    // against its first row and nothing against its continuations, which is what a line number
    // means everywhere else.
    let long = "This paragraph is deliberately long enough that it has to be broken over more than one row on screen, which is the case a line number has to get right.";
    let mut harness = harness(&format!("{long}\nsecond\nthird\n"));
    collapse(&mut harness);
    assert!(harness.state().settings.line_numbers, "numbers are on to begin with");
    let rows = harness.state().layout().lines.len();
    assert!(rows > 3, "the first paragraph should have wrapped, there are {rows} rows");
    harness.snapshot(shot("gutter_line_numbers"));
}

#[test]
fn the_line_numbers_can_be_put_away_and_the_text_goes_back_to_where_it_was() {
    let mut harness = harness("one\ntwo\nthree\n");
    collapse(&mut harness);
    let with_numbers = harness.state().editor_area().left();
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::ToggleLineNumbers, &ctx);
    harness.run();
    assert!(!harness.state().settings.line_numbers);
    let without = harness.state().editor_area().left();
    assert!(without < with_numbers, "the editing area reaches further left with no gutter");
    harness.snapshot(shot("gutter_hidden"));
}

#[test]
fn the_gutters_own_menu_opens_where_it_was_clicked() {
    let mut harness = harness("one\ntwo\nthree\n");
    collapse(&mut harness);
    // Opened through the window's own state rather than by pressing the right mouse button, which
    // the harness cannot do. That is why the menu is the window's state and not egui's memory.
    let at = harness.state().editor_area().left_top() + vec2(-30.0, 40.0);
    harness.state_mut().gutter_menu = Some(at);
    harness.run();
    harness.get_by_label("Hide Line Numbers");
    harness.snapshot(shot("gutter_menu"));
}

#[test]
fn three_files_open_in_three_tabs_and_the_one_showing_is_underlined() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    harness.state_mut().open_path_permanently(&folder.join("notes.txt")).expect("the file opens");
    harness.state_mut().open_path_permanently(&folder.join("program.rs")).expect("the file opens");
    harness.run();
    assert_eq!(harness.state().files.len(), 3);
    // A change in the middle tab, so the amber dot is in the picture too.
    harness.state_mut().show_tab(1);
    harness.state_mut().command(Command::Insert("edited".to_owned()));
    harness.state_mut().show_tab(2);
    harness.run();
    harness.get_by_label("Tab: program.rs");
    harness.snapshot(shot("file_tabs"));
}

#[test]
fn a_single_click_reuses_one_tab_and_a_double_click_opens_another() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path(&folder.join("readme.md")).expect("the file opens");
    harness.run();
    assert_eq!(harness.state().files.len(), 1);
    assert!(harness.state().files.active().transient, "one click is a glance");

    harness.state_mut().open_path(&folder.join("notes.txt")).expect("the file opens");
    harness.run();
    assert_eq!(harness.state().files.len(), 1, "a second glance replaces the first");
    assert_eq!(harness.state().files.active().name(), "notes.txt");

    harness.state_mut().open_path_permanently(&folder.join("program.rs")).expect("the file opens");
    harness.run();
    assert_eq!(harness.state().files.len(), 2, "a double click keeps what was there");
}

#[test]
fn typing_into_a_tab_that_was_only_glanced_at_keeps_it() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path(&folder.join("readme.md")).expect("the file opens");
    harness.run();
    assert!(harness.state().files.active().transient);
    harness.input_mut().events.push(egui::Event::Text("a".to_owned()));
    harness.run();
    assert!(!harness.state().files.active().transient, "editing it means you meant to open it");
}

#[test]
fn a_tab_can_be_closed_and_the_last_one_leaves_an_untitled_document() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    harness.state_mut().open_path_permanently(&folder.join("notes.txt")).expect("the file opens");
    harness.run();
    harness.get_by_label("Close notes.txt").click();
    harness.run();
    assert_eq!(harness.state().files.len(), 1);
    assert_eq!(harness.state().files.active().name(), "readme.md");
    harness.state_mut().close_tab(0);
    harness.run();
    assert_eq!(
        harness.state().files.active().name(),
        "untitled",
        "never a window with no document"
    );
}

#[test]
fn the_explorers_own_menu_holds_what_can_be_done_to_a_file() {
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().explorer_menu = Some((
        egui::pos2(120.0, 260.0),
        folder.join("readme.md"),
        false,
        unluminous_app::app::actions::Aim::AtARow,
    ));
    harness.run();
    // Asked for by names this menu alone has: `File` is also a menu in the bar, and `New` is a
    // heading rather than a control, because a submenu inside the window is drawn as a heading with
    // its entries indented under it.
    for entry in ["Copy Path", "Rename...", "Reload from Disk"] {
        harness.get_by_label(entry);
    }
    let entries = unluminous_app::app::actions::explorer_menu(
        &folder.join("readme.md"),
        false,
        false,
        unluminous_app::app::actions::Aim::AtARow,
    );
    assert!(
        format!("{entries:?}").contains("NewFile"),
        "New > File is in the menu, which is what task-1649 asks for"
    );
    harness.snapshot(shot("explorer_menu"));
}

#[test]
fn making_a_file_from_the_explorers_menu_opens_it() {
    let folder = std::env::temp_dir().join("unluminous-screenshot-new-file");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the folder");
    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::NewFile(folder.clone()), &ctx);
    harness.run();
    harness.snapshot(shot("new_file_prompt"));

    // Any extension, which is what task-1649 asks for.
    if let Some(prompt) = harness.state_mut().prompt.as_mut() {
        prompt.value = "example.json".to_owned();
    }
    let prompt = harness.state_mut().prompt.take().expect("a prompt");
    harness.state_mut().run_prompt_for_test(prompt);
    harness.state_mut().prompt = None;
    harness.run();
    assert!(folder.join("example.json").is_file(), "the file is made");
    assert_eq!(harness.state().files.active().name(), "example.json", "and opened");
}

#[test]
fn renaming_a_file_moves_it_and_the_tab_follows() {
    let folder = std::env::temp_dir().join("unluminous-screenshot-rename");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the folder");
    std::fs::write(folder.join("before.md"), "# before\n").expect("write it");
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("before.md")).expect("the file opens");
    harness.run();
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::RenamePath(folder.join("before.md")), &ctx);
    harness.run();
    if let Some(prompt) = harness.state_mut().prompt.as_mut() {
        prompt.value = "after.md".to_owned();
    }
    let prompt = harness.state_mut().prompt.take().expect("a prompt");
    harness.state_mut().run_prompt_for_test(prompt);
    harness.state_mut().prompt = None;
    harness.run();
    assert!(!folder.join("before.md").exists());
    assert!(folder.join("after.md").is_file());
    assert_eq!(harness.state().files.active().name(), "after.md");
}

#[test]
fn cutting_a_file_and_pasting_it_into_a_folder_moves_it() {
    let folder = std::env::temp_dir().join("unluminous-screenshot-clipboard");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(folder.join("inner")).expect("make the folders");
    std::fs::write(folder.join("note.md"), "# note\n").expect("write it");
    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::CutPath(folder.join("note.md")), &ctx);
    harness.state_mut().run_action(Action::PasteInto(folder.join("inner")), &ctx);
    harness.run();
    assert!(!folder.join("note.md").exists(), "a cut moves it");
    assert!(folder.join("inner/note.md").is_file());
}

#[test]
fn the_view_menu_holds_the_font_size_and_ticks_the_mode_that_is_showing() {
    // The one menu in Unluminous with a checked row in it, so it is the one that shows the tick. It used
    // to be drawn as the character at U+2713, which no font in the stack Unluminous hands egui has a
    // shape for, so it came out as the empty box a missing glyph renders as — the fault the style
    // guide already records for the shift symbol, found again while retaking the documentation
    // captures for `task-1657`. **Look at the image**: there should be a tick beside `Raw Markdown`.
    let mut harness = harness("");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    for entry in ["Increase Font Size", "Decrease Font Size", "Reset Font Size"] {
        harness.get_by_label(entry);
    }
    // `Raw Markdown` is both a button on the strip and a row on this menu, so it is asked for by
    // count rather than by name.
    assert_eq!(harness.get_all_by_label("Raw Markdown").count(), 2);
    harness.snapshot(shot("view_menu"));
}

// ---------------------------------------------------------------------------------------------
// task-1658: the window's own resize grips, the rail of pane buttons, the project's state and
// pictures in a tab.
// ---------------------------------------------------------------------------------------------

/// The window is created with no operating system frame, so it has no resize grip of its own and one
/// is drawn at each edge and each corner. Before `task-1658` the only one that worked was the top,
/// and the window could not be made wider or shorter at all.
#[test]
fn the_window_can_be_resized_from_every_edge_and_every_corner() {
    let harness = harness("");
    for grip in
        ["top", "bottom", "left", "right", "top left", "top right", "bottom left", "bottom right"]
    {
        harness.get_by_label(&format!("Resize window: {grip}"));
    }
}

/// The rail down the far left is the one place a pane is put away and brought back from.
#[test]
fn the_rail_puts_each_pane_away_and_brings_it_back() {
    let mut harness = harness("");
    assert!(harness.state().explorer_visible);
    harness.get_by_label("Project").click();
    harness.run();
    assert!(!harness.state().explorer_visible, "the rail's Project button hides the explorer");
    harness.get_by_label("Project").click();
    harness.run();
    assert!(harness.state().explorer_visible, "and brings it back");

    assert!(!harness.state().terminal.visible);
    // A detached terminal, so nothing here depends on a shell starting.
    harness.state_mut().new_detached_terminal_tab(8, 60);
    harness.run();
    assert!(harness.state().terminal.visible);
    harness.get_by_label("Terminal tile").click();
    harness.run();
    assert!(!harness.state().terminal.visible, "the rail's terminal button puts the tile away");
    harness.snapshot(shot("activity_bar"));
}

/// The commit panel is the rail's third button, and it is the same action the Git menu's `Commit...`
/// entry is, so pressing it twice puts the panel away again.
#[test]
fn the_rails_version_control_button_opens_the_commit_panel_and_shuts_it() {
    let root = git_folder("unluminous-rail-git");
    let mut harness = harness_in(&root);
    assert!(harness.state().git.is_some(), "the folder should be a repository");
    harness.get_by_label("Version Control").click();
    // `nudge`, not `run`: opening the panel asks git for the status of the repository, and the thread that
    // answers asks for a repaint when it is done. A `run` that lands while that is still in flight panics
    // for a reason that is not a fault in Unluminous, which is what made this test fail in the full suite and
    // pass on its own.
    nudge(&mut harness);
    assert!(
        harness.state().git.as_ref().is_some_and(|git| git.panel.open),
        "the panel should be open"
    );
    harness.get_by_label("Version Control").click();
    nudge(&mut harness);
    assert!(
        harness.state().git.as_ref().is_some_and(|git| !git.panel.open),
        "and pressing it again should shut it"
    );
}

/// What was open in a project is written into a `.unluminous` folder beside it and read back next time.
#[test]
fn what_was_open_in_a_project_comes_back_when_it_is_opened_again() {
    let root = copy_out_of_the_repository(&sample_folder(), "unluminous-project-state-window");
    {
        let mut harness = harness_in(&root);
        harness.state_mut().restore_project();
        harness.state_mut().open_path_permanently(&root.join("readme.md")).expect("the file opens");
        harness.state_mut().open_path_permanently(&root.join("notes.txt")).expect("the file opens");
        harness.state_mut().tree.expand(&root.join("chapters"));
        harness.run();
        // Written when the window closes, as it is when a person shuts Unluminous.
        let ctx = harness.ctx.clone();
        harness.state_mut().run_action(Action::CloseWindow, &ctx);
    }
    assert!(
        root.join(".unluminous/open-files.txt").is_file(),
        "the project's state should be beside the project"
    );

    let mut second = harness_in(&root);
    second.state_mut().restore_project();
    second.run();
    let names: Vec<String> =
        second.state().files.iter().map(unluminous_app::app::files::OpenFile::name).collect();
    assert!(names.contains(&"readme.md".to_owned()), "the tabs came back, they are {names:?}");
    assert!(names.contains(&"notes.txt".to_owned()), "both of them, they are {names:?}");
    assert!(
        second.state().tree.expanded_folders().iter().any(|path| path.ends_with("chapters")),
        "and the folder that was opened out is opened out again"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// A picture opens in a tab that shows it, fitted to the editing area to begin with.
#[test]
fn a_picture_opens_in_a_tab_that_shows_it() {
    let mut harness = harness("");
    harness.get_by_label_contains("picture.png").click();
    harness.run();
    assert!(harness.state().files.active().is_picture(), "the tab should be holding a picture");
    let picture = harness.state().files.active().picture.as_ref().expect("a picture");
    assert_eq!(picture.problem, None, "it should have decoded");
    assert_eq!(picture.size, [160, 100]);
    assert!(harness.query_by_label("Text options").is_none(), "a picture has no text to format");
    harness.snapshot(shot("picture"));
}

/// Control and plus zooms the picture rather than the editor's font, and `Reset Font Size` fits it
/// back into the area.
#[test]
fn the_keyboard_zooms_a_picture_and_leaves_the_editors_font_alone() {
    let mut harness = harness("");
    harness.get_by_label_contains("picture.png").click();
    harness.run();
    let font_before = harness.state().settings.font_size;
    let area = harness.state().editor_area().size();
    let scale = |harness: &Harness<'static, UnluminousApp>| {
        harness.state().files.active().picture.as_ref().expect("a picture").scale_in(area)
    };
    let fitted = scale(&harness);

    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Equals);
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Equals);
    harness.run();
    let zoomed = scale(&harness);
    assert!(zoomed > fitted, "two presses should have made it bigger: {fitted} then {zoomed}");
    assert_eq!(
        harness.state().settings.font_size,
        font_before,
        "and the editor's own font should not have moved"
    );
    harness.snapshot(shot("picture_zoomed"));

    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::ResetFontSize, &ctx);
    harness.run();
    assert!((scale(&harness) - fitted).abs() < 0.001, "resetting should fit it back into the area");
}

/// A picture cannot be edited, so saving one must not write an empty file over it.
#[test]
fn saving_a_tab_that_holds_a_picture_does_not_write_over_the_picture() {
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-picture-save");
    let path = folder.join("picture.png");
    let before = std::fs::read(&path).expect("read the picture");
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&path).expect("the file opens");
    harness.run();
    assert!(harness.state().files.active().is_picture());
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Save, &ctx);
    harness.run();
    let after = std::fs::read(&path).expect("read it again");
    assert_eq!(before, after, "the picture on disk must be untouched");
    std::fs::remove_dir_all(&folder).ok();
}

// `Go to File`, `Find in Files`, and the modals that can be moved and resized (`task-1659`).

/// Open `Go to File` the way the shortcut does, and let it settle.
fn open_go_to_file(harness: &mut Harness<'static, UnluminousApp>) {
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::GoToFile, &ctx);
    harness.run();
}

/// The names the finder is currently offering, in the order it offers them.
fn found_names(harness: &Harness<'static, UnluminousApp>) -> Vec<String> {
    harness
        .state()
        .go_to_file
        .as_ref()
        .expect("the finder should be open")
        .results()
        .iter()
        .map(|found| found.name.clone())
        .collect()
}

#[test]
fn go_to_file_lists_the_project_before_anything_is_typed() {
    let mut harness = harness("");
    open_go_to_file(&mut harness);
    harness.get_by_label("Go to file");
    let names = found_names(&harness);
    assert!(names.contains(&"readme.md".to_owned()), "it lists what is there: {names:?}");
    assert!(names.contains(&"one.md".to_owned()), "including files inside folders: {names:?}");
}

#[test]
fn go_to_file_narrows_as_a_name_is_typed() {
    let mut harness = harness("");
    open_go_to_file(&mut harness);
    harness.input_mut().events.push(egui::Event::Text("one".to_owned()));
    harness.run();
    let names = found_names(&harness);
    assert_eq!(names.first().map(String::as_str), Some("one.md"), "best match first: {names:?}");
    assert!(!names.contains(&"notes.txt".to_owned()), "and what does not match is gone: {names:?}");
    harness.snapshot(shot("go_to_file"));
}

#[test]
fn double_clicking_a_row_in_go_to_file_opens_the_file_and_shuts_the_modal() {
    let mut harness = harness("");
    open_go_to_file(&mut harness);
    harness.input_mut().events.push(egui::Event::Text("readme".to_owned()));
    harness.run();
    double_click(&mut harness, "Go to readme.md");
    assert!(harness.state().go_to_file.is_none(), "opening a file shuts the modal");
    assert_eq!(harness.state().document().text().to_string(), "# Unluminous\n");
    assert_eq!(
        harness.state().files.active().path().and_then(|path| path.file_name()),
        Some(std::ffi::OsStr::new("readme.md")),
    );
}

#[test]
fn the_arrow_keys_and_enter_open_a_file_from_go_to_file() {
    let mut harness = harness("");
    open_go_to_file(&mut harness);
    harness.input_mut().events.push(egui::Event::Text("md".to_owned()));
    harness.run();
    let first = found_names(&harness)[0].clone();
    harness.key_press(egui::Key::ArrowDown);
    harness.run();
    let second = found_names(&harness)[1].clone();
    assert_ne!(first, second, "the sample folder has more than one Markdown file");
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(harness.state().go_to_file.is_none());
    assert_eq!(
        harness.state().files.active().name(),
        second,
        "Enter opens the row the arrow keys walked to"
    );
}

#[test]
fn escape_shuts_go_to_file_without_opening_anything() {
    let mut harness = harness("");
    let before = harness.state().files.active().name();
    open_go_to_file(&mut harness);
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.state().go_to_file.is_none());
    assert_eq!(harness.state().files.active().name(), before);
}

/// Open `Find in Files`, type `text`, and wait for the thread to finish reading the project.
///
/// The search runs on a thread, so the test waits for an answer rather than assuming one frame is
/// enough. `pump` rather than `Harness::run` inside the loop, for the reason `task-1654` records:
/// `run` gives the window four steps to go quiet and panics otherwise, which is right for a settled
/// window and wrong while something is still being worked on.
fn search_for(harness: &mut Harness<'static, UnluminousApp>, text: &str) {
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::FindInFiles, &ctx);
    harness.run();
    harness.input_mut().events.push(egui::Event::Text(text.to_owned()));
    harness.run();
    for _ in 0..200 {
        if !harness
            .state()
            .find_in_files
            .as_ref()
            .expect("the search should be open")
            .is_searching()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.step();
    }
    harness.run();
}

/// Where the search found its matches, as `name:line` for each one.
fn matches(harness: &Harness<'static, UnluminousApp>) -> Vec<String> {
    harness
        .state()
        .find_in_files
        .as_ref()
        .expect("the search should be open")
        .hits()
        .iter()
        .map(|hit| format!("{}:{}", hit.path.file_name().unwrap().to_string_lossy(), hit.line))
        .collect()
}

#[test]
fn find_in_files_finds_text_anywhere_in_the_project() {
    let mut harness = harness("");
    search_for(&mut harness, "Unluminous");
    let found = matches(&harness);
    assert!(found.contains(&"readme.md:1".to_owned()), "readme.md says `# Unluminous`: {found:?}");
    harness.get_by_label("Find in files");
    harness.get_by_label("Match case");
    harness.snapshot(shot("find_in_files"));
}

#[test]
fn find_in_files_narrows_to_nothing_when_nothing_matches() {
    let mut harness = harness("");
    search_for(&mut harness, "zzzznothinghere");
    assert!(matches(&harness).is_empty());
    assert!(!harness.state().find_in_files.as_ref().unwrap().is_searching());
}

#[test]
fn opening_a_result_selects_the_match_in_the_document() {
    let mut harness = harness("");
    search_for(&mut harness, "tables");
    let found = matches(&harness);
    assert_eq!(found, vec!["tables.txt:1".to_owned()], "one file holds the word: {found:?}");
    double_click(&mut harness, "Result tables.txt:1");
    assert!(harness.state().find_in_files.is_none(), "opening a result shuts the modal");
    assert_eq!(harness.state().files.active().name(), "tables.txt");
    assert_eq!(
        harness.state().document().selected_text(),
        "tables",
        "the match itself should be selected, which is what highlights it"
    );
}

#[test]
fn the_case_of_a_search_can_be_insisted_on() {
    let mut harness = harness("");
    search_for(&mut harness, "unluminous");
    assert!(
        !matches(&harness).is_empty(),
        "`unluminous` should find `# Unluminous` while case is being ignored"
    );
    harness.get_by_label("Match case").click();
    harness.run();
    for _ in 0..200 {
        if !harness.state().find_in_files.as_ref().unwrap().is_searching() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.step();
    }
    harness.run();
    let found = matches(&harness);
    assert!(
        !found.iter().any(|hit| hit.starts_with("readme.md")),
        "with the case insisted on, `unluminous` no longer matches `Unluminous`: {found:?}"
    );
}

#[test]
fn escape_shuts_find_in_files() {
    let mut harness = harness("");
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::FindInFiles, &ctx);
    harness.run();
    assert!(harness.state().find_in_files.is_some());
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(harness.state().find_in_files.is_none());
}

// A modal is moved by its header and resized by its edges (`task-1659`). Both live in
// `components::modal`, so `Go to File` is what they are tested through and every other modal has
// them for the same reason.

/// Where the modal with this id sits and how big it is, read back from egui's own memory.
fn placement(
    harness: &Harness<'static, UnluminousApp>,
    id: &str,
) -> unluminous_app::components::modal::Placement {
    unluminous_app::components::modal::placement(&harness.ctx, id)
}

#[test]
fn a_modal_is_moved_by_dragging_its_header() {
    let mut harness = harness("");
    open_go_to_file(&mut harness);
    assert!(placement(&harness, "unluminous-go-to-file").is_untouched(), "it opens in the middle");
    let bar = harness.get_by_label("Move go to file").rect();
    let from = bar.center();
    drag(&mut harness, from, from + egui::vec2(-120.0, 60.0));
    let moved = placement(&harness, "unluminous-go-to-file");
    assert!(moved.offset.x < -100.0, "dragged left: {moved:?}");
    assert!(moved.offset.y > 40.0, "and down: {moved:?}");
    // The modal really is somewhere else, rather than only the number having changed.
    let after = harness.get_by_label("Move go to file").rect();
    assert!(after.center().x < bar.center().x - 100.0);
    harness.snapshot(shot("modal_dragged"));
}

#[test]
fn a_modal_is_resized_by_dragging_a_corner() {
    let mut harness = harness("");
    open_go_to_file(&mut harness);
    let before = harness.get_by_label("Move go to file").rect().width();
    let corner = harness.get_by_label("Resize go to file: bottom right").rect().center();
    drag(&mut harness, corner, corner + egui::vec2(80.0, 40.0));
    let grown = placement(&harness, "unluminous-go-to-file");
    assert!(grown.grown.x > 60.0, "wider: {grown:?}");
    assert!(grown.grown.y > 20.0, "and taller: {grown:?}");
    let after = harness.get_by_label("Move go to file").rect().width();
    assert!(after > before + 60.0, "{before} then {after}");
}

#[test]
fn dragging_the_left_edge_of_a_modal_leaves_its_right_edge_where_it_was() {
    let mut harness = harness("");
    open_go_to_file(&mut harness);
    let before = harness.get_by_label("Move go to file").rect();
    let edge = egui::pos2(before.left() + 3.0, before.center().y + 120.0);
    drag(&mut harness, edge, edge + egui::vec2(-60.0, 0.0));
    let after = harness.get_by_label("Move go to file").rect();
    assert!(after.left() < before.left() - 40.0, "the edge that was dragged moved");
    assert!(
        (after.right() - before.right()).abs() < 2.0,
        "and the other one did not: {} then {}",
        before.right(),
        after.right()
    );
}

#[test]
fn double_clicking_a_modals_header_puts_it_back_in_the_middle() {
    let mut harness = harness("");
    open_go_to_file(&mut harness);
    let bar = harness.get_by_label("Move go to file").rect();
    let from = bar.center();
    drag(&mut harness, from, from + egui::vec2(-120.0, 60.0));
    assert!(!placement(&harness, "unluminous-go-to-file").is_untouched());
    double_click(&mut harness, "Move go to file");
    assert!(
        placement(&harness, "unluminous-go-to-file").is_untouched(),
        "a double click puts it back, as it does to a pane divider"
    );
}

#[test]
fn a_modal_cannot_be_dragged_out_of_the_window() {
    let mut harness = harness("");
    open_go_to_file(&mut harness);
    let bar = harness.get_by_label("Move go to file").rect();
    let from = bar.center();
    // Far past the right hand edge of a 1180 point window.
    drag(&mut harness, from, from + egui::vec2(4000.0, 4000.0));
    let after = harness.get_by_label("Move go to file").rect();
    assert!(after.right() <= WINDOW[0], "still inside: {after:?}");
    assert!(after.top() >= 0.0);
}

// Pictures in the Markdown preview (`task-1659`).

/// A folder of its own holding a Markdown file with a picture in it.
///
/// Its own folder rather than the shared sample, because every explorer screenshot counts the files
/// in that one, and a document made of pictures is not what the rest of the tests are about. Written
/// each time, like `git_folder`, and kept apart by its name.
///
/// **Not [`fixture`], because `picture.png` is not text**, and a Markdown document about a picture
/// needs the picture beside it.
fn picture_document_folder() -> std::path::PathBuf {
    let root = std::env::temp_dir().join("unluminous-preview-pictures");
    std::fs::create_dir_all(&root).expect("make the folder");
    write_sample_picture(&root.join("picture.png"));
    std::fs::write(
        root.join("gallery.md"),
        "# A picture\n\nSome words before it.\n\n![the sample picture](picture.png)\n\nAnd some after.\n\n![missing](nowhere.png)\n",
    )
    .expect("write gallery.md");
    root
}

/// Open `gallery.md` in the preview, in a window on the folder holding it.
fn preview_of_the_gallery() -> Harness<'static, UnluminousApp> {
    let folder = picture_document_folder();
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("gallery.md")).expect("the file opens");
    harness.run();
    harness.state_mut().set_view_mode(ViewMode::Preview);
    harness.run();
    harness.run();
    harness
}

#[test]
fn the_markdown_preview_draws_a_picture() {
    let mut harness = preview_of_the_gallery();
    let pictures = harness.state().preview_pictures();
    assert_eq!(pictures.len(), 2, "one picture that is there and one that is not");
    let drawn = &pictures[0];
    assert!(drawn.texture.is_some(), "the picture beside the document should have been read");
    assert_eq!(drawn.size, egui::vec2(160.0, 100.0), "at its own size, which fits the pane");
    harness.snapshot(shot("preview_picture"));
}

#[test]
fn a_picture_that_is_not_there_leaves_its_alt_text() {
    let harness = preview_of_the_gallery();
    let missing = &harness.state().preview_pictures()[1];
    assert!(missing.texture.is_none());
    assert_eq!(missing.alt, "missing", "which is what is drawn in its place");
    assert_eq!(missing.size, egui::vec2(0.0, 0.0), "and it takes no room of its own");
}

#[test]
fn the_line_holding_a_picture_is_as_tall_as_the_picture() {
    let harness = preview_of_the_gallery();
    let picture = &harness.state().preview_pictures()[0];
    let line = harness
        .state()
        .preview_layout()
        .lines
        .iter()
        .find(|line| line.paragraph == picture.paragraph)
        .expect("the picture's own line");
    assert!(
        line.height >= picture.size.y,
        "the room reserved ({}) has to hold the picture ({})",
        line.height,
        picture.size.y
    );
    // And what follows it really is below it, rather than drawn over it.
    let next = harness
        .state()
        .preview_layout()
        .lines
        .iter()
        .find(|line| line.paragraph > picture.paragraph)
        .expect("the line after the picture");
    assert!(next.y >= line.y + picture.size.y);
}

#[test]
fn a_wide_picture_is_scaled_down_to_the_width_of_the_pane() {
    let folder = std::env::temp_dir().join("unluminous-preview-wide-picture");
    std::fs::create_dir_all(&folder).expect("make the folder");
    // Four thousand pixels across, which is wider than any pane in a 1180 point window.
    let wide = image::RgbaImage::from_pixel(4000, 1000, image::Rgba([0x30, 0x70, 0xC0, 255]));
    wide.save(folder.join("wide.png")).expect("write wide.png");
    std::fs::write(folder.join("wide.md"), "![wide](wide.png)\n").expect("write wide.md");
    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&folder.join("wide.md")).expect("the file opens");
    harness.run();
    harness.state_mut().set_view_mode(ViewMode::Preview);
    harness.run();
    harness.run();
    let picture = &harness.state().preview_pictures()[0];
    assert!(picture.size.x <= WINDOW[0], "scaled to fit: {:?}", picture.size);
    assert!(
        (picture.size.x / picture.size.y - 4.0).abs() < 0.01,
        "and kept its shape: {:?}",
        picture.size
    );
    std::fs::remove_dir_all(&folder).ok();
}

/// The counterpart of `cargo run --example markdown_check`, run automatically as part of the suite
/// rather than left for somebody to remember to run by hand.
///
/// It walks the repository for every `.md` and `.markdown` file, skipping the same names the example
/// skips, renders each one with `unluminous_core::markdown::render` and checks the same four
/// invariants the example's own `problems` function checks: the spans cover the text exactly, there
/// is one paragraph style and one source line for every preview line, the source lines never go
/// backwards, and everything a picture, a diagram or a panel names is inside the text.
/// `unluminous-core`'s own `markdown::tests` module checks the same four against a battery of short
/// fixtures written by hand; this checks them against the hundred and some real files this
/// repository is actually written in, which is `markdown_check`'s own reason for existing — nobody
/// writing a fixture thinks to write the shape of Markdown that breaks a parser, and files written
/// by hand over months already hold it.
///
/// Reading the repository from a test is fine here in the one way it is ever fine: what is read is
/// the checked-in Markdown files themselves, which travel with the code and answer the same way on
/// every machine that runs the suite, unlike a person's own settings, projects or fonts, which a
/// test must never depend on because those differ from one machine to the next.
/// `every_accepted_image_is_named_by_a_test`, near the top of this file, already reads this same
/// repository's own test sources for the same reason.
///
/// The walk and the four checks are copies of the example's own `collect` and `problems` functions
/// rather than calls into them, because an example is a separate binary a test cannot call into and
/// the example itself is not to change: its printed table is what a person reads when a file breaks.
#[test]
fn every_markdown_file_in_the_repository_renders_with_no_broken_invariant() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    collect_markdown_files(&root, &mut files);
    assert!(
        files.len() >= 50,
        "found only {} markdown files under {}, which looks like a broken walk rather than a small \
         repository",
        files.len(),
        root.display()
    );

    let options = unluminous_core::markdown::Options::new(
        unluminous_core::CharStyle::default(),
        unluminous_core::PreviewColors::default(),
        Some("Courier".to_owned()),
    );

    let mut broken = Vec::new();
    for path in &files {
        let Ok(source) = std::fs::read_to_string(path) else { continue };
        let preview = unluminous_core::markdown::render(&source, &options);
        for problem in markdown_preview_problems(&preview) {
            broken.push(format!("{}: {problem}", path.display()));
        }
    }
    assert!(
        broken.is_empty(),
        "{} problem(s) found among {} markdown files:\n{}",
        broken.len(),
        files.len(),
        broken.join("\n")
    );
}

/// Every `.md` and `.markdown` file under a folder, leaving out what a build wrote.
///
/// Copied from `examples/markdown_check.rs`'s own `collect`; see
/// `every_markdown_file_in_the_repository_renders_with_no_broken_invariant` for why it is a copy
/// rather than a shared call.
fn collect_markdown_files(folder: &std::path::Path, into: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if name.starts_with('.') || matches!(name.as_str(), "target" | "node_modules" | "releases")
        {
            continue;
        }
        if path.is_dir() {
            collect_markdown_files(&path, into);
        } else if path.extension().is_some_and(|end| end == "md" || end == "markdown") {
            into.push(path);
        }
    }
}

/// The four things that must be true of every preview whatever the source was.
///
/// Copied from `examples/markdown_check.rs`'s own `problems`, for the same reason
/// `collect_markdown_files` above is a copy.
fn markdown_preview_problems(preview: &unluminous_core::Preview) -> Vec<String> {
    let lines = preview.text.len_lines();
    let mut out = Vec::new();
    if preview.chars.total_len() != preview.text.len_bytes() {
        out.push(format!(
            "the spans cover {} bytes of {}",
            preview.chars.total_len(),
            preview.text.len_bytes()
        ));
    }
    if preview.paragraphs.len() != lines {
        out.push(format!("{} paragraph styles for {lines} lines", preview.paragraphs.len()));
    }
    if preview.source_lines.len() != lines {
        out.push(format!("{} source lines for {lines} lines", preview.source_lines.len()));
    }
    if !preview.source_lines.windows(2).all(|pair| pair[0] <= pair[1]) {
        out.push("the source lines go backwards".to_owned());
    }
    for image in &preview.images {
        if image.paragraph >= lines {
            out.push(format!("a picture at paragraph {} of {lines}", image.paragraph));
        }
    }
    for diagram in &preview.diagrams {
        if diagram.paragraph >= lines {
            out.push(format!("a diagram at paragraph {} of {lines}", diagram.paragraph));
        }
    }
    for panel in &preview.panels {
        if panel.paragraphs.end > lines || panel.paragraphs.start >= panel.paragraphs.end {
            out.push(format!("a panel over {:?} of {lines}", panel.paragraphs));
        }
    }
    out
}

#[test]
fn the_shortcut_on_the_menu_opens_go_to_file() {
    let mut harness = harness("");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::O);
    harness.run();
    assert!(harness.state().go_to_file.is_some(), "Ctrl or Cmd, Shift and O");
}

#[test]
fn the_shortcut_on_the_menu_opens_find_in_files() {
    let mut harness = harness("");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::F);
    harness.run();
    assert!(harness.state().find_in_files.is_some(), "Ctrl or Cmd, Shift and F");
}

#[test]
fn typing_in_go_to_file_leaves_the_document_alone() {
    // The same rule `task-1656` is about: egui leaves the events a text box consumed in the frame's
    // list, so a new box is a new chance for the document behind it to read them too.
    let mut harness = harness("the document");
    let before = harness.state().document().text().to_string();
    // `with_text` types the text in, so the document starts out modified; what matters is that
    // nothing typed into the box changes it any further.
    let modified = harness.state().document().is_modified();
    open_go_to_file(&mut harness);
    harness.input_mut().events.push(egui::Event::Text("readme".to_owned()));
    harness.run();
    harness.key_press(egui::Key::Backspace);
    harness.run();
    assert_eq!(harness.state().go_to_file.as_ref().unwrap().query, "readm");
    assert_eq!(harness.state().document().text().to_string(), before);
    assert_eq!(harness.state().document().is_modified(), modified);
}

#[test]
fn typing_in_find_in_files_leaves_the_document_alone() {
    let mut harness = harness("the document");
    let before = harness.state().document().text().to_string();
    let modified = harness.state().document().is_modified();
    search_for(&mut harness, "unluminous");
    harness.key_press(egui::Key::Backspace);
    harness.run();
    assert_eq!(harness.state().find_in_files.as_ref().unwrap().query, "unluminou");
    assert_eq!(harness.state().document().text().to_string(), before);
    assert_eq!(harness.state().document().is_modified(), modified);
}

#[test]
fn the_divider_in_find_in_files_moves_the_split_between_the_results_and_the_preview() {
    let mut harness = harness("");
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::FindInFiles, &ctx);
    harness.run();
    let before = harness.state().panes.find_split;
    let divider = harness.get_by_label("Resize find results").rect().center();
    drag(&mut harness, divider, divider + egui::vec2(0.0, 90.0));
    let after = harness.state().panes.find_split;
    assert!(after > before + 0.05, "the results should have grown: {before} then {after}");
    // And it is a pane like any other, so a double click puts it back.
    double_click(&mut harness, "Resize find results");
    assert!(
        (harness.state().panes.find_split - unluminous_app::components::find_in_files::SPLIT).abs()
            < 0.001
    );
}

#[test]
fn the_preview_under_the_results_follows_the_one_that_is_chosen() {
    let mut harness = harness("");
    search_for(&mut harness, "Unluminous");
    let find = harness.state().find_in_files.as_ref().expect("open");
    let chosen = find.chosen_hit().expect("something matched").clone();
    assert_eq!(
        find.scrolled_to(),
        Some((chosen.path.as_path(), chosen.line)),
        "the preview should have been scrolled to the result that is chosen"
    );
    // Walking the list moves the preview with it.
    if harness.state().find_in_files.as_ref().unwrap().hits().len() > 1 {
        harness.key_press(egui::Key::ArrowDown);
        harness.run();
        let find = harness.state().find_in_files.as_ref().unwrap();
        let now = find.chosen_hit().unwrap().clone();
        assert_ne!((now.path.clone(), now.line), (chosen.path, chosen.line));
        assert_eq!(find.scrolled_to(), Some((now.path.as_path(), now.line)));
    }
}

// The window closing or minimising itself while somebody types.
//
// This is what the report of a crash while typing turned out to be, and it is also the report of the
// window minimising itself in the middle of a word. Neither is a crash: nothing panics, nothing is
// written to `crash.log`, and macOS files no report, because the window is asked to close in the
// ordinary way — by a button being pressed.
//
// egui moves keyboard focus when a bare `Tab` or a bare arrow key is pressed, and it keeps moving it
// unless the widget that holds focus says those keys are its own. Unluminous's editing area never held
// egui's focus, so nothing said that, and the focus walked out of the document and onto the three
// window buttons in the title bar. A button with keyboard focus is pressed by `Space` or `Enter`. So
// one arrow key followed by a space closed the window if the focus had landed on `Close`, minimised it
// if it had landed on `Minimise`, and resized it if it had landed on `Maximise` — while the person was
// doing nothing but typing.
//
// Every key below is one a person types constantly in a code file.

/// The commands the window sent this frame that move, close or resize the window itself.
fn window_commands(harness: &Harness<'static, UnluminousApp>) -> Vec<String> {
    harness
        .output()
        .viewport_output
        .values()
        .flat_map(|viewport| viewport.commands.iter())
        .filter(|command| {
            matches!(
                command,
                egui::ViewportCommand::Close
                    | egui::ViewportCommand::Minimized(_)
                    | egui::ViewportCommand::Maximized(_)
                    | egui::ViewportCommand::Fullscreen(_)
                    | egui::ViewportCommand::StartDrag
            )
        })
        .map(|command| format!("{command:?}"))
        .collect()
}

/// Whether any of the three window buttons holds the keyboard.
fn a_window_button_holds_the_keyboard(
    harness: &mut Harness<'static, UnluminousApp>,
) -> Option<String> {
    ["Close", "Minimise", "Maximise"].into_iter().find_map(|label| {
        let held = harness.get_all_by_label(label).any(|node| node.is_focused());
        held.then(|| label.to_owned())
    })
}

#[test]
fn typing_a_space_after_a_tab_or_an_arrow_key_cannot_close_or_minimise_the_window() {
    let mut harness = javascript_harness();
    type_and_paint(&mut harness, "class Person {\n");

    // Each of these is pressed and then a space is typed, which is what pressing a focused button
    // takes. The arrows are in it because egui moves focus on those as well as on `Tab`, and an arrow
    // key in a code file is the commonest key press there is.
    for key in [
        egui::Key::Tab,
        egui::Key::ArrowUp,
        egui::Key::ArrowDown,
        egui::Key::ArrowLeft,
        egui::Key::ArrowRight,
    ] {
        harness.key_press(key);
        harness.run();
        assert_eq!(
            a_window_button_holds_the_keyboard(&mut harness),
            None,
            "after {key:?} the keyboard is still in the document, not on a window button"
        );
        // And it is Unluminous's own holder that has it, so this test is passing for the reason it is meant
        // to and not because the title bar happened not to be drawn.
        assert_eq!(
            harness.ctx.memory(|memory| memory.focused()),
            Some(egui::Id::new(unluminous_app::app::KEYBOARD_HOLDER)),
            "the focus stays where Unluminous put it after {key:?}"
        );

        // A space, as a keyboard sends it: the key press and the letter both.
        harness.key_press(egui::Key::Space);
        harness.input_mut().events.push(egui::Event::Text(" ".to_owned()));
        harness.run();
        let commands = window_commands(&harness);
        assert!(
            commands.is_empty(),
            "{key:?} then a space must type a space and nothing else, and it sent {commands:?}"
        );

        // And the same for Enter, which presses a focused button too.
        harness.key_press(key);
        harness.run();
        harness.key_press(egui::Key::Enter);
        harness.run();
        let commands = window_commands(&harness);
        assert!(
            commands.is_empty(),
            "{key:?} then Enter must not reach a window button, and it sent {commands:?}"
        );
    }

    // The keys have to keep doing what they did, which is the other half of it. The arrows moved the
    // caret about as they were pressed, so where in the file each character landed is not fixed — what
    // matters is that every one of these keys still reached the document: the class was typed, `Tab`
    // still put a tab in and the spaces still went in as spaces.
    let text = harness.state().document().text().to_string();
    assert!(text.contains("Person {"), "what was typed is still there: {text:?}");
    assert!(text.contains('\t'), "Tab still types a tab: {text:?}");
    assert!(text.contains(' '), "and a space is still a space: {text:?}");
}

#[test]
fn an_idle_window_always_asks_to_be_woken_again() {
    // The window was found asleep with a command queued and no frame drawn in three seconds: it had
    // asked for no frame, so the only thing that could have drawn one was a wake from another thread,
    // and that wake never arrived. Asking for the next frame on every frame is what makes a lost wake
    // cost a quarter of a second. If this ever stops being true, a missed wake can hang the window
    // again, and there is no way to see that from a screenshot.
    let mut harness = harness("");
    harness.run();
    let asked_for = harness
        .output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .expect("the window's own output")
        .repaint_delay;
    assert!(
        asked_for <= unluminous_app::app::HEARTBEAT,
        "an idle window asked to sleep for {asked_for:?}, which is longer than the heartbeat of {:?}",
        unluminous_app::app::HEARTBEAT
    );

    // And typing does not take it away: it is asked for on every frame, not on the first.
    harness.get_by_label_contains("readme.md").click();
    harness.run();
    harness.input_mut().events.push(egui::Event::Text("x".to_owned()));
    harness.run();
    harness.run();
    let asked_for = harness
        .output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .expect("the window's own output")
        .repaint_delay;
    assert!(
        asked_for <= unluminous_app::app::HEARTBEAT,
        "and still after a frame of typing: {asked_for:?}"
    );
}

/// What holds egui's keyboard focus this frame.
fn what_holds_the_keyboard(harness: &Harness<'static, UnluminousApp>) -> Option<egui::Id> {
    harness.ctx.memory(|memory| memory.focused())
}

/// Unluminous's own holder, which is where the focus belongs while a pane is being typed into.
fn the_holder() -> egui::Id {
    egui::Id::new(unluminous_app::app::KEYBOARD_HOLDER)
}

#[test]
fn a_text_box_takes_the_keyboard_from_the_holder_and_the_holder_takes_it_back() {
    // The other half of holding the focus: a box that is typed into has to be able to take it, or the
    // explorer's filter, the commit message and the rename prompt could never be typed into at all.
    let mut harness = harness("");
    harness.run();
    assert_eq!(what_holds_the_keyboard(&harness), Some(the_holder()), "it starts here");

    harness.get_by_label("Filter files").click();
    harness.run();
    assert_ne!(
        what_holds_the_keyboard(&harness),
        Some(the_holder()),
        "a click on the filter box hands the keyboard over, and it is not taken back on the next frame"
    );
    harness.get_by_label("Filter files").type_text("two");
    harness.run();
    assert_eq!(harness.state().filter, "two", "so it can be typed into");

    // Escape hands the keyboard back, and the holder has it again.
    harness.key_press(egui::Key::Escape);
    harness.run();
    harness.run();
    assert_eq!(
        what_holds_the_keyboard(&harness),
        Some(the_holder()),
        "and when the box lets go, the focus comes back rather than sitting on nothing"
    );
}

#[test]
fn a_tab_out_of_a_text_box_cannot_land_on_a_window_button() {
    // The second line of defence. `Tab` in a text box is the box's own key only while the box holds the
    // keyboard; egui moves the focus on out of it, and where it goes is whatever egui draws next that
    // can take focus. The three window buttons take no focus at all, so it cannot be one of them, and
    // whatever it is the holder takes the keyboard back on the frame after.
    let mut harness = harness("");
    harness.get_by_label("Filter files").click();
    harness.run();

    for press in 0..12 {
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert_eq!(
            a_window_button_holds_the_keyboard(&mut harness),
            None,
            "Tab number {press} out of the filter box reached a window button"
        );
        harness.key_press(egui::Key::Space);
        harness.input_mut().events.push(egui::Event::Text(" ".to_owned()));
        harness.run();
        let commands = window_commands(&harness);
        assert!(commands.is_empty(), "and a space after it sent {commands:?}");
        assert_eq!(
            what_holds_the_keyboard(&harness),
            Some(the_holder()),
            "the keyboard is back with the holder after Tab number {press}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// `task-1848`: "Links in markdown should allow me to CMD/Ctrl+Click to open them in a new browser
// window." The preview drew a link's words and nothing knew it was a link.
// ---------------------------------------------------------------------------------------------

/// Where on the screen the first link in the preview is, worked out from the layout rather than guessed:
/// a hard-coded position is a test that fails the day the font or the heading sizes change, and it would
/// fail by clicking on the wrong words rather than by saying so.
fn where_the_first_link_is(harness: &mut Harness<'static, UnluminousApp>) -> egui::Pos2 {
    // **Scrolled to first.** In `MARKDOWN` the link is 835 points down a page drawn in a pane 638 points
    // tall, so without this the point worked out below is off the bottom of the window and the click
    // lands on nothing — which fails as "no browser tab opened" rather than as "the test aimed wrongly".
    let links = harness.state().preview_links();
    let link = links.first().expect("MARKDOWN has a link in it").clone();
    let wanted = harness.state().preview_layout().caret_at(link.bytes.start).y;
    let area = harness.state().editor_area();
    harness.state_mut().files.active_mut().preview_scroll = (wanted - area.height() / 2.0).max(0.0);
    harness.run();

    let links = harness.state().preview_links();
    let link = links.first().expect("MARKDOWN has a link in it");
    let scroll = harness.state().files.active().preview_scroll;
    // The middle of its first character, in the preview's own coordinates, plus where the preview's text
    // starts on the screen. `caret_at` is what the editing area uses to place a caret at an offset.
    let caret = harness.state().preview_layout().caret_at(link.bytes.start);
    let area = harness.state().editor_area();
    use unluminous_app::theme::size::{EDITOR_PADDING_X, EDITOR_PADDING_Y};
    egui::Pos2::new(
        // A few points into the word rather than at the very edge of its first character, so a rounding
        // difference cannot put the point one byte before the link starts.
        area.left() + EDITOR_PADDING_X + caret.x + 4.0,
        area.top() + EDITOR_PADDING_Y - scroll + caret.y + caret.height / 2.0,
    )
}

fn click_at_with(
    harness: &mut Harness<'static, UnluminousApp>,
    at: egui::Pos2,
    modifiers: Modifiers,
) {
    // A move on its own first, so the frame that reads the click has already had the pointer settle —
    // which is what a person's hand does and what makes the hover the click is decided against real.
    // The modifier is carried by a key press, because `RawInput` has no modifier state of its own: egui
    // works it out from the events, so a held key has to be one.
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    hold(harness, modifiers);
    harness.run();
    hold(harness, modifiers);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
    harness.run();
    // Let go, or every later frame in the test is still holding the key.
    hold(harness, Modifiers::default());
    harness.run();
}

/// Put the modifier keys down for the frame about to be run.
///
/// **`Event::ModifiersChanged` is the only thing that sets `InputState::modifiers`**, and it persists
/// across frames until something changes it back — which is exactly what a held key does. A `Key` event
/// carrying modifiers does *not* do it: egui reads the modifiers off a `Key` event only to match that one
/// press against a shortcut, so a frame that saw one still reports `Modifiers::NONE` from
/// `input().modifiers`, which is what `Ctrl/Cmd+Click` is asked about. Measured: the frame saw
/// `Modifiers::NONE` and the pointer stayed a text cursor.
fn hold(harness: &mut Harness<'static, UnluminousApp>, modifiers: Modifiers) {
    harness.input_mut().events.push(egui::Event::ModifiersChanged(modifiers));
}

/// The preview reports its links, which is what everything else here rests on.
#[test]
fn the_preview_reports_where_its_links_are_and_where_they_go() {
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let links = harness.state().preview_links();
    assert_eq!(links.len(), 1, "MARKDOWN has one link in it");
    assert_eq!(links[0].target, "https://example.com/design");
    let text = harness.state().preview_text();
    assert_eq!(&text[links[0].bytes.clone()], "the design", "the words, not the address");
}

/// **`Ctrl/Cmd+Click` opens it in a browser tab.** The modifier is Go to Definition's, which is the rule
/// `task-1696` set: modifier held means "take me to the thing this names".
#[test]
fn command_clicking_a_link_opens_a_browser_tab() {
    if !unluminous_app::services::browser::SUPPORTED {
        return; // A platform with no web view refuses with a sentence, which is the test below.
    }
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let before = harness.state().files.len();
    let at = where_the_first_link_is(&mut harness);
    click_at_with(&mut harness, at, Modifiers::COMMAND);
    assert_eq!(harness.state().files.len(), before + 1, "a tab was opened");
    let opened = harness.state().files.active();
    assert!(
        opened.browser.is_some(),
        "and it is a browser tab rather than a file, showing {:?}",
        opened.path()
    );
}

/// **A plain click keeps the meaning it already had**, which in a preview is placing a selection. A
/// person reading a page has to be able to click in it without a browser opening.
#[test]
fn a_plain_click_on_a_link_only_moves_the_selection() {
    let mut harness = harness(MARKDOWN);
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let before = harness.state().files.len();
    let at = where_the_first_link_is(&mut harness);
    click_at(&mut harness, at);
    assert_eq!(harness.state().files.len(), before, "no tab was opened");
    assert!(harness.state().is_reading_the_preview(), "the click was the preview's");
}

/// **A scheme that is not `http` or `https` is refused by name.** A document must not be able to reach
/// this machine because somebody clicked a word in it — the rule `services::preview_images` already
/// keeps about a picture with a scheme in it.
#[test]
fn a_javascript_link_is_refused_rather_than_opened() {
    let mut harness = harness("A [trap](javascript:alert(1)) in a document.\n");
    harness.get_by_label("Markdown preview").click();
    harness.run();
    assert_eq!(harness.state().preview_links().len(), 1, "it is read as a link");
    let before = harness.state().files.len();
    let at = where_the_first_link_is(&mut harness);
    click_at_with(&mut harness, at, Modifiers::COMMAND);
    assert_eq!(harness.state().files.len(), before, "and nothing was opened");
    let said = harness.state().message.clone().unwrap_or_default();
    assert!(
        said.contains("javascript"),
        "the refusal names the scheme rather than being silent, said {said:?}"
    );
}

/// A `file:` link is the other half of the same refusal, and it is the one that would read somebody's
/// disk. Named separately so that allowing one could not quietly allow the other.
#[test]
fn a_file_link_is_refused_rather_than_opened() {
    let mut harness = harness("A [local file](file:///etc/passwd) in a document.\n");
    harness.get_by_label("Markdown preview").click();
    harness.run();
    let before = harness.state().files.len();
    let at = where_the_first_link_is(&mut harness);
    click_at_with(&mut harness, at, Modifiers::COMMAND);
    assert_eq!(harness.state().files.len(), before);
    assert!(harness.state().message.clone().unwrap_or_default().contains("file"));
}

/// The pointer says a link is a link before it is clicked. Without it, a link is underlined text and the
/// feature is one nobody finds — and the hand has to be set **after** the text cursor the whole page
/// asks for, or it is set and immediately replaced.
///
/// **What is asserted is the ordering, not the cursor.** `egui_kittest` never reports the preview pane as
/// hovered: measured, the pane's own `CursorIcon::Text` does not appear either, at the middle of the pane
/// or anywhere in it, however many frames the pointer is held still for — the harness feeds pointer
/// events but the response's `hovered()` stays false, which is a limit of driving egui offscreen rather
/// than anything about this feature. The click works because `clicked()` does not go through `hovered()`.
///
/// So this reads the source: the call that sets the hand has to come **after** the one that sets the text
/// cursor, because egui keeps the last cursor asked for in a frame. The other way round the hand is set
/// and then replaced, which is a link that cannot be seen and can still be clicked. It is verified in a
/// real window, which is layer 4.
#[test]
fn the_hand_cursor_is_set_after_the_text_cursor_so_it_wins_over_a_link() {
    // **`app/preview.rs`, not `app/mod.rs`.** `task-1922` split that file into sixteen, and this test
    // reads source text rather than state, so the day the two calls moved it stopped asserting
    // anything -- it would have failed on the `expect`, which is what it did. Both calls are in one
    // file, which is what makes comparing their positions mean anything at all.
    let source = include_str!("../src/app/preview.rs");
    let sets_the_text_cursor = source
        .find("painter_ui.ctx().set_cursor_icon(egui::CursorIcon::Text);")
        .expect("the preview sets a text cursor over its words");
    let reads_the_link = source
        .find("self.open_a_link_in_the_preview(&painter_ui, &response, origin);")
        .expect("the preview reads the link under the pointer");
    assert!(
        sets_the_text_cursor < reads_the_link,
        "the hand is set after the text cursor, or egui replaces it in the same frame"
    );
}

// -------------------------------------------------------------------------------------- task-1922
//
// The `Find Action` palette. WP4's answer to the question `unluminous-cli action list` already
// answers — what can this window be asked to do — put in front of a person: one box, a list that
// narrows as you type, and Enter.

#[test]
fn the_palette_is_built_from_the_real_menus_and_narrows_as_it_is_typed_into() {
    let mut harness = harness_in(&sample_folder());
    choose(&mut harness, Action::CommandPalette);
    let all = harness.state().palette.as_ref().expect("the palette is open").results().len();
    assert!(all > 40, "it opens on everything the menus hold, which is {all} rows");

    // A subsequence rather than a substring, which is `file_search`'s own rule — the same scorer,
    // not a second one written for names.
    //
    // **`tln` shows the tiering rather than one winner.** `Go to Line...` matches it in the
    // *wording*, which is what is scored first because it is what is on the screen;
    // `Show Line Numbers` holds no `t` at all and matches only through the entry's own **name**,
    // `toggle-line-numbers`, which is the third tier and is worth less. So both are offered and the
    // one whose label matched is first. An earlier version of this test asserted the other way
    // round and had never been run.
    let palette = harness.state_mut().palette.as_mut().expect("still open");
    palette.query = "tln".to_owned();
    palette.refresh();
    let names: Vec<String> = harness
        .state()
        .palette
        .as_ref()
        .unwrap()
        .results()
        .iter()
        .map(|row| row.command.name.clone())
        .collect();
    assert!(names.len() < all, "typing narrows it: {} of {all}", names.len());
    assert_eq!(names.first().map(String::as_str), Some("go-to-line"), "{names:?}");
    assert!(names.iter().any(|name| name == "toggle-line-numbers"), "{names:?}");
}

#[test]
fn a_row_the_palette_runs_goes_through_the_same_function_a_menu_row_does() {
    let mut harness = harness_in(&sample_folder());
    let was = harness.state().settings.line_numbers;
    choose(&mut harness, Action::CommandPalette);
    let palette = harness.state_mut().palette.as_mut().expect("the palette is open");
    // **The wording, because that is what the palette scores first and what a person reads** -- and
    // `line numbers` rather than either whole wording, because this entry's row says `Hide Line
    // Numbers` while they are on and `Show Line Numbers` while they are off. A query naming one of
    // them would pass or fail on the state the harness happened to start in.
    //
    // Before it was ever run this test asked for `toggle line numbers`, which matches nothing at
    // all: the entry's own name holds no spaces and neither wording holds a `t`.
    palette.query = "line numbers".to_owned();
    palette.refresh();
    let chosen = harness.state().palette.as_ref().unwrap().chosen_command().cloned();
    let chosen = chosen.expect("a row is chosen");
    assert_eq!(chosen.name, "toggle-line-numbers");
    let ctx = harness.ctx.clone();
    harness.state_mut().run_a_palette_row(chosen, &ctx);
    harness.run();
    assert_eq!(harness.state().settings.line_numbers, !was);
}

#[test]
fn a_command_that_cannot_be_used_is_shown_dimmed_and_refused_with_the_reason() {
    // Somebody looking for `Redo` wants to be told there is nothing to redo, not told there is no
    // such command — so the row is listed, drawn dimmed, and running it says why.
    let mut harness = harness_in(&sample_folder());
    choose(&mut harness, Action::CommandPalette);
    let palette = harness.state_mut().palette.as_mut().expect("the palette is open");
    palette.query = "redo".to_owned();
    palette.refresh();
    let chosen = harness
        .state()
        .palette
        .as_ref()
        .unwrap()
        .results()
        .iter()
        .find(|row| row.command.name == "redo")
        .map(|row| row.command.clone())
        .expect("Redo is offered even though it cannot be used");
    assert!(!chosen.enabled);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_a_palette_row(chosen, &ctx);
    harness.run();
    let said = harness.state().message.clone().unwrap_or_default();
    assert!(said.contains("Redo") && said.contains("Edit"), "{said}");
}

#[test]
fn the_palette_offers_the_entries_wp4_added_and_names_their_chords() {
    // The machinery's own promise: an entry added to a menu is in the palette with no list anywhere
    // to add it to. These are the seven WP4 put on `Edit` and the one it put on `Find`.
    let folder = fixture("unluminous-1922-palette", &[("main.rs", "fn one() {}\n")]);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open main.rs --permanent");
    // Read through `action list`, which since WP4 is the **same walk of the menus** the palette
    // draws from — one list rather than two that agree today — and which is not cut to a page.
    let listed = did(&mut harness, "action list");
    let rows: Vec<(String, String)> = listed["actions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["name"].as_str().unwrap_or_default().to_owned(),
                row["shortcut"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    for (name, chord) in [
        ("toggle-line-comment", "Ctrl+Slash"),
        ("toggle-block-comment", "Ctrl+Shift+Slash"),
        ("duplicate-line", "Ctrl+Shift+D"),
        ("join-lines", "Ctrl+Shift+J"),
        ("go-to-line", "Ctrl+Shift+L"),
        ("go-to-matching-bracket", "Ctrl+Shift+Backslash"),
        ("reopen-closed-tab", "Ctrl+Shift+T"),
        ("command-palette", "Ctrl+Shift+A"),
    ] {
        let found = rows.iter().find(|(had, _)| had == name);
        assert!(found.is_some(), "{name} should be on a menu: {rows:?}");
        // The chord is spelled the way the menu spells it, which is what the palette shows on the
        // right of the row. Written for Windows, where `command` is the control key.
        if cfg!(not(target_os = "macos")) {
            assert_eq!(found.unwrap().1, chord, "{name}");
        }
    }
}

// ==============================================================================================
// task-1922 §5.4: no action is a silent no operation
//
// `UnluminousApp::run_action` is the one place a menu row, a key chord or `unluminous-cli action
// run` turns into a change, and nothing asked whether each of its arms does anything at all. The
// walk below runs every value of every [`Action`] against a window put into the state that action
// needs, and fails when the window came out of it unchanged **and** with nothing new in the status
// bar. Unluminous's own rule is that a control which cannot apply is absent and one that cannot be
// used just now is dimmed and says why; one that quietly does nothing is neither.
//
// **Exhaustive by construction.** Rust has no reflection, so the list of actions is written out by
// hand -- and [`variant_name`] beside it is an exhaustive `match` over `&Action` with no wildcard
// arm, so a variant added to `Action` stops this file compiling until somebody writes a line for
// it. [`EVERY_VARIANT`] is the set of names that match can return, and the walk fails while a name
// in it is neither driven nor named on [`CANNOT_BE_DRIVEN`]. The four sub enums that carry a menu's
// worth of entries of their own are held the same way, because each of their values is a distinct
// menu row rather than one row with a parameter.
//
// **What counts as having done something** is a signature of the whole window. `status --json` is
// most of it -- the editor, the tabs, the panes, the panels, the explorer, the terminal, who holds
// the keyboard, the modal that is open, the settings, git, the project and the status bar
// **message** -- so one comparison covers both halves of the rule at once. The read-only commands
// beside it answer for what `status` does not carry: the collapsed blocks, the marked passages, the
// canvas, the runs, the debugger and the breakpoints. The last few values no command answers for at
// all are read off the window itself.
//
// A signature that differs for a reason other than the action is harmless, because the walk only
// fails when nothing differed. A signature too narrow to see what an action really changed is the
// fault that matters, and is why it is as wide as it is.

use std::path::PathBuf;
use std::sync::OnceLock;

use unluminous_app::app::actions::{
    DebugAction, FoldAction, GitAction, HighlightColor, RunAction, SpaceAction,
};
use unluminous_app::app::dock::{Panel, Side};
use unluminous_app::services::space::Kind;

/// Every variant of [`Action`], by the name [`variant_name`] answers with.
///
/// Written out because Rust cannot be asked. What keeps it in step with the enum is the exhaustive
/// match below, which stops this file compiling when a variant is added, and the walk, which fails
/// while a name here is neither driven nor excluded.
const EVERY_VARIANT: &[&str] = &[
    "NewWindow",
    "OpenFolder",
    "OpenFile",
    "OpenWebAddress",
    "OpenInBrowser",
    "GoToFile",
    "FindInFiles",
    "CheckForUpdates",
    "Find",
    "Replace",
    "FindNext",
    "FindPrevious",
    "GoToDefinition",
    "FindReferences",
    "RenameSymbol",
    "CompleteWord",
    "NavigateBack",
    "NavigateForward",
    "ToggleLineComment",
    "ToggleBlockComment",
    "DuplicateLines",
    "MoveLines",
    "JoinLines",
    "SortLines",
    "TrimTrailingWhitespace",
    "GoToLine",
    "GoToMatchingBracket",
    "CommandPalette",
    "ReopenClosedTab",
    "OpenRecent",
    "ForgetRecent",
    "Save",
    "SaveAs",
    "CloseWindow",
    "Settings",
    "Undo",
    "Redo",
    "Cut",
    "Copy",
    "Paste",
    "SelectAll",
    "SetViewMode",
    "ToggleExplorer",
    "ToggleEditor",
    "ToggleMaximisedPane",
    "ToggleLineNumbers",
    "ChangeFontSize",
    "ResetFontSize",
    "ToggleTerminal",
    "ToggleRunTile",
    "ToggleDebugTile",
    "Dock",
    "ResetPanelLayout",
    "Space",
    "Run",
    "Debug",
    "CloseTab",
    "NextTab",
    "PreviousTab",
    "SplitRight",
    "MoveTabRight",
    "MoveTabLeft",
    "Unsplit",
    "UnsplitAll",
    "NextPane",
    "PreviousPane",
    "SelectOpenFile",
    "NewTerminalTab",
    "CloseTerminalTab",
    "RenameTerminalTab",
    "NewFile",
    "NewFolder",
    "CutPath",
    "CopyPath",
    "CopyPathReference",
    "PasteInto",
    "RenamePath",
    "DeletePath",
    "RevealPath",
    "ReloadPath",
    "Git",
    "Highlight",
    "ClearHighlight",
    "ClearHighlights",
    "Fold",
    "About",
    "Quit",
    "PluginPane",
    "PluginCommand",
    "PluginTab",
];

/// The name of the variant `action` is.
///
/// **No wildcard arm**, which is the whole reason it exists: a variant added to [`Action`] fails to
/// compile here until somebody writes a line for it, and the line they write is the reminder to add
/// the action to [`every_step`] or to [`CANNOT_BE_DRIVEN`].
fn variant_name(action: &Action) -> &'static str {
    match action {
        Action::NewWindow => "NewWindow",
        Action::OpenFolder => "OpenFolder",
        Action::OpenFile => "OpenFile",
        Action::OpenWebAddress => "OpenWebAddress",
        Action::OpenInBrowser(_) => "OpenInBrowser",
        Action::GoToFile => "GoToFile",
        Action::FindInFiles => "FindInFiles",
        Action::CheckForUpdates => "CheckForUpdates",
        Action::Find => "Find",
        Action::Replace => "Replace",
        Action::FindNext => "FindNext",
        Action::FindPrevious => "FindPrevious",
        Action::GoToDefinition => "GoToDefinition",
        Action::FindReferences => "FindReferences",
        Action::RenameSymbol => "RenameSymbol",
        Action::CompleteWord => "CompleteWord",
        Action::NavigateBack => "NavigateBack",
        Action::NavigateForward => "NavigateForward",
        Action::ToggleLineComment => "ToggleLineComment",
        Action::ToggleBlockComment => "ToggleBlockComment",
        Action::DuplicateLines => "DuplicateLines",
        Action::MoveLines { .. } => "MoveLines",
        Action::JoinLines => "JoinLines",
        Action::SortLines => "SortLines",
        Action::TrimTrailingWhitespace => "TrimTrailingWhitespace",
        Action::GoToLine => "GoToLine",
        Action::GoToMatchingBracket => "GoToMatchingBracket",
        Action::CommandPalette => "CommandPalette",
        Action::ReopenClosedTab => "ReopenClosedTab",
        Action::OpenRecent(_) => "OpenRecent",
        Action::ForgetRecent => "ForgetRecent",
        Action::Save => "Save",
        Action::SaveAs => "SaveAs",
        Action::CloseWindow => "CloseWindow",
        Action::Settings => "Settings",
        Action::Undo => "Undo",
        Action::Redo => "Redo",
        Action::Cut => "Cut",
        Action::Copy => "Copy",
        Action::Paste => "Paste",
        Action::SelectAll => "SelectAll",
        Action::SetViewMode(_) => "SetViewMode",
        Action::ToggleExplorer => "ToggleExplorer",
        Action::ToggleEditor => "ToggleEditor",
        Action::ToggleMaximisedPane => "ToggleMaximisedPane",
        Action::ToggleLineNumbers => "ToggleLineNumbers",
        Action::ChangeFontSize { .. } => "ChangeFontSize",
        Action::ResetFontSize => "ResetFontSize",
        Action::ToggleTerminal => "ToggleTerminal",
        Action::ToggleRunTile => "ToggleRunTile",
        Action::ToggleDebugTile => "ToggleDebugTile",
        Action::Dock { .. } => "Dock",
        Action::ResetPanelLayout => "ResetPanelLayout",
        Action::Space(_) => "Space",
        Action::Run(_) => "Run",
        Action::Debug(_) => "Debug",
        Action::CloseTab => "CloseTab",
        Action::NextTab => "NextTab",
        Action::PreviousTab => "PreviousTab",
        Action::SplitRight => "SplitRight",
        Action::MoveTabRight => "MoveTabRight",
        Action::MoveTabLeft => "MoveTabLeft",
        Action::Unsplit => "Unsplit",
        Action::UnsplitAll => "UnsplitAll",
        Action::NextPane => "NextPane",
        Action::PreviousPane => "PreviousPane",
        Action::SelectOpenFile => "SelectOpenFile",
        Action::NewTerminalTab => "NewTerminalTab",
        Action::CloseTerminalTab => "CloseTerminalTab",
        Action::RenameTerminalTab => "RenameTerminalTab",
        Action::NewFile(_) => "NewFile",
        Action::NewFolder(_) => "NewFolder",
        Action::CutPath(_) => "CutPath",
        Action::CopyPath(_) => "CopyPath",
        Action::CopyPathReference(_) => "CopyPathReference",
        Action::PasteInto(_) => "PasteInto",
        Action::RenamePath(_) => "RenamePath",
        Action::DeletePath(_) => "DeletePath",
        Action::RevealPath(_) => "RevealPath",
        Action::ReloadPath(_) => "ReloadPath",
        Action::Git(_) => "Git",
        Action::Highlight(_) => "Highlight",
        Action::ClearHighlight => "ClearHighlight",
        Action::ClearHighlights => "ClearHighlights",
        Action::Fold(_) => "Fold",
        Action::About => "About",
        Action::Quit => "Quit",
        Action::PluginPane { .. } => "PluginPane",
        Action::PluginCommand { .. } => "PluginCommand",
        Action::PluginTab { .. } => "PluginTab",
    }
}

/// Every entry on the Git menu, by name.
const EVERY_GIT: &[&str] = &[
    "Commit",
    "Add",
    "ShowDiff",
    "CompareWithRevision",
    "ShowHistory",
    "ShowCurrentRevision",
    "Rollback",
    "Annotate",
    "Push",
    "Pull",
    "Fetch",
    "Merge",
    "Rebase",
    "Continue",
    "Abort",
    "Branches",
    "Switch",
    "NewBranch",
    "NewTag",
    "ResetHead",
    "Stash",
    "Unstash",
    "Remotes",
    "Clone",
    "Exclude",
    "Refresh",
];

fn git_variant_name(action: &GitAction) -> &'static str {
    match action {
        GitAction::Commit => "Commit",
        GitAction::Add(_) => "Add",
        GitAction::ShowDiff(_) => "ShowDiff",
        GitAction::CompareWithRevision(_) => "CompareWithRevision",
        GitAction::ShowHistory(_) => "ShowHistory",
        GitAction::ShowCurrentRevision => "ShowCurrentRevision",
        GitAction::Rollback(_) => "Rollback",
        GitAction::Annotate => "Annotate",
        GitAction::Push => "Push",
        GitAction::Pull => "Pull",
        GitAction::Fetch => "Fetch",
        GitAction::Merge => "Merge",
        GitAction::Rebase => "Rebase",
        GitAction::Continue => "Continue",
        GitAction::Abort => "Abort",
        GitAction::Branches => "Branches",
        GitAction::Switch(_) => "Switch",
        GitAction::NewBranch => "NewBranch",
        GitAction::NewTag => "NewTag",
        GitAction::ResetHead => "ResetHead",
        GitAction::Stash => "Stash",
        GitAction::Unstash => "Unstash",
        GitAction::Remotes => "Remotes",
        GitAction::Clone => "Clone",
        GitAction::Exclude => "Exclude",
        GitAction::Refresh => "Refresh",
    }
}

/// Everything the Run menu and the run widget can ask for.
const EVERY_RUN: &[&str] = &["Start", "Stop", "Rerun", "Select", "CurrentFile", "Edit"];

fn run_variant_name(action: &RunAction) -> &'static str {
    match action {
        RunAction::Start(_) => "Start",
        RunAction::Stop(_) => "Stop",
        RunAction::Rerun(_) => "Rerun",
        RunAction::Select(_) => "Select",
        RunAction::CurrentFile => "CurrentFile",
        RunAction::Edit => "Edit",
    }
}

/// Everything the Run menu's debug half, the debug tile and the gutter's menu can ask for.
const EVERY_DEBUG: &[&str] = &[
    "Start",
    "CurrentFile",
    "Stop",
    "Resume",
    "StepOver",
    "StepInto",
    "StepOut",
    "Pause",
    "RunToCursor",
    "ToggleBreakpoint",
    "EditBreakpoint",
    "ToggleBreakpointEnabled",
    "ShowValue",
    "EvaluateExpression",
    "ToggleTile",
    "InstallAdapter",
];

fn debug_variant_name(action: &DebugAction) -> &'static str {
    match action {
        DebugAction::Start(_) => "Start",
        DebugAction::CurrentFile => "CurrentFile",
        DebugAction::Stop => "Stop",
        DebugAction::Resume => "Resume",
        DebugAction::StepOver => "StepOver",
        DebugAction::StepInto => "StepInto",
        DebugAction::StepOut => "StepOut",
        DebugAction::Pause => "Pause",
        DebugAction::RunToCursor => "RunToCursor",
        DebugAction::ToggleBreakpoint => "ToggleBreakpoint",
        DebugAction::EditBreakpoint => "EditBreakpoint",
        DebugAction::ToggleBreakpointEnabled => "ToggleBreakpointEnabled",
        DebugAction::ShowValue => "ShowValue",
        DebugAction::EvaluateExpression => "EvaluateExpression",
        DebugAction::ToggleTile => "ToggleTile",
        DebugAction::InstallAdapter(_) => "InstallAdapter",
    }
}

/// Everything the canvas's own menus can ask for.
const EVERY_SPACE: &[&str] = &[
    "Toggle",
    "Add",
    "Fit",
    "OpenAddModal",
    "NewView",
    "Manage",
    "RenameView",
    "DuplicateView",
    "DeleteView",
    "RenameNode",
    "CloseNode",
    "ChooseFolder",
    "RestartNode",
    "ResumeSession",
    "StartWhatWasRunning",
    "Disconnect",
    "CarryLines",
];

fn space_variant_name(action: &SpaceAction) -> &'static str {
    match action {
        SpaceAction::Toggle => "Toggle",
        SpaceAction::Add(_) => "Add",
        SpaceAction::Fit => "Fit",
        SpaceAction::OpenAddModal => "OpenAddModal",
        SpaceAction::NewView => "NewView",
        SpaceAction::Manage => "Manage",
        SpaceAction::RenameView => "RenameView",
        SpaceAction::DuplicateView => "DuplicateView",
        SpaceAction::DeleteView => "DeleteView",
        SpaceAction::RenameNode => "RenameNode",
        SpaceAction::CloseNode => "CloseNode",
        SpaceAction::ChooseFolder => "ChooseFolder",
        SpaceAction::RestartNode => "RestartNode",
        SpaceAction::ResumeSession => "ResumeSession",
        SpaceAction::StartWhatWasRunning => "StartWhatWasRunning",
        SpaceAction::Disconnect => "Disconnect",
        SpaceAction::CarryLines(_) => "CarryLines",
    }
}

/// The actions no test can run, and why each one cannot.
///
/// `task-1922` §5.4 asks for each to be named rather than for anything to be quietly skipped, and
/// the walk fails when an action on this list is stepped anyway, so the list cannot grow stale in
/// the direction that hides something. Each name is read back through `Action::from_name`, so a
/// name that stops being an action fails here too.
const CANNOT_BE_DRIVEN: &[(&str, &str)] = &[
    ("new-window", "it starts a second Unluminous process"),
    ("open-recent", "it starts a second Unluminous process on the project it names"),
    (
        "open-folder",
        "it opens the platform's own folder chooser and waits for somebody to click in it",
    ),
    ("open-file", "it opens the platform's own file chooser and waits for somebody to click in it"),
    ("save-as", "it opens the platform's own save dialog and waits for somebody to click in it"),
    (
        "space-choose-folder",
        "it opens the platform's own folder chooser and waits for somebody to click in it",
    ),
    ("quit", "it closes the window, and the walk has the rest of the actions still to run"),
    ("close-window", "the same: it closes the window"),
    (
        "reveal-path",
        "it hands the path to the operating system's own file manager, which opens a window on \
         this machine",
    ),
    (
        "check-for-updates",
        "it asks GitHub for the latest release, and a test does not reach the network",
    ),
    ("debug-install", "it downloads and installs a debug adapter onto this machine"),
    (
        "paste",
        "it reads the machine's own clipboard, and whether it does anything at all is decided by \
         what somebody left on it; a test cannot set that without taking the clipboard away from \
         whoever is using the machine",
    ),
];

/// Which window an action needs behind it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Where {
    /// A project holding a Rust file with blocks in it, a Markdown file and a local page.
    Project,
    /// The same project, in a real git repository.
    Repository,
    /// A debug session stopped at line 4 with three locals in scope, fed the messages a real
    /// adapter would have sent.
    Paused,
    /// A project whose own settings folder holds a manifest that contributes a tab, because no
    /// plugin that ships contributes one.
    ContributedTab,
}

type Ready = fn(&mut Harness<'static, UnluminousApp>);

/// One action, and what the window needs before it can apply.
struct Step {
    action: Action,
    /// Command lines run before the action, after the shared preparation.
    lines: Vec<String>,
    /// The same, for what no command line can set up.
    ready: Option<Ready>,
    place: Where,
}

impl Step {
    fn new(action: Action) -> Self {
        Step { action, lines: Vec::new(), ready: None, place: Where::Project }
    }

    fn after(mut self, lines: &[&str]) -> Self {
        self.lines = lines.iter().map(|line| (*line).to_owned()).collect();
        self
    }

    fn ready_with(mut self, ready: Ready) -> Self {
        self.ready = Some(ready);
        self
    }

    fn at(mut self, place: Where) -> Self {
        self.place = place;
        self
    }
}

/// The project the walk runs in.
///
/// `src/main.rs` holds a function called from another, a comment, two blocks that can be collapsed,
/// a line ending in whitespace and a bracket whose partner is four lines down -- between them, what
/// a dozen of the actions below need in order to have something to do.
fn walk_folder() -> PathBuf {
    fixture(
        "unluminous-1922-actions",
        &[
            ("readme.md", "# Readme\n\nSome prose, in a file that has a preview.\n"),
            ("page.html", "<h1>A page</h1>\n"),
            (
                "src/main.rs",
                "fn helper(value: usize) -> usize {\n    // a comment\n    let total = value + 1;\n    total   \n}\n\nfn main() {\n    let answer = helper(2);\n    println!(\"{answer}\");\n}\n",
            ),
            ("src/other.rs", "pub fn other() {}\n"),
            ("docs/one.md", "# One\n"),
        ],
    )
}

/// A real repository for the Git menu, built once rather than once an action.
///
/// `git_folder` rebuilds its repository on every call, which is right for a test that changes one
/// and wrong for twenty six windows that each want a repository to act on. What they do to it
/// between them does not matter: each needs *a* repository with a committed file and an uncommitted
/// change in it, and every one of them gets a window of its own on this one.
fn walk_repository() -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| git_folder("unluminous-1922-actions")).clone()
}

/// A project whose settings folder contributes a tab, which no plugin that ships does.
fn walk_tab_folder() -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let folder = copy_out_of_the_repository(&walk_folder(), "unluminous-1922-actions-tab");
        let plugin = folder.join(".unluminous-settings").join("plugins").join("agent-tasks");
        std::fs::create_dir_all(&plugin).expect("a plugin folder");
        std::fs::write(
            plugin.join("plugin.conf"),
            "plugin.id = agent-tasks\nplugin.name = Agent-Tasks\nplugin.kind = ui\n\
             ui.provider = agent-tasks\ntab.id = board\ntab.label = Agent-Tasks\n",
        )
        .expect("a manifest that contributes a tab");
        folder
    })
    .clone()
}

/// A window of the kind this step needs, with nothing done to it yet.
fn window_for(place: Where) -> Harness<'static, UnluminousApp> {
    match place {
        Where::Project => harness_in(&walk_folder()),
        Where::Repository => {
            let mut harness = harness_in(&walk_repository());
            settle_the_walk(&mut harness, Where::Repository);
            harness
        }
        Where::Paused => paused_harness("unluminous-1922-actions"),
        Where::ContributedTab => {
            let folder = walk_tab_folder();
            let mut harness = harness_in(&folder);
            harness.state_mut().use_store(unluminous_app::services::store::Store::at(
                folder.join(".unluminous-settings"),
            ));
            for _ in 0..4 {
                harness.step();
            }
            harness
        }
    }
}

/// What every window of this kind has done to it before the step's own preparation.
fn shared_preparation(place: Where) -> &'static [&'static str] {
    match place {
        // `version.ts` holds changes git has not been told about, so the entries that stage, diff
        // and roll one back have something to work on.
        Where::Repository => &["tab open version.ts --permanent", "window message walking"],
        Where::Paused => &["window message walking"],
        _ => &[
            "tab open src/main.rs --permanent",
            "editor select --from-line 3 --to-line 3",
            "window message walking",
        ],
    }
}

/// Run a command line and take what it answered, without insisting that it worked.
///
/// The walk's preparation is allowed to be refused -- a line that cannot apply to this window has
/// not left it in a worse state than it was -- and the signature's own reads are asked of windows
/// holding a picture, a page or nothing, where some of them are correctly refused. What is compared
/// is the answer, so a refusal is part of the signature rather than a failure.
fn ask(harness: &mut Harness<'static, UnluminousApp>, line: &str) -> serde_json::Value {
    let ctx = harness.ctx.clone();
    let answered = harness.state_mut().run_command_line(line, &ctx);
    harness.step();
    match answered {
        Some(reply) => serde_json::json!({ "ok": reply.ok, "result": reply.result }),
        // A command that holds its answer open for a later frame. Nothing the signature reads does,
        // and a preparation line that did has still been dispatched.
        None => serde_json::Value::Null,
    }
}

/// What the window has asked to be put on the system clipboard.
///
/// Read straight after the action rather than as part of the signature, because the commands are
/// drained into each frame's output and the signature draws a dozen frames of its own. `Copy`,
/// `Cut` and `Copy Path` change nothing else at all, which is what it is here for.
fn copied_by_the_window(harness: &Harness<'static, UnluminousApp>) -> String {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Draw a few frames, and wait for git when there is a repository behind the window.
///
/// Every entry on the Git menu that is not a dialog sends a request to `unluminous_git::Worker`,
/// which answers on a thread, so what it did reaches the window some frames later. Everything else
/// here has finished by the time `run_action` returns.
///
/// **Quiet for several turns rather than quiet once.** `GitState::is_busy` reads
/// `Worker::running()`, which the worker thread sets when it picks a request up -- so for the first
/// moments after a request is sent the worker is not running anything yet and the window looks
/// settled. Waiting for one quiet turn read the repository back before git had touched it, which is
/// what made `git add`, `git fetch`, `git merge --continue` and `git merge --abort` all look like
/// entries that do nothing at all.
///
/// It answers with **every line the status bar showed while it waited**, not only the one left at
/// the end, because a git command's own answer does not survive the settling: `Reply::Done` puts
/// git's words in the status bar and asks for the repository and the log to be read again, and
/// `Reply::Log` clears the status bar when it arrives. So `Add`, `Fetch`, `Continue` and `Abort`
/// each say what git said for a few frames and then say nothing, and a window read afterwards looks
/// exactly like a window nothing happened to. A sentence that reached the status bar is the second
/// half of the rule whether or not it is still there a moment later.
fn settle_the_walk(harness: &mut Harness<'static, UnluminousApp>, place: Where) -> Vec<String> {
    let mut said: Vec<String> = Vec::new();
    match place {
        Where::Repository => {
            let mut quiet = 0;
            for _ in 0..400 {
                pump(harness);
                note_the_status_bar(harness, &mut said);
                match harness.state().git.as_ref().is_some_and(|git| git.is_busy()) {
                    true => quiet = 0,
                    false => quiet += 1,
                }
                if quiet >= 10 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            for _ in 0..3 {
                harness.step();
                note_the_status_bar(harness, &mut said);
            }
        }
        _ => {
            for _ in 0..4 {
                harness.step();
                note_the_status_bar(harness, &mut said);
            }
        }
    }
    said
}

/// Write down what the status bar is showing, if it is not already written down.
///
/// Both lines, because they are two different sentences: `UnluminousApp::message` is the window's
/// own and `GitState::message` is whatever the git worker last answered with.
fn note_the_status_bar(harness: &Harness<'static, UnluminousApp>, said: &mut Vec<String>) {
    let state = harness.state();
    let line = format!(
        "{:?} / {:?}",
        state.message,
        state.git.as_ref().and_then(|git| git.message.clone())
    );
    if !said.contains(&line) {
        said.push(line);
    }
}

/// The whole window as one value, which is what deciding whether an action did anything compares.
fn signature(harness: &mut Harness<'static, UnluminousApp>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for line in [
        "status --json",
        "fold list --json",
        "highlight list --all --json",
        "space view --json",
        "space connections --json",
        "run list --json",
        "run status --json",
        "debug status --json",
        "debug breakpoint list --json",
        "terminal list --json",
        "plugins list --json",
        // The whole of what git says about the repository, which `status`'s own `git` section
        // narrows to a branch and a count -- so staging a file, which leaves the count where it
        // was, is a change only this one can see.
        "git status --json",
        "editor text",
    ] {
        parts.push(format!("{line} -> {}", ask(harness, line)));
    }
    // And the values no command answers for. Each is here because an action below changes it and
    // nothing above would have shown it.
    let state = harness.state();
    // Whether the editing area is showing, which `status` does not answer: its `panels` are
    // `dock::Panel::ALL`, and the editing area is not one of them -- it is what the panels leave.
    parts.push(format!("editing area showing -> {}", state.editor_visible));
    // And whether the tab that is showing is annotated with blame, which is the whole of what
    // `Annotate` does and which nothing else here would see.
    parts.push(format!("blame -> {}", state.files.active().blame.is_some()));
    parts.push(format!("explorer clipboard -> {:?}", state.clipboard));
    parts.push(format!("find bar -> {:?}", state.find));
    parts.push(format!("references or rename -> {}", state.references.is_some()));
    parts.push(format!("recent projects -> {:?}", state.recent));
    parts.push(format!("explorer selection -> {:?}", state.selected));
    parts.push(format!("run configurations dialog -> {:?}", state.run_dialog));
    parts.push(format!("expression box -> {}", state.evaluate.is_some()));
    parts.push(format!("breakpoint dialog -> {}", state.breakpoint_dialog.is_some()));
    parts.push(format!("add a node -> {}", state.space.adding.is_some()));
    parts.push(format!("manage the canvases -> {}", state.space.managing.is_some()));
    parts.push(format!("in hand -> {:?}", state.space.in_hand));
    parts.push(format!("closing -> {}", state.closing));
    parts.push(format!("completion list -> {}", state.completion().is_some()));
    parts.push(format!("value tooltip -> {}", state.value_tooltip_is_open()));
    for pane in ["agent-tasks/board", "agent-chat/chat"] {
        parts.push(format!("{pane} -> {}", showing(harness, pane)));
    }
    parts.join("\n")
}

// ------------------------------------------------------------------ what each action needs first

fn a_canvas_with_a_chosen_node(harness: &mut Harness<'static, UnluminousApp>) {
    let node = harness.state_mut().new_detached_space_node(Kind::Editor, egui::pos2(40.0, 40.0));
    ask(harness, &format!("space focus {node}"));
}

/// Two nodes, and the camera moved off them, so putting every node on the screen is a change.
fn a_canvas_the_camera_is_off(harness: &mut Harness<'static, UnluminousApp>) {
    harness.state_mut().new_detached_space_node(Kind::Editor, egui::pos2(40.0, 40.0));
    harness.state_mut().new_detached_space_node(Kind::Folder, egui::pos2(600.0, 320.0));
    for _ in 0..4 {
        harness.step();
    }
    ask(harness, "space camera --x 900 --y 900");
}

/// Two terminal nodes with a wire between them, and the wire in hand -- which is what a right click
/// on one puts there, and what each of the three entries on a wire's own menu reads.
fn a_wire_in_hand(harness: &mut Harness<'static, UnluminousApp>) {
    hold_a_wire(harness, false);
}

/// The same, with the wire already carrying lines, so turning that off is a change.
fn a_wire_in_hand_carrying_lines(harness: &mut Harness<'static, UnluminousApp>) {
    hold_a_wire(harness, true);
}

fn hold_a_wire(harness: &mut Harness<'static, UnluminousApp>, carrying: bool) {
    // Detached, because a terminal node started the ordinary way runs a real shell.
    let from = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    let to = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(600.0, 40.0));
    let pipe = if carrying { " --pipe lines" } else { "" };
    ask(harness, &format!("space connect {from} {to}{pipe}"));
    let wires = ask(harness, "space connections --json");
    harness.state_mut().space.in_hand.wire =
        wires["result"]["connections"][0]["connection"].as_u64();
}

fn a_terminal_tab(harness: &mut Harness<'static, UnluminousApp>) {
    harness.state_mut().new_detached_terminal_tab(20, 60);
    harness.step();
}

fn a_project_that_has_been_open_before(harness: &mut Harness<'static, UnluminousApp>) {
    harness.state_mut().recent = vec![PathBuf::from("/a/project")];
}

fn a_path_held_to_be_pasted(harness: &mut Harness<'static, UnluminousApp>) {
    let ctx = harness.ctx.clone();
    let readme = walk_folder().join("readme.md");
    harness.state_mut().run_action(Action::CopyPath(readme), &ctx);
    harness.step();
}

fn a_breakpoint_on_the_caret_line(harness: &mut Harness<'static, UnluminousApp>) {
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Debug(DebugAction::ToggleBreakpoint), &ctx);
    harness.step();
}

fn a_run_that_has_started(harness: &mut Harness<'static, UnluminousApp>) {
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Run(RunAction::Start(None)), &ctx);
    for _ in 0..20 {
        harness.step();
    }
}

/// The file on disk changed underneath the tab, so reading it again is a change.
fn the_file_changed_underneath(harness: &mut Harness<'static, UnluminousApp>) {
    std::fs::write(walk_folder().join("src/other.rs"), "pub fn other() { let changed = 1; }\n")
        .expect("rewrite src/other.rs");
    harness.step();
}

fn a_jump_to_go_back_from(harness: &mut Harness<'static, UnluminousApp>) {
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::GoToDefinition, &ctx);
    harness.step();
}

fn a_jump_that_has_been_gone_back_from(harness: &mut Harness<'static, UnluminousApp>) {
    a_jump_to_go_back_from(harness);
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::NavigateBack, &ctx);
    harness.step();
}

// ------------------------------------------------------------------------------- the whole walk

/// Every action the walk runs, with what its window needs first.
fn every_step() -> Vec<Step> {
    let folder = walk_folder();
    let readme = folder.join("readme.md");
    let mut steps: Vec<Step> = vec![
        Step::new(Action::OpenWebAddress),
        Step::new(Action::OpenInBrowser(folder.join("page.html"))),
        Step::new(Action::GoToFile),
        Step::new(Action::FindInFiles),
        Step::new(Action::Find),
        Step::new(Action::Replace),
        Step::new(Action::FindNext).after(&["editor find total"]),
        Step::new(Action::FindPrevious).after(&["editor find total"]),
        // The caret on the call to `helper`, which is the one name in this file that is defined
        // somewhere else in it.
        Step::new(Action::GoToDefinition).after(&["editor caret --line 8 --column 20"]),
        Step::new(Action::FindReferences).after(&["editor caret --line 8 --column 20"]),
        Step::new(Action::RenameSymbol).after(&["editor caret --line 8 --column 20"]),
        // Part way through a word, so there is a stem with something longer to offer, and with the
        // editing area holding the keyboard, which is what `Complete Word` asks about first.
        Step::new(Action::CompleteWord)
            .after(&["pane focus 0", "editor caret --line 4 --column 8"]),
        Step::new(Action::NavigateBack).ready_with(a_jump_to_go_back_from),
        Step::new(Action::NavigateForward).ready_with(a_jump_that_has_been_gone_back_from),
        Step::new(Action::ToggleLineComment),
        Step::new(Action::ToggleBlockComment),
        Step::new(Action::DuplicateLines),
        Step::new(Action::MoveLines { down: true }),
        Step::new(Action::MoveLines { down: false }),
        Step::new(Action::JoinLines),
        // Three lines that are not in order, so sorting them really moves one.
        Step::new(Action::SortLines).after(&["editor select --from-line 7 --to-line 9"]),
        Step::new(Action::TrimTrailingWhitespace),
        Step::new(Action::GoToLine),
        // Beside the brace that opens `fn main`, whose partner is three lines down.
        Step::new(Action::GoToMatchingBracket).after(&["editor caret --line 7 --column 12"]),
        Step::new(Action::CommandPalette),
        Step::new(Action::ReopenClosedTab)
            .after(&["tab open src/other.rs --permanent", "tab close"]),
        Step::new(Action::ForgetRecent).ready_with(a_project_that_has_been_open_before),
        Step::new(Action::Save).after(&["editor insert x"]),
        Step::new(Action::Settings),
        Step::new(Action::Undo).after(&["editor insert x"]),
        Step::new(Action::Redo).after(&["editor insert x", "editor undo"]),
        Step::new(Action::Cut),
        Step::new(Action::Copy),
        Step::new(Action::SelectAll),
        // The three view modes are about a file that has a preview, and the raw one is where a tab
        // opens, so it is switched away from first or it would be asked to do what is already done.
        Step::new(Action::SetViewMode(ViewMode::Raw))
            .after(&["tab open readme.md --permanent", "editor view preview"]),
        Step::new(Action::SetViewMode(ViewMode::SideBySide))
            .after(&["tab open readme.md --permanent"]),
        Step::new(Action::SetViewMode(ViewMode::Preview))
            .after(&["tab open readme.md --permanent"]),
        Step::new(Action::ToggleExplorer),
        Step::new(Action::ToggleEditor),
        Step::new(Action::ToggleMaximisedPane),
        Step::new(Action::ToggleLineNumbers),
        Step::new(Action::ChangeFontSize { larger: true }),
        Step::new(Action::ChangeFontSize { larger: false }),
        Step::new(Action::ResetFontSize).after(&["settings set appearance.font.size 20"]),
        Step::new(Action::ToggleTerminal),
        Step::new(Action::ToggleRunTile),
        Step::new(Action::ToggleDebugTile),
        Step::new(Action::ResetPanelLayout).after(&["panel dock terminal left"]),
        Step::new(Action::CloseTab),
        Step::new(Action::NextTab).after(&["tab open src/other.rs --permanent"]),
        Step::new(Action::PreviousTab).after(&["tab open src/other.rs --permanent"]),
        Step::new(Action::SplitRight).after(&["tab open src/other.rs --permanent"]),
        // A second pane, with the keyboard put back in the first, so there is one to the right.
        Step::new(Action::MoveTabRight).after(&[
            "tab open src/other.rs --permanent",
            "pane split",
            "pane focus 0",
        ]),
        Step::new(Action::MoveTabLeft).after(&["tab open src/other.rs --permanent", "pane split"]),
        Step::new(Action::Unsplit).after(&["tab open src/other.rs --permanent", "pane split"]),
        Step::new(Action::UnsplitAll).after(&["tab open src/other.rs --permanent", "pane split"]),
        Step::new(Action::NextPane).after(&["tab open src/other.rs --permanent", "pane split"]),
        Step::new(Action::PreviousPane).after(&["tab open src/other.rs --permanent", "pane split"]),
        // Put away and filtered, which is the state the entry exists to get out of.
        Step::new(Action::SelectOpenFile).after(&["explorer hide", "explorer filter zzz"]),
        Step::new(Action::NewTerminalTab),
        Step::new(Action::CloseTerminalTab).ready_with(a_terminal_tab),
        Step::new(Action::RenameTerminalTab).ready_with(a_terminal_tab),
        Step::new(Action::NewFile(folder.join("docs"))),
        Step::new(Action::NewFolder(folder.join("docs"))),
        Step::new(Action::CutPath(readme.clone())),
        Step::new(Action::CopyPath(readme.clone())),
        Step::new(Action::CopyPathReference(readme.clone())),
        Step::new(Action::PasteInto(folder.join("docs"))).ready_with(a_path_held_to_be_pasted),
        Step::new(Action::RenamePath(readme.clone())),
        Step::new(Action::DeletePath(readme.clone())),
        Step::new(Action::ReloadPath(folder.join("src/other.rs")))
            .after(&["tab open src/other.rs --permanent"])
            .ready_with(the_file_changed_underneath),
        Step::new(Action::ClearHighlight).after(&[
            "highlight add --from-line 3 --to-line 3",
            "editor caret --line 3 --column 10",
        ]),
        Step::new(Action::ClearHighlights).after(&["highlight add --from-line 3 --to-line 3"]),
        Step::new(Action::About),
        Step::new(Action::PluginPane { pane: "agent-tasks/board".to_owned() }),
        // A plugin command that changes the window rather than one that answers with data: this is
        // the half of the plugin contract Unluminous owns, and it is what puts a pane on the screen.
        Step::new(Action::PluginCommand {
            plugin: "agent-tasks".to_owned(),
            command: "open-pane".to_owned(),
        }),
        Step::new(Action::PluginTab { tab: "agent-tasks/board".to_owned() })
            .at(Where::ContributedTab),
        // The debug half that needs no session: both of these are refusals in the status bar, which
        // is the other half of the rule.
        Step::new(Action::Debug(DebugAction::Start(None))),
        Step::new(Action::Debug(DebugAction::CurrentFile)),
        Step::new(Action::Debug(DebugAction::ToggleBreakpoint)),
        Step::new(Action::Debug(DebugAction::EditBreakpoint))
            .ready_with(a_breakpoint_on_the_caret_line),
        Step::new(Action::Debug(DebugAction::ToggleBreakpointEnabled))
            .ready_with(a_breakpoint_on_the_caret_line),
        Step::new(Action::Debug(DebugAction::ToggleTile)),
        // And the half that needs a program stopped somewhere.
        Step::new(Action::Debug(DebugAction::Stop)).at(Where::Paused),
        Step::new(Action::Debug(DebugAction::Resume)).at(Where::Paused),
        Step::new(Action::Debug(DebugAction::StepOver)).at(Where::Paused),
        Step::new(Action::Debug(DebugAction::StepInto)).at(Where::Paused),
        Step::new(Action::Debug(DebugAction::StepOut)).at(Where::Paused),
        Step::new(Action::Debug(DebugAction::Pause)).at(Where::Paused),
        Step::new(Action::Debug(DebugAction::RunToCursor)).at(Where::Paused),
        Step::new(Action::Debug(DebugAction::ShowValue))
            .at(Where::Paused)
            .after(&["editor caret --line 4 --column 12"]),
        Step::new(Action::Debug(DebugAction::EvaluateExpression)).at(Where::Paused),
    ];

    // The Run menu, against a configuration that prints a word and stops.
    let a_configuration = ["run add echoing cmd /c echo hello", "run select echoing"];
    for action in
        [RunAction::Start(None), RunAction::Rerun(None), RunAction::CurrentFile, RunAction::Edit]
    {
        steps.push(Step::new(Action::Run(action)).after(&a_configuration));
    }
    steps.push(
        Step::new(Action::Run(RunAction::Stop(None)))
            .after(&a_configuration)
            .ready_with(a_run_that_has_started),
    );
    // Choosing one that is not already chosen, or the entry would correctly do nothing.
    steps.push(Step::new(Action::Run(RunAction::Select("echoing".to_owned()))).after(&[
        "run add echoing cmd /c echo hello",
        "run add other cmd /c echo other",
        "run select other",
    ]));

    // The Git menu, against a real repository.
    for action in GitAction::ALL {
        steps.push(Step::new(Action::Git(action.clone())).at(Where::Repository));
    }
    // The one entry that is not on a menu: the branch flyout's row, which names the branch it is
    // about. The branch that is already checked out, so nothing reaches a remote or moves a file.
    steps.push(Step::new(Action::Git(GitAction::Switch("main".to_owned()))).at(Where::Repository));

    // The canvas.
    steps.push(Step::new(Action::Space(SpaceAction::Toggle)).after(&["space show"]));
    for kind in Kind::ALL {
        steps.push(Step::new(Action::Space(SpaceAction::Add(kind))).after(&["space show"]));
    }
    for action in [
        SpaceAction::OpenAddModal,
        SpaceAction::NewView,
        SpaceAction::Manage,
        SpaceAction::RenameView,
        SpaceAction::DuplicateView,
    ] {
        steps.push(Step::new(Action::Space(action)).after(&["space show"]));
    }
    steps.push(
        Step::new(Action::Space(SpaceAction::Fit))
            .after(&["space show"])
            .ready_with(a_canvas_the_camera_is_off),
    );
    // The last view cannot be deleted, so there are two.
    steps.push(
        Step::new(Action::Space(SpaceAction::DeleteView))
            .after(&["space show", "space new-view Second"]),
    );
    for action in [
        SpaceAction::RenameNode,
        SpaceAction::CloseNode,
        SpaceAction::RestartNode,
        SpaceAction::ResumeSession,
        SpaceAction::StartWhatWasRunning,
    ] {
        steps.push(
            Step::new(Action::Space(action))
                .after(&["space show"])
                .ready_with(a_canvas_with_a_chosen_node),
        );
    }
    steps.push(
        Step::new(Action::Space(SpaceAction::Disconnect))
            .after(&["space show"])
            .ready_with(a_wire_in_hand),
    );
    steps.push(
        Step::new(Action::Space(SpaceAction::CarryLines(true)))
            .after(&["space show"])
            .ready_with(a_wire_in_hand),
    );
    steps.push(
        Step::new(Action::Space(SpaceAction::CarryLines(false)))
            .after(&["space show"])
            .ready_with(a_wire_in_hand_carrying_lines),
    );

    // The four colours the editor's right click menu offers, over the selected line.
    for colour in HighlightColor::ALL {
        steps.push(Step::new(Action::Highlight(colour)));
    }

    // The six things that can be done to a block. Two of them are about blocks that are already
    // collapsed, and one falls back to the selection when nothing is marked -- so it is given a
    // mark, which is what the entry is named after.
    for action in FoldAction::ALL {
        let step = Step::new(Action::Fold(action));
        steps.push(match action {
            FoldAction::None_ | FoldAction::ExpandRecursively => {
                step.after(&["fold collapse --all"])
            }
            FoldAction::Others => step.after(&["highlight add --from-line 3 --to-line 3"]),
            _ => step,
        });
    }

    // A panel to each of the four edges. Moved somewhere else first, because docking a panel to the
    // side it is already on is a row that would correctly do nothing.
    for panel in Panel::ALL {
        for side in Side::ALL {
            let elsewhere = if side == Side::Left { Side::Right } else { Side::Left };
            let line = format!("panel dock {} {}", panel.name(), elsewhere.name());
            steps.push(Step::new(Action::Dock { panel, side }).after(&[line.as_str()]));
        }
    }

    steps
}

/// Every `Action` runs, and every one of them changes the window or says why it cannot.
///
/// `task-1922` §5.4. Read the section comment above this one for what the signature is and why the
/// exclusion list is written down rather than skipped.
#[test]
fn every_action_changes_the_window_or_says_why_it_cannot() {
    let steps = every_step();
    let mut faults: Vec<String> = Vec::new();

    // A path, because a dozen of the actions below are about a file and `from_name` keeps the one
    // it is given. Which file does not matter here: none of the excluded ones is run.
    let a_path = walk_folder().join("readme.md");
    let excluded: Vec<Action> = CANNOT_BE_DRIVEN
        .iter()
        .map(|(name, _)| {
            Action::from_name(name, Some(a_path.clone())).unwrap_or_else(|| {
                panic!("`{name}` is on CANNOT_BE_DRIVEN and is not the name of an action")
            })
        })
        .collect();

    // The list cannot go stale in the direction that hides something: an action that is excluded
    // and stepped anyway is a fault rather than a skip.
    for step in &steps {
        let name = step.action.name();
        if let Some((_, reason)) = CANNOT_BE_DRIVEN.iter().find(|(listed, _)| *listed == name) {
            faults.push(format!(
                "`{name}` is on CANNOT_BE_DRIVEN -- {reason} -- and the walk runs it anyway. Take \
                 it off the list."
            ));
        }
    }
    let mut named: Vec<String> = Vec::new();
    for step in &steps {
        let name = step.action.name();
        if named.contains(&name) {
            faults.push(format!("two steps are both called `{name}`"));
        }
        named.push(name);
    }

    // Every variant of `Action` and of the four sub enums is either walked or excluded, and nothing
    // walked names a variant the lists above have not heard of.
    let every: Vec<&Action> =
        steps.iter().map(|step| &step.action).chain(excluded.iter()).collect();
    fn covered(names: &[&str], seen: Vec<&'static str>, what: &str, faults: &mut Vec<String>) {
        for name in names {
            if !seen.contains(name) {
                faults.push(format!(
                    "{what}::{name} is neither walked nor on CANNOT_BE_DRIVEN. Add a step for it, \
                     or add it to the list with the reason it cannot be run."
                ));
            }
        }
        for name in &seen {
            if !names.contains(name) {
                faults.push(format!("{what}::{name} is walked and is not in the list beside it"));
            }
        }
    }
    covered(
        EVERY_VARIANT,
        every.iter().map(|action| variant_name(action)).collect(),
        "Action",
        &mut faults,
    );
    covered(
        EVERY_GIT,
        every
            .iter()
            .filter_map(|action| match action {
                Action::Git(what) => Some(git_variant_name(what)),
                _ => None,
            })
            .collect(),
        "GitAction",
        &mut faults,
    );
    covered(
        EVERY_RUN,
        every
            .iter()
            .filter_map(|action| match action {
                Action::Run(what) => Some(run_variant_name(what)),
                _ => None,
            })
            .collect(),
        "RunAction",
        &mut faults,
    );
    covered(
        EVERY_DEBUG,
        every
            .iter()
            .filter_map(|action| match action {
                Action::Debug(what) => Some(debug_variant_name(what)),
                _ => None,
            })
            .collect(),
        "DebugAction",
        &mut faults,
    );
    covered(
        EVERY_SPACE,
        every
            .iter()
            .filter_map(|action| match action {
                Action::Space(what) => Some(space_variant_name(what)),
                _ => None,
            })
            .collect(),
        "SpaceAction",
        &mut faults,
    );

    // Then the walk itself. A window of its own for each, because an action that leaves a modal
    // open, a tab closed or a panel somewhere else is the next action's starting state otherwise,
    // and a walk whose preparation quietly stopped working would report faults that are not there.
    let began = std::time::Instant::now();
    for step in &steps {
        let name = step.action.name();
        let mut harness = window_for(step.place);
        for line in shared_preparation(step.place) {
            ask(&mut harness, line);
        }
        for line in &step.lines {
            ask(&mut harness, line);
        }
        if let Some(ready) = step.ready {
            ready(&mut harness);
        }
        let said_before = settle_the_walk(&mut harness, step.place);

        let before = signature(&mut harness);
        let ctx = harness.ctx.clone();
        harness.state_mut().run_action(step.action.clone(), &ctx);
        harness.step();
        let copied = copied_by_the_window(&harness);
        let said_after = settle_the_walk(&mut harness, step.place);
        let after = signature(&mut harness);

        if before == after && said_before == said_after && copied.is_empty() {
            faults.push(format!(
                "`{name}` on a {:?} window changed nothing and said nothing. Either it is a silent \
                 no operation, or the window it was given was not one it can apply to -- the step \
                 for it is `{:?}`.",
                step.place, step.lines
            ));
        }
    }

    println!(
        "{} actions walked in {:.1}s, {} of them excluded",
        steps.len(),
        began.elapsed().as_secs_f32(),
        CANNOT_BE_DRIVEN.len()
    );
    assert!(faults.is_empty(), "{} faults:\n  {}", faults.len(), faults.join("\n  "));
}
