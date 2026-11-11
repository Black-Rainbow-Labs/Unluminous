//! The terminal along the bottom, and the run tile beside it.
//!
//! The terminal's own tabs — added, shown, renamed, dragged and closed — what it draws of colour,
//! bold and a program that takes over the screen, what it is told when its tile is resized, and one
//! test that really types into a real shell and waits for the answer. Then the run tile, which is
//! the terminal tile's sibling: the widget in the title bar, the flyout, the dialog, what a run that
//! ended says it ended with, and the run configurations driven from the command line.
//!
//! **Almost every picture here is drawn from a detached session** — one with no program behind it,
//! fed fixed bytes — because when a real program answers is not something a test can know, and a
//! picture that depended on it would differ between runs.
//!
//! **16 of the 37 tests here take a picture**, and the rest read the window's state back.

mod common;

use common::*;

use egui::{vec2, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, SnapshotResults};
use unluminous_app::app::actions::DebugAction;
use unluminous_app::app::actions::{Action, RunAction};
use unluminous_app::components::title_bar::MenuPlacement;
use unluminous_app::services::run_configurations::{Configuration, RunConfigurations};
use unluminous_app::UnluminousApp;

#[test]
fn the_terminal_opens_along_the_bottom_and_shows_what_a_program_wrote() {
    let mut harness = with_terminal("A document above the terminal.", 12, 80);
    assert!(harness.state().terminal.visible);
    feed(
        &mut harness,
        b"jason.mcaffee@unluminous ~ % cargo test\r\n   Compiling unluminous-terminal v0.1.0\r\n    Finished in 1.26s\r\n",
    );
    let screen = harness.state().terminal.tabs.active().expect("a tab").snapshot();
    assert!(screen.contains("Compiling unluminous-terminal"), "the output should be on the screen");
    harness.snapshot(shot("terminal"));
}

#[test]
fn the_terminal_draws_colour_bold_and_the_other_attributes() {
    let mut harness = with_terminal("", 12, 80);
    let mut bytes = Vec::new();
    // The eight ordinary colours, then the eight bright ones, then the attributes.
    for code in 30..38 {
        bytes.extend_from_slice(format!("\x1b[{code}m colour{code} ").as_bytes());
    }
    bytes.extend_from_slice(b"\x1b[0m\r\n");
    for code in 90..98 {
        bytes.extend_from_slice(format!("\x1b[{code}m bright{code} ").as_bytes());
    }
    bytes.extend_from_slice(b"\x1b[0m\r\n");
    bytes.extend_from_slice(
        b"\x1b[1mbold\x1b[0m \x1b[3mitalic\x1b[0m \x1b[4munderline\x1b[0m \x1b[9mstruck\x1b[0m \x1b[7minverse\x1b[0m \x1b[2mdim\x1b[0m\r\n",
    );
    bytes.extend_from_slice(
        b"\x1b[48;5;24m background \x1b[0m \x1b[38;2;255;120;0mtrue colour\x1b[0m\r\n",
    );
    feed(&mut harness, &bytes);
    harness.snapshot(shot("terminal_colours"));
}

/// `task-2514`: everything Termoil's emulator knows about a cell that the painter used to drop. The five underline
/// styles and a colored underline, an overline, a box drawn with line characters whose rows join, block and shade
/// characters, braille, accents over their letters, emoji in color, an `OSC 8` link and a picture sent with the
/// kitty protocol.
#[test]
fn the_terminal_draws_what_the_emulator_knows_about_each_cell() {
    let mut harness = with_terminal("", 14, 80);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        b"\x1b[4:1msingle\x1b[0m \x1b[4:2mdouble\x1b[0m \x1b[4:3mcurly\x1b[0m \x1b[4:4mdotted\x1b[0m \x1b[4:5mdashed\x1b[0m ",
    );
    bytes.extend_from_slice(b"\x1b[4:3;58;2;255;60;60mred curl\x1b[0m \x1b[53moverline\x1b[0m\r\n");
    bytes.extend_from_slice("\u{256D}\u{2500}\u{2500}\u{2500}\u{2500}\u{256E} \u{2588}\u{2593}\u{2592}\u{2591} \u{2580}\u{2584}\u{258C}\u{2590} \u{28FF}\u{2847}\u{28B8}\r\n".as_bytes());
    bytes.extend_from_slice("\u{2502}box \u{2502} n\u{0303} a\u{0302} e\u{0301} \u{1F600}\u{2705}\u{1F680} \u{2764}\u{FE0F}\r\n".as_bytes());
    bytes.extend_from_slice("\u{2570}\u{2500}\u{2500}\u{2500}\u{2500}\u{256F} ".as_bytes());
    bytes.extend_from_slice(
        b"\x1b]8;;https://unluminous.com/\x1b\\a link\x1b]8;;\x1b\\ after it\r\n",
    );
    // Four by two pixels, red over blue, placed over three columns and one row.
    bytes.extend_from_slice(b"\x1b_Ga=T,f=32,s=4,v=2,c=6,r=2,q=2;");
    let mut pixels = Vec::new();
    for row in 0..2 {
        for _ in 0..4 {
            pixels.extend_from_slice(if row == 0 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    bytes.extend_from_slice(base64_of(&pixels).as_bytes());
    bytes.extend_from_slice(b"\x1b\\\r\n\r\nafter the picture\r\n");
    feed(&mut harness, &bytes);
    let screen = harness.state().terminal.tabs.active().expect("a tab").snapshot();
    assert_eq!(screen.images.len(), 1, "the picture is on the screen");
    assert_eq!(screen.links, vec!["https://unluminous.com/".to_owned()]);
    harness.snapshot(shot("terminal_emulator_cells"));
}

/// Standard base64, which the kitty protocol carries its pixels in.
fn base64_of(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let value = chunk
            .iter()
            .enumerate()
            .fold(0_u32, |acc, (index, byte)| acc | (*byte as u32) << (16 - 8 * index));
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(ALPHABET[(value >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn the_terminal_draws_a_program_that_takes_over_the_screen() {
    let mut harness = with_terminal("", 14, 80);
    // What a full screen program draws: the alternate screen, box drawing characters, and text placed by
    // moving the cursor rather than by printing lines in order.
    let mut bytes = b"\x1b[?1049h\x1b[H".to_vec();
    bytes.extend_from_slice("\u{250c}".as_bytes());
    for _ in 0..40 {
        bytes.extend_from_slice("\u{2500}".as_bytes());
    }
    bytes.extend_from_slice("\u{2510}".as_bytes());
    for row in 2..8 {
        bytes.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        bytes.extend_from_slice("\u{2502}".as_bytes());
        bytes.extend_from_slice(format!("\x1b[{row};42H").as_bytes());
        bytes.extend_from_slice("\u{2502}".as_bytes());
    }
    bytes.extend_from_slice(b"\x1b[8;1H");
    bytes.extend_from_slice("\u{2514}".as_bytes());
    for _ in 0..40 {
        bytes.extend_from_slice("\u{2500}".as_bytes());
    }
    bytes.extend_from_slice("\u{2518}".as_bytes());
    bytes.extend_from_slice(b"\x1b[3;4H\x1b[1;36mA program drawing its own screen\x1b[0m");
    bytes.extend_from_slice("\x1b[5;4H\u{25b6} one\x1b[6;4H  two".as_bytes());
    feed(&mut harness, &bytes);
    assert!(
        harness.state().terminal.tabs.active().expect("a tab").on_alternate_screen(),
        "the program should be on its own screen"
    );
    harness.snapshot(shot("terminal_full_screen"));
}

#[test]
fn a_terminal_tab_comes_back_in_its_folder_showing_what_was_on_it() {
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-tab-replay");
    let inside = folder.join("chapters");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    steady(&mut harness);

    // A detached tab, so what is drawn and what is written down do not depend on a shell answering.
    harness.state_mut().new_detached_terminal_tab(10, 60);
    feed(&mut harness, b"$ ls\r\ntotal 48\r\nsrc  tests  Cargo.toml\r\n$ ");
    steady(&mut harness);

    // What the window would write on its way out.
    harness.state_mut().write_the_tab_screens_down();
    let saved = unluminous_app::services::realm::store::screen_path(
        &folder,
        unluminous_app::services::realm::store::Screen::Tab(0),
    );
    assert!(saved.is_file(), "the tab's screen was written down at {}", saved.display());
    assert!(
        String::from_utf8_lossy(&std::fs::read(&saved).expect("the screen reads back"))
            .contains("Cargo.toml"),
        "and it holds what was on the screen"
    );

    // And that there is a screen for the tab to be started with, which is the same shim a node uses: a
    // screen written into the emulator from outside is erased by the console host, `task-1912`.
    let printing = unluminous_app::services::realm::store::a_screen_to_print(
        &folder,
        unluminous_app::services::realm::store::Screen::Tab(0),
    )
    .expect("there is a screen to print");
    assert_eq!(printing, saved);

    // The folder half: what a project remembers about a tab, and what it opens the tab in.
    let mut state = unluminous_app::services::project_state::load(&folder);
    state.terminal_visible = true;
    state.terminal_tabs = 1;
    state.terminals = vec![unluminous_app::services::project_state::RememberedTerminal {
        name: "build".to_owned(),
        folder: inside.to_string_lossy().into_owned(),
    }];
    unluminous_app::services::project_state::save(&folder, &state);

    let mut second = harness_in(&folder);
    second.state_mut().restore_project();
    steady(&mut second);
    second.state_mut().start_the_restored_terminals();
    steady(&mut second);
    assert_eq!(second.state().terminal.tabs.count(), 1, "the tab came back");
    assert_eq!(
        second.state().terminal.tabs.at(0).and_then(unluminous_terminal::Session::given_name),
        Some("build"),
        "with the name somebody typed"
    );
    assert_eq!(
        second.state().terminal.tabs.settings.working_directory.as_deref(),
        Some(inside.as_path()),
        "and opened in the folder its shell was in rather than the project's root"
    );
}

/// A second tab is added, and the new one is the one showing.
#[test]
fn a_second_terminal_tab_is_added_and_shown_in_front() {
    let mut harness = with_terminal("", 10, 60);
    feed(&mut harness, b"the first tab");
    harness.state_mut().new_detached_terminal_tab(10, 60);
    steady(&mut harness);
    feed(&mut harness, b"the second tab");
    assert_eq!(harness.state().terminal.tabs.count(), 2);
    assert_eq!(harness.state().terminal.tabs.active_index(), 1);
    let screen = harness.state().terminal.tabs.active().expect("a tab").snapshot();
    assert!(screen.contains("the second tab"));
    harness.snapshot(shot("terminal_tabs"));

    // Going back to the first tab shows what was in it, so a tab keeps its own screen.
    harness.get_by_label("Terminal tab: detached").click();
    steady(&mut harness);
    assert_eq!(harness.state().terminal.tabs.active_index(), 0);
    let screen = harness.state().terminal.tabs.active().expect("a tab").snapshot();
    assert!(screen.contains("the first tab"), "the first tab kept its screen");
}

#[test]
fn a_terminal_tab_is_renamed_from_its_own_menu() {
    let mut harness = with_terminal("", 8, 60);
    harness.state_mut().new_detached_terminal_tab(8, 60);
    steady(&mut harness);
    assert_eq!(harness.state().terminal.tabs.names(), vec!["detached", "detached 2"]);

    // Opened through the window's own state, as the gutter's and a file tab's menus are, because
    // the harness cannot press the right mouse button. To the right of the strip, so the picture
    // holds the tabs the menu is about as well as the menu.
    let at = harness.state().terminal.grid_area().left_top() + vec2(420.0, -14.0);
    harness.state_mut().terminal_menu = Some((at, 1));
    steady(&mut harness);
    harness.get_by_label("Rename...");
    harness.get_by_label("New Terminal Tab");
    harness.snapshot(shot("terminal_tab_menu"));

    // Choosing it puts the menu away and opens the prompt, seeded with what the tab is called now.
    harness.state_mut().terminal_menu = None;
    choose(&mut harness, Action::RenameTerminalTab);
    assert_eq!(harness.state().prompt.as_ref().expect("the prompt is open").value, "detached 2",);
    if let Some(prompt) = harness.state_mut().prompt.as_mut() {
        prompt.value = "the build".to_owned();
    }
    let prompt = harness.state_mut().prompt.take().expect("a prompt");
    harness.state_mut().run_prompt_for_test(prompt);
    harness.state_mut().prompt = None;
    steady(&mut harness);
    assert_eq!(harness.state().terminal.tabs.names(), vec!["detached", "the build"]);

    // And the name a person typed is not taken away again by the program setting a title of its
    // own, which is the whole reason it is held apart from the title.
    feed(&mut harness, b"\x1b]0;claude\x07");
    assert_eq!(harness.state().terminal.tabs.names(), vec!["detached", "the build"]);
    harness.snapshot(shot("terminal_tab_renamed"));
}

#[test]
fn a_terminal_tab_is_dragged_along_the_strip() {
    let mut harness = with_terminal("", 8, 60);
    harness.state_mut().new_detached_terminal_tab(8, 60);
    steady(&mut harness);
    did(&mut harness, "terminal rename --tab 0 first");
    did(&mut harness, "terminal rename --tab 1 second");
    steady(&mut harness);
    assert_eq!(harness.state().terminal.tabs.names(), vec!["first", "second"]);

    // The first tab dragged past the middle of the second, which is where a drop lands after it.
    let from = harness.get_by_label("Terminal tab: first").rect().center();
    let onto = harness.get_by_label("Terminal tab: second").rect();
    let to = egui::pos2(onto.right() - 2.0, onto.center().y);
    let modifiers = Modifiers::default();
    harness.input_mut().events.push(egui::Event::PointerMoved(from));
    steady(&mut harness);
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    steady(&mut harness);
    harness.input_mut().events.push(egui::Event::PointerMoved(to));
    steady(&mut harness);
    // Held, so the picture shows the tab outlined in the air and the accent mark saying where it
    // would land. It is the same mark the file tabs draw, from the same two functions.
    harness.snapshot(shot("terminal_tab_dragging"));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    steady(&mut harness);
    assert_eq!(
        harness.state().terminal.tabs.names(),
        vec!["second", "first"],
        "the tab that was dragged is now the second one"
    );
    assert_eq!(harness.state().terminal.tabs.active_index(), 1, "and it is the one showing");
    harness.snapshot(shot("terminal_tabs_rearranged"));
}

#[test]
fn dragging_a_terminal_tab_and_the_command_line_are_the_same_rearrangement() {
    let mut harness = with_terminal("", 8, 60);
    for name in ["one", "two", "three"] {
        harness.state_mut().new_detached_terminal_tab(8, 60);
        steady(&mut harness);
        let last = harness.state().terminal.tabs.count() - 1;
        did(&mut harness, &format!("terminal rename --tab {last} {name}"));
    }
    // `terminal move` counts the tabs as they are on the screen, exactly as `tab move` does, so
    // moving the last one to the front is position 0.
    did(&mut harness, "terminal move --tab 3 0");
    assert_eq!(harness.state().terminal.tabs.names(), vec!["three", "detached", "one", "two"]);
    assert_eq!(harness.state().terminal.tabs.active_index(), 0);

    // An empty name puts a tab back to being named after the program in it, which is the one thing
    // the dialog cannot ask for, because its button needs a name in the field.
    did(&mut harness, "terminal rename --tab 0");
    assert_eq!(harness.state().terminal.tabs.names()[0], "detached");
}

#[test]
fn closing_the_last_terminal_tab_puts_the_tile_away() {
    let mut harness = with_terminal("", 8, 60);
    assert!(harness.state().terminal.visible);
    harness.get_by_label_contains("Close terminal detached").click();
    steady(&mut harness);
    assert_eq!(harness.state().terminal.tabs.count(), 0);
    assert!(!harness.state().terminal.visible, "with no terminals there is nothing to show");
}

#[test]
fn the_terminal_is_told_the_new_size_when_the_tile_is_dragged() {
    let mut harness = with_terminal("", 12, 80);
    feed(&mut harness, b"before the resize");
    let tall = harness.state().terminal.tabs.active().expect("a tab").size();
    let results = &mut SnapshotResults::new();
    results.add(harness.try_snapshot(shot("terminal_tall")));

    // Drag the tile's top edge downwards, which makes it shorter.
    let handle = harness.get_by_label("Resize terminal").rect();
    let from = handle.center();
    drag(&mut harness, from, egui::pos2(from.x, from.y + 90.0));

    let short = harness.state().terminal.tabs.active().expect("a tab").size();
    assert!(
        short.rows < tall.rows,
        "a shorter tile holds fewer rows: {} then {}",
        tall.rows,
        short.rows
    );
    assert_eq!(short.columns, tall.columns, "its width did not change");
    assert!(
        harness
            .state()
            .terminal
            .tabs
            .active()
            .expect("a tab")
            .snapshot()
            .contains("before the resize"),
        "and what was written is still there"
    );
    results.add(harness.try_snapshot(shot("terminal_short")));
    report(std::mem::replace(results, SnapshotResults::new()));
}

#[test]
fn the_terminal_font_size_changes_the_size_of_the_grid() {
    let mut harness = with_terminal("", 12, 80);
    feed(&mut harness, b"a line at the bigger size\r\n\x1b[32mand a green one\x1b[0m");
    let before = harness.state().terminal.tabs.active().expect("a tab").size();
    let mut settings = harness.state().settings.clone();
    settings.terminal_font_size = 20.0;
    harness.state_mut().set_settings(settings);
    steady(&mut harness);
    let after = harness.state().terminal.tabs.active().expect("a tab").size();
    assert!(
        after.columns < before.columns && after.rows <= before.rows,
        "a bigger font means fewer cells fit: {before:?} then {after:?}"
    );
    harness.snapshot(shot("terminal_large_font"));
}

/// Why the terminal has no tab, as far as the terminal can say.
///
/// **`task-2110`.** Both tests below once failed in a full suite run finding no tab at all, and the
/// failure said only that: a shell that could not be started and a shell that started and stopped at
/// once both leave an empty strip, and they call for different things to go and look at.
fn why_there_is_no_tab(harness: &Harness<'static, UnluminousApp>) -> String {
    let tabs = &harness.state().terminal.tabs;
    format!(
        "the shell's start said {:?} after {:?}, and the last tab to stop said {:?}",
        tabs.last_error, tabs.last_start_took, tabs.last_ended
    )
}

#[test]
fn the_view_menu_shows_and_hides_the_terminal() {
    let mut harness = harness("");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    steady(&mut harness);
    assert!(!harness.state().terminal.visible);

    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::ToggleTerminal, &ctx);
    steady(&mut harness);
    assert!(harness.state().terminal.visible, "the terminal should have opened");
    assert_eq!(
        harness.state().terminal.tabs.count(),
        1,
        "with a shell in it; {}",
        why_there_is_no_tab(&harness)
    );
    assert_eq!(
        harness.state().focus,
        unluminous_app::app::Focus::Terminal,
        "and the keyboard in it"
    );

    harness.state_mut().run_action(Action::ToggleTerminal, &ctx);
    steady(&mut harness);
    assert!(!harness.state().terminal.visible);
    assert_eq!(
        harness.state().focus,
        unluminous_app::app::Focus::Editor,
        "the keyboard comes back"
    );
}

/// Typing goes to the program in the terminal rather than to the document.
///
/// This is the one screenshot test that starts a real shell, because a detached terminal has nothing to
/// answer. It waits for the output rather than assuming it has arrived, and asserts on the text rather than
/// on pixels, because when a shell answers is not something a test can know.
#[test]
fn typing_in_the_terminal_reaches_the_shell_and_not_the_document() {
    let mut harness = harness("the document is untouched");
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::ToggleTerminal, &ctx);
    pump(&mut harness);
    assert_eq!(harness.state().focus, unluminous_app::app::Focus::Terminal);

    // `pump` rather than `run` from here on. A real shell is starting behind this and writes its
    // prompt whenever it is ready, and every write wakes the window, so `run`'s budget of four steps
    // to go quiet is the wrong budget — it is the rule the file's other waiting loops already follow.
    // Seen failing on a loaded machine as `Harness::run exceeded max_steps (4)` with the terminal's
    // own waker as the repaint cause.
    {
        let text = "echo unluminous-typing-works";
        harness.input_mut().events.push(egui::Event::Text(text.to_owned()));
        pump(&mut harness);
    }
    harness.key_press(egui::Key::Enter);
    pump(&mut harness);

    // Thirty seconds, because this waits for a real shell on whatever machine the tests are run on, and a
    // machine busy with a build can take much longer than an idle one.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        pump(&mut harness);
        let found = harness
            .state()
            .terminal
            .tabs
            .active()
            .map(|session| session.snapshot().contains("unluminous-typing-works"))
            .unwrap_or(false);
        if found {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the shell did not answer in thirty seconds, the terminal holds {:?}; {}",
            harness.state().terminal.tabs.active().map(|session| session.snapshot().text()),
            why_there_is_no_tab(&harness)
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(
        harness.state().document().text().to_string(),
        "the document is untouched",
        "nothing typed into the terminal reached the document"
    );
}

// ============================================================================================
// Running: the tile, the widget, the flyout and the dialog.
//
// `task-1683`. Every one of these is drawn from a **detached** session — one with no program behind
// it, fed fixed bytes — which is the trick the terminal's own pictures already use: when a real
// program answers is not something a test can know, and a picture that depended on it would differ
// between runs. What is being looked at is the drawing, and the drawing is the same either way.

/// A window with a run going that has no program behind it.
fn with_run(name: &str, command: &str, rows: usize) -> Harness<'static, UnluminousApp> {
    let mut harness = harness("A document above the run tile.");
    harness.state_mut().new_detached_run(configuration(name, command), rows, 96);
    steady(&mut harness);
    harness
}

/// Feed bytes to the run that is showing, as a program writing them would.
fn feed_run(harness: &mut Harness<'static, UnluminousApp>, bytes: &[u8]) {
    harness.state_mut().run.active_mut().expect("a run").session.feed(bytes);
    steady(harness);
}

#[test]
fn the_run_tile_shows_a_program_running_along_the_bottom() {
    let mut harness = with_run("Dev server", "node server.js --port 3000", 12);
    assert!(harness.state().run.visible);
    assert!(!harness.state().terminal.visible, "the bottom of the window holds one tile");
    feed_run(
        &mut harness,
        b"> dev-server@1.0.0 start\r\n> node server.js --port 3000\r\n\r\n\x1b[32mListening on http://localhost:3000\x1b[0m\r\n  GET /            200  4ms\r\n  GET /style.css   200  1ms\r\n",
    );
    let screen = harness.state().run.active().expect("a run").session.snapshot();
    assert!(screen.contains("Listening on"), "the output should be on the screen");
    // The strip says it is going, and the three buttons that act on it are there.
    harness.get_by_label("Run: Dev server");
    harness.get_by_label("Rerun");
    harness.get_by_label("Stop the run");
    harness.get_by_label("Clear the run output");
    harness.snapshot(shot("run_tile"));
}

#[test]
fn a_run_that_ended_keeps_its_tab_and_the_strip_says_what_it_ended_with() {
    // The reference editor prints its epilogue into the console; Unluminous puts it in the strip, because a line
    // pretending to be program output is exactly the confusion a separate strip avoids.
    let mut harness = with_run("cargo test", "cargo test", 10);
    feed_run(
        &mut harness,
        b"running 12 tests\r\n\x1b[31mtest the_thing ... FAILED\x1b[0m\r\n\r\ntest result: FAILED. 11 passed; 1 failed\r\n",
    );
    // A second run, so the picture holds a finished tab and a running one side by side.
    harness.state_mut().new_detached_run(configuration("Dev server", "node server.js"), 10, 96);
    steady(&mut harness);
    feed_run(&mut harness, b"Listening on http://localhost:3000\r\n");
    let at = harness.state().run.index_of("cargo test").expect("the first run");
    harness.state_mut().run.end_detached(at, Some(101));
    steady(&mut harness);
    assert_eq!(
        harness.state().run.at(at).expect("a run").state().label(),
        "exit code 101",
        "the tab stays, holding what the program wrote"
    );
    harness.snapshot(shot("run_tile_finished"));
}

#[test]
fn the_run_tile_and_the_terminal_tile_take_the_same_place_and_never_both() {
    // Two grids stacked take the editing area below the fold of anything, so pressing either
    // button shows one and puts the other away.
    let mut harness = with_run("Dev server", "node server.js", 8);
    assert!(harness.state().run.visible && !harness.state().terminal.visible);
    let bottom = harness.state().run.grid_area();
    choose(&mut harness, Action::ToggleTerminal);
    assert!(harness.state().terminal.visible && !harness.state().run.visible);
    assert_eq!(
        harness.state().terminal.grid_area().bottom(),
        bottom.bottom(),
        "the same place at the bottom of the window"
    );
    choose(&mut harness, Action::ToggleRunTile);
    assert!(harness.state().run.visible && !harness.state().terminal.visible);
    // And the rail has a button for each, the run one above the terminal one.
    harness.get_by_label("Run tile");
    harness.get_by_label("Terminal tile");

    // Every path that shows either tile puts the other away, which is what the two functions on
    // `UnluminousApp` are for. This is the fault they were written for: `terminal show` from the command
    // line set its own flag and left the run tile up, so both grids were drawn into the same
    // rectangle, one over the other — found in the real window rather than here.
    did(&mut harness, "terminal show");
    assert!(harness.state().terminal.visible && !harness.state().run.visible, "terminal show");
    choose(&mut harness, Action::ToggleRunTile);
    assert!(harness.state().run.visible && !harness.state().terminal.visible);
    choose(&mut harness, Action::NewTerminalTab);
    assert!(harness.state().terminal.visible && !harness.state().run.visible, "New Terminal Tab");
    did(&mut harness, "terminal hide");
    assert!(!harness.state().terminal.visible && !harness.state().run.visible, "and neither is up");
}

#[test]
fn the_run_widget_draws_its_three_states_in_the_title_bar() {
    let mut results = SnapshotResults::new();

    // Idle: a configuration chosen, nothing running.
    let mut harness = harness("");
    harness
        .state_mut()
        .run_configurations
        .add_permanent(configuration("Dev server", "node server.js --port 3000"));
    harness.state_mut().run_selected = Some("Dev server".to_owned());
    steady(&mut harness);
    harness.get_by_label("Choose a run configuration");
    harness.get_by_label("Run the selected configuration");
    results.add(harness.try_snapshot(shot("run_widget_idle")));

    // Running: the stop square appears beside the play button. A control absent when it cannot
    // apply, drawn the moment it can.
    let mut harness = with_run("Dev server", "node server.js --port 3000", 8);
    feed_run(&mut harness, b"Listening on http://localhost:3000\r\n");
    harness.get_by_label("Stop the selected configuration");
    results.add(harness.try_snapshot(shot("run_widget_running")));

    // Stopped with an error: the widget goes back to two buttons and the tile's strip carries the
    // code, which is where the eye already is.
    let at = harness.state().run.index_of("Dev server").expect("the run");
    harness.state_mut().run.end_detached(at, Some(1));
    steady(&mut harness);
    assert!(
        harness.query_by_label("Stop the selected configuration").is_none(),
        "there is nothing left to stop"
    );
    results.add(harness.try_snapshot(shot("run_widget_stopped")));

    report(results);
}

/// `task-1692`: the two buttons at the top right, in the reference editor's order, and the rule that decides
/// whether the second one is there at all.
#[test]
fn the_title_bar_carries_a_run_button_and_a_debug_button_beside_it() {
    // The picture is `run_widget_idle`, which is this scene; what is asserted here is the rule that
    // decides whether the second button is drawn at all.
    // A configuration a debugger can take: `node server.js` names js-debug through the command line
    // itself, which is what `debuggers::adapter_for` reads.
    let mut both = harness("");
    both.state_mut()
        .run_configurations
        .add_permanent(configuration("Dev server", "node server.js --port 3000"));
    both.state_mut().run_selected = Some("Dev server".to_owned());
    steady(&mut both);
    both.get_by_label("Run the selected configuration");
    both.get_by_label("Debug the selected configuration");

    // And one nothing can debug has one button, which is Unluminous's rule for a control that cannot
    // apply: absent rather than dimmed. Nothing here names a debugger — not the command line, not
    // the plugins, and not the untitled document that is showing.
    let mut plain = harness("");
    plain.state_mut().run_configurations.add_permanent(configuration("Format", "black app"));
    plain.state_mut().run_selected = Some("Format".to_owned());
    steady(&mut plain);
    plain.get_by_label("Run the selected configuration");
    assert!(
        plain.query_by_label("Debug the selected configuration").is_none(),
        "there is nothing here a debugger could take"
    );
}

/// The bug button sends the same `Action` the `Run` menu and `Shift+F9` send, which is what
/// `UnluminousApp::debug_a_configuration` being the one place means.
#[test]
fn the_widgets_debug_button_starts_the_chosen_configuration_under_a_debugger() {
    let mut pressed = harness("");
    pressed
        .state_mut()
        .run_configurations
        .add_permanent(configuration("Dev server", "node server.js"));
    pressed.state_mut().run_selected = Some("Dev server".to_owned());
    steady(&mut pressed);
    pressed.get_by_label("Debug the selected configuration").click();
    // The press starts a real adapter and, behind it, a real program, and their output keeps the
    // window redrawing, so the settle is a pump rather than a run — the shape `git_harness` uses for
    // a test whose answer arrives on another thread. A deadline rather than a count of turns, because
    // the adapter search reads folders on a thread and on a busy machine a count of 600 ran out first
    // (`task-2353`).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        pump(&mut pressed);
        if pressed.state().debug.is_some()
            || pressed.state().message.as_deref().is_some_and(|said| said.contains("node"))
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // What happens next depends on what this machine has installed, and the test may not: what is
    // being proved is that the press reached the debugger at all, which either a session or a
    // sentence about the adapter shows.
    let said = pressed.state().message.clone().unwrap_or_default();
    assert!(
        pressed.state().debug.is_some() || said.contains("node"),
        "the press reached the debugger: {said}"
    );
}

/// A debugger this machine has not got is a panel that says what is missing and offers the command,
/// rather than an empty box — `task-1692` §7.1.
///
/// What was found is seeded rather than searched for, because a picture that depended on whether the
/// machine running the test had CodeLLDB installed would not be a baseline at all.
#[test]
fn the_debug_tile_says_what_is_missing_and_offers_to_install_it() {
    let mut harness = harness("");
    harness
        .state_mut()
        .run_configurations
        .add_permanent(configuration("App", r"target\debugpp.exe"));
    harness.state_mut().run_selected = Some("App".to_owned());
    harness.state_mut().debug_adapters.insert(
        "lldb".to_owned(),
        (
            std::time::Instant::now(),
            unluminous_app::services::debuggers::Report {
                name: "lldb",
                found: None,
                configured: false,
                programs: vec!["codelldb", "lldb-dap"],
                languages: vec!["Rust".to_owned()],
                comes_from:
                    "lldb-dap ships with LLVM, and codelldb is the CodeLLDB extension's adapter",
                install: "winget install --id LLVM.LLVM -e".to_owned(),
                settings_key: "debug.lldb".to_owned(),
                caveat: "",
            },
        ),
    );
    choose(&mut harness, Action::Debug(DebugAction::ToggleTile));
    steady(&mut harness);
    assert!(harness.state().debug_panel.visible);
    harness.get_by_label("Install");
    harness.get_by_label("Copy command");
    harness.snapshot(shot("debug_tile_missing_adapter"));
}

#[test]
fn with_nothing_to_run_the_widget_is_the_play_button_that_opens_the_dialog() {
    // Present, because the way to discover the feature has to be visible; small, because it is not
    // yet in use. The sample folder holds neither a Cargo.toml nor a package.json, so no detector
    // has anything to say about it.
    let folder = sample_folder();
    let mut harness = harness_in(&folder);
    assert!(harness.state().run_rows().is_empty(), "nothing to suggest in the sample folder");
    harness.get_by_label("Add a run configuration").click();
    steady(&mut harness);
    assert!(
        harness.state().run_dialog.open,
        "the play button opens the dialog when nothing is chosen"
    );
}

#[test]
fn the_widgets_play_button_starts_the_chosen_configuration() {
    // The button is wired to the same `Action` the `Run` menu and the keyboard send, which is what
    // `UnluminousApp::run_action` being the one place means. A program that is not there is what is run
    // on purpose: what is being proved is that the press reaches the starting, and a test that
    // spawned a real one would be a test that waited for it.
    let mut harness = harness("");
    harness
        .state_mut()
        .run_configurations
        .add_permanent(configuration("Nothing", "unluminous-no-such-program-at-all"));
    harness.state_mut().run_selected = Some("Nothing".to_owned());
    steady(&mut harness);
    harness.get_by_label("Run the selected configuration").click();
    steady(&mut harness);
    let said = harness.state().message.clone().expect("the status bar says what happened");
    assert!(
        said.contains("unluminous-no-such-program-at-all"),
        "the press should have reached the starting, and the bar says {said:?}"
    );
    assert!(harness.state().run.is_empty(), "nothing was started");
}

#[test]
fn the_flyout_lists_the_permanents_the_temporaries_and_the_suggestions() {
    // A project with a `package.json` in it, so the npm detector has something to say — which is
    // what makes the third kind of row appear at all.
    let folder = fixtures_root().join("unluminous-run-suggestions");
    std::fs::create_dir_all(&folder).expect("make the project");
    std::fs::write(
        folder.join("package.json"),
        "{\n  \"name\": \"site\",\n  \"scripts\": { \"dev\": \"vite\", \"build\": \"vite build\" }\n}\n",
    )
    .expect("write the package");
    std::fs::write(folder.join("server.js"), "// a server\n").expect("write a file");

    let mut harness = harness_in(&folder);
    harness
        .state_mut()
        .run_configurations
        .add_permanent(configuration("Dev server", "node server.js --port 3000"));
    harness
        .state_mut()
        .run_configurations
        .add_temporary(configuration("server.js", "node server.js"));
    harness.state_mut().run_selected = Some("Dev server".to_owned());
    steady(&mut harness);

    let rows: Vec<String> = harness.state().run_rows().into_iter().map(|row| row.name).collect();
    assert_eq!(
        rows,
        vec!["Dev server", "server.js", "npm run build", "npm run dev"],
        "permanents, then temporaries, then what the detectors suggest, in name order"
    );

    harness.get_by_label("Choose a run configuration").click();
    steady(&mut harness);
    harness.get_by_label("npm run dev");
    harness.get_by_label("Edit Configurations...");
    harness.snapshot(shot("run_flyout"));
}

#[test]
fn running_a_suggestion_keeps_it_as_a_temporary_so_it_can_be_run_again() {
    let folder = fixtures_root().join("unluminous-run-suggestion-kept");
    std::fs::create_dir_all(&folder).expect("make the project");
    std::fs::write(folder.join("Cargo.toml"), "[package]\nname = \"thing\"\n").expect("write it");
    let mut harness = harness_in(&folder);
    assert_eq!(
        harness.state().run_rows().into_iter().map(|row| row.name).collect::<Vec<_>>(),
        vec!["cargo run"],
        "the detector offers it"
    );
    assert!(harness.state().run_configurations.is_empty(), "and nothing is held yet");
    // Running it makes a temporary. The program itself may or may not start on this machine, which
    // is not what is being tested: what is, is that the thing that was run is now in the list.
    choose(&mut harness, Action::Run(RunAction::Start(Some("cargo run".to_owned()))));
    assert_eq!(harness.state().run_configurations.temporary().len(), 1);
    assert_eq!(harness.state().run_selected.as_deref(), Some("cargo run"));
    // And it is no longer offered as a suggestion as well, so it is one row rather than two.
    assert_eq!(harness.state().run_rows().len(), 1);
    harness.state_mut().run.kill_everything();
}

#[test]
fn run_current_file_is_offered_for_a_javascript_file_and_not_for_a_rust_one() {
    // The plugin's own `run.file`, asked at the moment of use — so switching the JavaScript plugin
    // off withdraws it in the same frame.
    let folder = fixtures_root().join("unluminous-run-current-file");
    std::fs::create_dir_all(&folder).expect("make the project");
    std::fs::write(folder.join("server.js"), "console.log('hello')\n").expect("write it");
    std::fs::write(folder.join("main.rs"), "fn main() {}\n").expect("write it");
    let mut harness = harness_in(&folder);

    harness.state_mut().open_path_permanently(&folder.join("server.js")).expect("the file opens");
    steady(&mut harness);
    assert_eq!(harness.state().run_file_template().as_deref(), Some("node {file}"));

    harness.state_mut().open_path_permanently(&folder.join("main.rs")).expect("the file opens");
    steady(&mut harness);
    assert_eq!(
        harness.state().run_file_template(),
        None,
        "running one file of a Cargo project is not a thing cargo does"
    );

    harness.state_mut().open_path_permanently(&folder.join("server.js")).expect("the file opens");
    steady(&mut harness);
    harness.state_mut().set_plugin_enabled("javascript", false);
    steady(&mut harness);
    assert_eq!(harness.state().run_file_template(), None, "and the plugin is the switch");
}

#[test]
fn a_program_that_prints_and_stops_leaves_what_it_printed_in_its_tab() {
    // The one test here that starts a real program, and it earns it: this is the fault `task-1683`
    // spent its last hour on. A run is opened at a **guessed** size and told the real one on the
    // first frame the tile draws — and a pseudoconsole resized while its child is writing its first
    // line loses that line. `cmd /c echo something` writes and exits inside a millisecond, so it
    // was always still starting when that frame came, and its tab came up empty every single time.
    //
    // The fix is that `UnluminousApp::run_grid_size` works the size out from the rectangle the tile
    // really has, so there is no resize at all. What proves it is a program that prints and stops.
    let folder = fixtures_root().join("unluminous-run-prints-and-stops");
    std::fs::create_dir_all(&folder).expect("make the project");
    let mut harness = harness_in(&folder);

    let marker = "unluminous-printed-this";
    let command = if cfg!(target_os = "windows") {
        format!("cmd /c echo {marker}")
    } else {
        format!("/bin/sh -c \"echo {marker}\"")
    };
    harness.state_mut().run_configurations.add_permanent(configuration("printer", &command));
    harness.state_mut().run_selected = Some("printer".to_owned());
    choose(&mut harness, Action::Run(RunAction::Start(None)));

    // `pump`, not `Harness::run`: `run` gives the window four steps to go quiet and panics
    // otherwise, which is right for a settled window and wrong while a program is being waited for.
    // The rule `task-1654` wrote down, wearing a different hat again.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        harness.step();
        let run = harness.state().run.at(0).expect("the run");
        if !run.is_running() && run.session.snapshot().contains(marker) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the program did not print and finish in thirty seconds; it is {} and the screen holds {:?}",
            run.state().label(),
            run.session.snapshot().text()
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // And it stays there, which is what a tab outliving its program means: the frames after it
    // finished must not take it away either.
    for _ in 0..8 {
        harness.step();
    }
    let run = harness.state().run.at(0).expect("the run");
    assert_eq!(run.state().label(), "finished");
    assert!(
        run.session.snapshot().contains(marker),
        "the output should still be there, and the screen holds {:?}",
        run.session.snapshot().text()
    );
    harness.state_mut().run.kill_everything();
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn the_run_configurations_dialog_lists_them_on_the_left_and_edits_one_on_the_right() {
    let mut harness = harness("");
    harness.state_mut().run_configurations.add_permanent(Configuration {
        name: "Dev server".to_owned(),
        command: "node server.js --port 3000".to_owned(),
        directory: "backend".to_owned(),
        env: "PORT=3000; DEBUG=app:*".to_owned(),
    });
    harness.state_mut().run_configurations.add_permanent(configuration("cargo run", "cargo run"));
    harness.state_mut().run_selected = Some("Dev server".to_owned());
    choose(&mut harness, Action::Run(RunAction::Edit));
    assert!(harness.state().run_dialog.open);
    harness.get_by_label("Run configuration name");
    harness.get_by_label("Run configuration command");
    harness.get_by_label("Run configuration directory");
    harness.get_by_label("Choose directory");
    harness.get_by_label("Add variable");
    harness.get_by_label("Environment key 1");
    harness.get_by_label("Environment value 2");
    harness.get_by_label("Add");
    harness.get_by_label("Remove");
    harness.get_by_label("Done");
    // `task-2282`: the field shows the full path of the folder, and what is stored stays relative.
    let root = harness.state().tree.root().to_path_buf();
    let shown = harness.get_by_label("Run configuration directory").value().unwrap_or_default();
    assert_eq!(
        std::path::Path::new(&shown),
        root.join("backend"),
        "the folder's full path: {shown}"
    );
    let stored = &harness.state().run_configurations.find("Dev server").expect("it").1.directory;
    assert_eq!(stored, "backend", "stored relative to the project");
    harness.snapshot(shot("run_dialog"));

    // Add makes one with a name nothing else has, and chooses it.
    harness.get_by_label("Add").click();
    steady(&mut harness);
    assert_eq!(harness.state().run_dialog.chosen.as_deref(), Some("Unnamed"));
    assert_eq!(harness.state().run_configurations.permanent().len(), 3);

    // A configuration that runs in the project itself shows the project's full path, not an empty box.
    harness.state_mut().run_dialog.chosen = Some("cargo run".to_owned());
    steady(&mut harness);
    let shown = harness.get_by_label("Run configuration directory").value().unwrap_or_default();
    assert_eq!(std::path::Path::new(&shown), root, "the project's own folder: {shown}");

    // Done shuts it.
    harness.get_by_label("Done").click();
    steady(&mut harness);
    assert!(!harness.state().run_dialog.open);
}

#[test]
fn the_environment_table_adds_edits_and_deletes_variables_and_stores_only_those_with_a_key() {
    let mut harness = harness("");
    harness.state_mut().run_configurations.add_permanent(configuration("serve", "node server.js"));
    harness.state_mut().run_selected = Some("serve".to_owned());
    choose(&mut harness, Action::Run(RunAction::Edit));
    let env = |harness: &Harness<'static, UnluminousApp>| {
        harness.state().run_configurations.find("serve").expect("serve").1.env.clone()
    };

    // Two rows, the first typed into, the second left with a value and no key.
    harness.get_by_label("Add variable").click();
    steady(&mut harness);
    harness.get_by_label("Add variable").click();
    steady(&mut harness);
    type_into(&mut harness, "Environment key 1", "PORT");
    steady(&mut harness);
    type_into(&mut harness, "Environment value 1", "3000");
    steady(&mut harness);
    type_into(&mut harness, "Environment value 2", "orphan");
    steady(&mut harness);
    assert_eq!(env(&harness), "PORT=3000", "a row with no key is not stored");
    type_into(&mut harness, "Environment key 2", "DEBUG");
    steady(&mut harness);
    assert_eq!(env(&harness), "PORT=3000; DEBUG=orphan");
    harness.snapshot(shot("run_dialog_environment_table"));

    // Deleting the first row leaves the second, and the dialog keeps showing it.
    harness.get_by_label("Delete environment variable 1").click();
    steady(&mut harness);
    assert_eq!(env(&harness), "DEBUG=orphan");
    harness.get_by_label("Environment key 1");

    // Closing the dialog writes down what the table held, in the format old files already use.
    harness.get_by_label("Done").click();
    steady(&mut harness);
    let configuration = harness.state().run_configurations.find("serve").expect("serve").1.clone();
    assert_eq!(configuration.environment(), vec![("DEBUG".to_owned(), "orphan".to_owned())]);
}

/// Typing the name of a second configuration renames that one, a letter at a time, and the run widget
/// follows the new name. `task-2374`: the dialog held its choice by name, so the first letter typed made a
/// name nothing held and the dialog chose the first configuration, and the next letters rewrote its name.
/// A name that is another configuration's on the way, `Unnamed` between `Unnamed 2` and `Unnamed X`, is
/// the case a lookup by name gets wrong even after it follows the rename.
#[test]
fn typing_a_second_configurations_name_renames_that_one_and_the_widget_follows_it() {
    let mut harness = harness("");
    harness.state_mut().run_configurations.add_permanent(configuration("Unnamed", "cargo run"));
    harness.state_mut().run_selected = Some("Unnamed".to_owned());
    choose(&mut harness, Action::Run(RunAction::Edit));
    harness.get_by_label("Add").click();
    steady(&mut harness);
    assert_eq!(harness.state().run_dialog.chosen.as_deref(), Some("Unnamed 2"));
    harness.state_mut().run_selected = Some("Unnamed 2".to_owned());
    harness.get_by_label("Run configuration name").click();
    steady(&mut harness);
    harness.key_press(egui::Key::End);
    steady(&mut harness);
    for _ in 0..2 {
        harness.key_press(egui::Key::Backspace);
        steady(&mut harness);
    }
    for letter in [" ", "X"] {
        harness.get_by_label("Run configuration name").type_text(letter);
        steady(&mut harness);
    }
    let names: Vec<String> =
        harness.state().run_configurations.permanent().iter().map(|c| c.name.clone()).collect();
    assert_eq!(names, ["Unnamed", "Unnamed X"], "the first configuration keeps its name");
    assert_eq!(harness.state().run_dialog.chosen.as_deref(), Some("Unnamed X"));
    assert_eq!(harness.state().run_selected.as_deref(), Some("Unnamed X"), "the widget follows");
    harness.get_by_label("Done").click();
    steady(&mut harness);
    assert_eq!(harness.state().run_selected.as_deref(), Some("Unnamed X"));
}

/// A key typed one letter at a time keeps every letter, a space included. `task-2287`: the table read its
/// rows back out of the stored text on every frame, and reading trims, so a space typed at the end of a key
/// was taken away before the next letter arrived.
#[test]
fn a_key_typed_a_letter_at_a_time_keeps_every_letter() {
    let mut harness = harness("");
    harness.state_mut().run_configurations.add_permanent(configuration("serve", "node server.js"));
    harness.state_mut().run_selected = Some("serve".to_owned());
    choose(&mut harness, Action::Run(RunAction::Edit));
    harness.get_by_label("Add variable").click();
    steady(&mut harness);
    harness.get_by_label("Environment key 1").click();
    steady(&mut harness);
    for letter in ["M", "Y", " ", "V", "A", "R"] {
        harness.get_by_label("Environment key 1").type_text(letter);
        steady(&mut harness);
    }
    let shown = harness.get_by_label("Environment key 1").value().unwrap_or_default();
    assert_eq!(shown, "MY VAR");
    type_into(&mut harness, "Environment value 1", "1");
    let stored = harness.state().run_configurations.find("serve").expect("serve").1.env.clone();
    assert_eq!(stored, "MY VAR=1");
}

#[test]
fn an_older_configuration_file_fills_the_table_and_a_new_key_field_takes_focus() {
    let mut harness = harness("");
    harness.state_mut().run_configurations.add_permanent(Configuration {
        name: "serve".to_owned(),
        command: "node server.js".to_owned(),
        directory: String::new(),
        env: "PORT=3000; DEBUG=app:*".to_owned(),
    });
    harness.state_mut().run_selected = Some("serve".to_owned());
    choose(&mut harness, Action::Run(RunAction::Edit));
    harness.get_by_label("Environment key 1");
    harness.get_by_label("Environment key 2");
    harness.get_by_label("Add variable").click();
    steady(&mut harness);
    assert!(
        harness.get_by_label("Environment key 3").accesskit_node().is_focused(),
        "the new row's key field is the one being typed into"
    );
}

#[test]
fn the_folder_button_asks_the_picker_and_stores_a_folder_inside_the_project_relatively() {
    let mut harness = harness("");
    harness.state_mut().run_configurations.add_permanent(configuration("serve", "node server.js"));
    harness.state_mut().run_selected = Some("serve".to_owned());
    // The native dialog cannot be driven from a test, so the window's picker is replaced.
    harness.state_mut().folder_picker = |_title, start| Some(start.join("backend"));
    choose(&mut harness, Action::Run(RunAction::Edit));
    harness.get_by_label("Choose directory").click();
    steady(&mut harness);
    let held = harness.state().run_configurations.find("serve").expect("serve").1.directory.clone();
    assert_eq!(held, "backend");

    // A folder outside the project is stored as the absolute path it is.
    let elsewhere = fixtures_root().join("unluminous-run-directory-outside");
    harness.state_mut().set_run_directory("serve", &elsewhere);
    let held = harness.state().run_configurations.find("serve").expect("serve").1.directory.clone();
    assert_eq!(held, elsewhere.display().to_string());
}
#[test]
fn the_dialog_asks_before_removing_a_configuration_whose_program_is_still_running() {
    // Silently killing a server somebody is watching is worse than one extra click.
    let mut harness = with_run("Dev server", "node server.js", 8);
    choose(&mut harness, Action::Run(RunAction::Edit));
    harness.get_by_label("Remove").click();
    steady(&mut harness);
    let question = harness.state().confirmation.clone().expect("a question is asked");
    assert!(question.note.contains("Dev server"), "and it says what is about to be stopped");
    assert_eq!(harness.state().run_configurations.len(), 1, "nothing has gone yet");

    harness.state_mut().answer_the_question(question.answer);
    steady(&mut harness);
    assert!(harness.state().run_configurations.is_empty());
    assert!(harness.state().run.is_empty(), "and its run went with it");
}

#[test]
fn a_project_comes_back_with_its_configurations_and_the_choice_it_still_has() {
    // What `.unluminous` remembers: the permanents in a file of their own, and which of them was chosen
    // in `workspace.conf` beside the terminal's flags. A **temporary** is deliberately not written
    // down, so a project that had one chosen comes back with nothing chosen rather than offering to
    // run something that is not there.
    let root = fixtures_root().join("unluminous-run-remembered");
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("make the project");
    let mut configurations = RunConfigurations::new();
    configurations.add_permanent(Configuration {
        name: "Dev server".to_owned(),
        command: "node server.js".to_owned(),
        directory: "backend".to_owned(),
        env: "PORT=3000".to_owned(),
    });
    unluminous_app::services::run_configurations::save(&root, &configurations);
    let folder = unluminous_app::services::project_state::folder(&root);
    std::fs::write(
        folder.join("workspace.conf"),
        "run.visible = true
run.selected = Dev server
",
    )
    .expect("write the workspace");

    let mut harness = harness_in(&root);
    harness.state_mut().restore_project();
    steady(&mut harness);
    assert_eq!(harness.state().run_configurations.permanent().len(), 1);
    let held =
        harness.state().run_configurations.find("Dev server").expect("it came back").1.clone();
    assert_eq!(held.command, "node server.js");
    assert_eq!(held.directory, "backend");
    assert_eq!(held.env, "PORT=3000");
    assert_eq!(harness.state().run_selected.as_deref(), Some("Dev server"));
    assert!(harness.state().run.visible, "and the tile was up");
    assert!(harness.state().run.is_empty(), "with nothing in it: a run is not restarted");

    // A remembered choice that nothing answers to any more is dropped rather than offered.
    std::fs::write(
        folder.join("workspace.conf"),
        "run.selected = server.js
",
    )
    .expect("write the workspace");
    let mut harness = harness_in(&root);
    harness.state_mut().restore_project();
    steady(&mut harness);
    assert_eq!(harness.state().run_selected, None);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_temporary_has_a_save_button_that_keeps_it_and_a_permanent_does_not() {
    let mut harness = harness("");
    harness
        .state_mut()
        .run_configurations
        .add_temporary(configuration("server.js", "node server.js"));
    harness.state_mut().run_configurations.add_permanent(configuration("cargo run", "cargo run"));
    harness.state_mut().run_dialog.open(Some("cargo run".to_owned()));
    steady(&mut harness);
    assert!(harness.query_by_label("Save").is_none(), "a permanent is already kept");

    harness.state_mut().run_dialog.chosen = Some("server.js".to_owned());
    steady(&mut harness);
    harness.get_by_label("Save").click();
    steady(&mut harness);
    assert!(harness.state().run_configurations.temporary().is_empty());
    assert_eq!(harness.state().run_configurations.permanent().len(), 2);
}

/// A window on a project that has something for the detectors to find.
fn run_project(name: &str) -> Harness<'static, UnluminousApp> {
    let folder = fixture(name, &[("Cargo.toml", "[package]\nname = \"thing\"\n")]);
    harness_in(&folder)
}

#[test]
fn the_command_line_keeps_a_configuration_and_lists_it_with_the_suggestions() {
    let mut harness = run_project("unluminous-cli-run-add");
    let added = did(&mut harness, "run add \"Dev server\" node server.js --port 3000");
    assert_eq!(added["selected"], "Dev server");
    let configurations = added["configurations"].as_array().expect("a list").clone();
    let first = &configurations[0];
    assert_eq!(first["name"], "Dev server");
    assert_eq!(first["command"], "node server.js --port 3000");
    assert_eq!(first["origin"], "permanent");
    assert_eq!(first["started"], false);
    // The detector's suggestion is listed after it, and says which it is.
    assert!(
        configurations.iter().any(|row| row["name"] == "cargo run" && row["origin"] == "suggested"),
        "{configurations:?}"
    );
    // The directory and the environment go in as flags, and a second one of the same name is
    // refused rather than quietly replacing what was there.
    did(
        &mut harness,
        "run add build cargo build --release --directory crates --env \"RUST_LOG=debug\"",
    );
    let held = harness.state().run_configurations.find("build").expect("build").1.clone();
    assert_eq!(held.command, "cargo build --release");
    assert_eq!(held.directory, "crates");
    assert_eq!(held.environment(), vec![("RUST_LOG".to_owned(), "debug".to_owned())]);
    assert_eq!(refused(&mut harness, "run add build cargo test"), "usage");
}

#[test]
fn the_command_line_chooses_removes_and_refuses_a_name_nothing_holds() {
    let mut harness = run_project("unluminous-cli-run-select");
    did(&mut harness, "run add \"Dev server\" node server.js");
    did(&mut harness, "run add build cargo build");
    assert_eq!(did(&mut harness, "run select build")["selected"], "build");
    assert_eq!(harness.state().run_selected.as_deref(), Some("build"));
    assert_eq!(refused(&mut harness, "run select nothing"), "not-found");
    assert_eq!(refused(&mut harness, "run start nothing"), "not-found");

    did(&mut harness, "run remove build");
    assert!(harness.state().run_configurations.find("build").is_none());
    assert_eq!(refused(&mut harness, "run remove build"), "not-found");
    // Removing the chosen one leaves nothing chosen, and `run start` says so rather than guessing.
    did(&mut harness, "run select \"Dev server\"");
    did(&mut harness, "run remove \"Dev server\"");
    assert_eq!(harness.state().run_selected, None);
    assert_eq!(refused(&mut harness, "run start"), "not-applicable");
}

#[test]
fn a_run_that_could_not_start_is_a_failure_rather_than_a_success() {
    // `task-1691`. `run start` on a configuration whose program is not on the window's `PATH` used
    // to come back with `isError` false, `started` false and no reason anywhere, because the arm
    // read the reason out of the status bar and answered `ok` whatever it found. An agent holding
    // only that could not tell a program that failed to spawn from one that ran and exited at once.
    let mut harness = run_project("unluminous-cli-run-cannot-start");
    did(&mut harness, "run add bogus definitely-not-a-real-program");
    let reply = run(&mut harness, "run start bogus");
    assert!(!reply.ok, "a program that could not be spawned is not a success: {}", reply.message);
    let failure = reply.error.expect("a refusal carries an error");
    assert_eq!(failure.code, "failed");
    assert!(
        failure.message.contains("definitely-not-a-real-program"),
        "the refusal should carry the reason the window had: {}",
        failure.message
    );
    // And it is still in the list, because what was tried is worth keeping — the rule
    // `start_a_run` already followed.
    assert!(harness.state().run_configurations.find("bogus").is_some());
}

#[test]
fn adding_a_configuration_whose_program_cannot_be_found_says_so_and_still_adds_it() {
    // The first failure `task-1691`'s agent hit: `run add` accepted `node primes.js` without
    // comment and only `run start` failed, on a window launched from Finder with no version
    // manager's directory on its `PATH`. It is a note rather than a refusal, because a
    // configuration may name a program that will exist by the time it is run.
    let mut harness = run_project("unluminous-cli-run-add-path");
    let reply = run(&mut harness, "run add bogus definitely-not-a-real-program --port 3000");
    assert!(reply.ok, "it is a note, not a refusal: {}", reply.message);
    assert!(
        reply.message.contains("could not be found on this window's PATH"),
        "the reply should say the program is not there: {}",
        reply.message
    );
    assert!(harness.state().run_configurations.find("bogus").is_some(), "and it was still added");

    // A program that really is on the `PATH` says nothing about it. `cargo` is there, because
    // cargo is what started this test.
    let found = run(&mut harness, "run add build cargo build");
    assert!(found.ok, "{}", found.message);
    assert_eq!(found.message, "Added build");
}

#[test]
fn the_command_line_reads_what_a_run_has_written() {
    // A detached run, so what is being tested is the reading rather than a program's timing.
    let mut harness = harness("");
    harness.state_mut().new_detached_run(configuration("Dev server", "node server.js"), 10, 60);
    steady(&mut harness);
    feed_run(&mut harness, b"Listening on http://localhost:3000\r\nGET / 200\r\nGET /a 200\r\n");

    let output = did(&mut harness, "run output");
    assert!(
        output["text"].as_str().expect("text").contains("Listening on http://localhost:3000"),
        "{output:?}"
    );
    // The tail is the last so many lines, which is what a long log wants.
    let tail = did(&mut harness, "run output --tail 1");
    assert_eq!(tail["text"], "GET /a 200");
    // And --wait-for is answered at once when what it is waiting for is already there.
    let found = did(&mut harness, "run output --wait-for Listening");
    assert_eq!(found["found"], true);
    // A configuration that has not been run has nothing to read.
    assert_eq!(refused(&mut harness, "run output nothing"), "not-applicable");
}

#[test]
fn the_command_line_says_whether_a_run_is_going_and_what_it_ended_with() {
    let mut harness = harness("");
    harness.state_mut().new_detached_run(configuration("cargo test", "cargo test"), 8, 60);
    steady(&mut harness);
    let going = did(&mut harness, "run status");
    assert_eq!(going["state"], "running");
    assert_eq!(going["running"], true);

    let at = harness.state().run.index_of("cargo test").expect("the run");
    harness.state_mut().run.end_detached(at, Some(101));
    steady(&mut harness);
    let ended = did(&mut harness, "run status");
    assert_eq!(ended["state"], "exit code 101");
    assert_eq!(ended["exitCode"], 101);
    assert_eq!(ended["running"], false);

    // One that was never started says so rather than pretending.
    harness.state_mut().run_configurations.add_permanent(configuration("build", "cargo build"));
    let never = did(&mut harness, "run status build");
    assert_eq!(never["started"], false);
    assert_eq!(never["state"], "not started");
}

#[test]
fn stopping_a_run_from_the_command_line_leaves_the_tab_and_what_it_wrote() {
    let mut harness = harness("");
    harness.state_mut().new_detached_run(configuration("Dev server", "node server.js"), 8, 60);
    steady(&mut harness);
    feed_run(&mut harness, b"Listening on http://localhost:3000\r\n");
    // The first stop is the polite one, so the run is still going.
    did_while_waiting(&mut harness, "run stop");
    assert!(harness.state().run.active().expect("a run").is_running());
    assert!(harness.state().run.is_stopping(), "and the window is waiting out the grace");
    // The second does not wait.
    did_while_waiting(&mut harness, "run stop");
    assert!(!harness.state().run.active().expect("a run").is_running());
    assert_eq!(harness.state().run.count(), 1, "the tab stays");
    let output = did(&mut harness, "run output");
    assert!(output["text"].as_str().expect("text").contains("Listening on"), "{output:?}");
}

#[test]
fn every_run_command_reaches_the_window() {
    // The rule `task-1661` asks for, checked for this area: a command the catalogue accepts is a
    // command the window knows, so none of them can answer "unknown command".
    let mut harness = run_project("unluminous-cli-run-known");
    for verb in ["list", "add", "remove", "start", "stop", "rerun", "select", "output", "status"] {
        let reply = run(&mut harness, &format!("run {verb} x"));
        assert_ne!(
            reply.error.as_ref().map(|error| error.code.as_str()),
            Some("unknown"),
            "`run {verb}` is in the catalog and the window does not know it"
        );
    }
}

#[test]
fn run_set_changes_the_same_variables_and_directory_the_dialog_edits() {
    let mut harness = harness("");
    harness.state_mut().run_configurations.add_permanent(Configuration {
        name: "serve".to_owned(),
        command: "node server.js".to_owned(),
        directory: String::new(),
        env: "PORT=3000; DEBUG=1".to_owned(),
    });
    let ctx = harness.ctx.clone();
    let run = |harness: &mut Harness<'static, UnluminousApp>, line: &str| {
        let reply = harness.state_mut().run_command_line(line, &ctx).expect("a reply");
        assert!(reply.ok, "`{line}` was refused: {}", reply.message);
    };
    run(&mut harness, "run set serve --set-env PORT=4000 --unset-env DEBUG");
    run(&mut harness, "run set serve --directory backend/api");
    let held = harness.state().run_configurations.find("serve").expect("serve").1.clone();
    assert_eq!(held.env, "PORT=4000");
    assert_eq!(held.directory, "backend/api");
    run(&mut harness, "run set serve --env \"A=1; B=2\"");
    let held = harness.state().run_configurations.find("serve").expect("serve").1.clone();
    assert_eq!(held.environment(), vec![("A".into(), "1".into()), ("B".into(), "2".into())]);
}

/// Click the field called `name`, so it has the keyboard, and type `text` into it.
fn type_into(harness: &mut Harness<'static, UnluminousApp>, name: &str, text: &str) {
    harness.get_by_label(name).click();
    steady(harness);
    harness.get_by_label(name).type_text(text);
    steady(harness);
}

/// `task-2392`: *"I can't copy text in the terminal. CMD + C should copy. And other shortcuts for paste etc should
/// work."* On macOS `Cmd+C` and `Cmd+V` are taken by the menu bar's `Edit -> Copy` and `Edit -> Paste`, so the grid
/// never saw them and the entries went to the document behind it. The menu entries now hand the grid the events a
/// keyboard would have made: a copy takes the grid's selection, and the document's own selection is left alone.
#[test]
fn the_edit_menus_copy_takes_the_terminals_selection_not_the_documents() {
    let mut harness = harness("the document line");
    harness.state_mut().new_detached_terminal_tab(6, 40);
    steady(&mut harness);
    harness
        .state_mut()
        .terminal
        .tabs
        .at_mut(0)
        .expect("the tab")
        .feed(b"copy me from the grid\r\n");
    harness.state_mut().focus = unluminous_app::app::Focus::Terminal;
    // The tile learns it has the keyboard at the end of a frame and reads keys from the next one.
    steady(&mut harness);
    harness.step();
    harness.step();
    {
        let session = harness.state_mut().terminal.tabs.at_mut(0).expect("the tab");
        session.begin_selection(0, 0, unluminous_terminal::session::SelectionKind::Simple);
        session.extend_selection(0, 12);
    }
    let ctx = harness.ctx.clone();
    assert!(
        harness.state().menu_state().terminal_has_the_keyboard,
        "Copy stays enabled for the grid, so the menu bar's key equivalent reaches it"
    );
    harness.state_mut().run_action(Action::Copy, &ctx);
    let mut copied = String::new();
    for _ in 0..3 {
        harness.step();
        if let Some(text) =
            harness.output().platform_output.commands.iter().find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
        {
            copied = text;
            break;
        }
    }
    assert!(copied.starts_with("copy me from"), "the grid's selection was copied, got {copied:?}");
    assert_eq!(
        harness.state().document().text().to_string(),
        "the document line",
        "and the document was not touched"
    );
}

// ============================================================================================
// Termoil's panes, keys, palette and themes inside every terminal grid (`task-2597`).
//
// Every one of these is a detached terminal, and a pane split from a detached terminal is detached too, so no
// shell is started and nothing waits for one.

use unluminous_app::components::terminal_layer;
use unluminous_app::services::termoil_keys::{self, TermoilCommand};

/// Press the key Termoil gives `command` on this platform, the way the platform reports it.
fn press_termoil(harness: &mut Harness<'static, UnluminousApp>, command: TermoilCommand) {
    let chord = termoil_keys::chord_of(command).expect("the command has a key");
    let mac = cfg!(target_os = "macos");
    let modifiers = Modifiers {
        alt: chord.alt,
        shift: chord.shift,
        ctrl: chord.ctrl || (chord.command && !mac),
        command: chord.command || (chord.ctrl && !mac),
        mac_cmd: chord.command && mac,
    };
    harness.key_press_modifiers(modifiers, chord.key);
    steady(harness);
}

/// The panes of the terminal tab that is showing.
fn panes_of_the_tab(harness: &Harness<'static, UnluminousApp>) -> Vec<terminal_layer::PaneInfo> {
    terminal_layer::panes_of(harness.state().terminal.tabs.active().expect("a tab")).0
}

/// What has been typed into one pane of the terminal tab that is showing.
fn typed_into(harness: &mut Harness<'static, UnluminousApp>, pane: u32) -> String {
    let host = harness.state_mut().terminal.tabs.active_mut().expect("a tab");
    terminal_layer::with_pane(host, pane, |session| session.sent_to_a_detached_session())
        .expect("the pane")
}

/// A detached terminal tab with the keyboard.
fn a_terminal_with_the_keyboard() -> Harness<'static, UnluminousApp> {
    let mut harness = with_terminal("the document", 20, 80);
    harness.state_mut().focus = unluminous_app::app::Focus::Terminal;
    steady(&mut harness);
    harness
}

#[test]
fn termoil_s_keys_split_a_terminal_tab_and_move_between_its_panes() {
    let mut harness = a_terminal_with_the_keyboard();
    press_termoil(&mut harness, TermoilCommand::SplitRight);
    let panes = panes_of_the_tab(&harness);
    assert_eq!(panes.len(), 2, "Split Right made a second pane");
    assert!(panes[1].focused, "the new pane has the keyboard");
    let _ = harness.get_by_label("Grid: terminal-grid-pane-1");

    // Typing goes to the pane with the keyboard and to no other.
    harness.input_mut().events.push(egui::Event::Text("right".to_owned()));
    steady(&mut harness);
    assert_eq!(typed_into(&mut harness, 1), "right");
    assert_eq!(typed_into(&mut harness, 0), "", "the first pane was not typed into");

    press_termoil(&mut harness, TermoilCommand::FocusLeft);
    assert!(panes_of_the_tab(&harness)[0].focused, "Alt+Left moved the keyboard to the first pane");
    harness.input_mut().events.push(egui::Event::Text("left".to_owned()));
    steady(&mut harness);
    assert_eq!(typed_into(&mut harness, 0), "left");

    press_termoil(&mut harness, TermoilCommand::SplitDown);
    assert_eq!(panes_of_the_tab(&harness).len(), 3);
    press_termoil(&mut harness, TermoilCommand::ZoomPane);
    let zoomed = terminal_layer::panes_of(harness.state().terminal.tabs.active().expect("a tab")).1;
    assert!(zoomed, "Zoom Pane shows the pane alone");
    press_termoil(&mut harness, TermoilCommand::ZoomPane);
    press_termoil(&mut harness, TermoilCommand::ClosePane);
    assert_eq!(panes_of_the_tab(&harness).len(), 2, "Close Pane closed the pane with the keyboard");
    assert_eq!(
        harness.state().document().text().to_string(),
        "the document",
        "no key reached the document"
    );
}

#[test]
fn termoil_s_palette_key_in_a_terminal_opens_the_terminal_s_own_palette() {
    let mut harness = a_terminal_with_the_keyboard();
    press_termoil(&mut harness, TermoilCommand::CommandPalette);
    let palette = harness.state().palette.as_ref().expect("a palette opened");
    assert_eq!(palette.words.title, "Terminal Commands");
    assert!(palette.results().iter().any(|found| found.command.label == "Split Pane Right"));
    assert!(palette.results().iter().all(|found| found.command.menu == "Terminal"));

    // Running a row runs the command on the terminal the palette was opened over.
    let row = palette
        .results()
        .iter()
        .find(|found| found.command.label == "Split Pane Down")
        .expect("a row")
        .command
        .clone();
    let ctx = harness.ctx.clone();
    harness.state_mut().palette = None;
    harness.state_mut().run_a_palette_row(row, &ctx);
    steady(&mut harness);
    assert_eq!(panes_of_the_tab(&harness).len(), 2);
}

#[test]
fn the_command_line_splits_a_tab_and_speaks_to_one_pane() {
    let mut harness = with_terminal("", 20, 80);
    let split = did(&mut harness, "terminal split --direction down");
    assert_eq!(split["panes"].as_array().expect("panes").len(), 2);
    did(&mut harness, "terminal send --pane 1 --no-enter hello");
    assert_eq!(typed_into(&mut harness, 1), "hello");
    assert_eq!(typed_into(&mut harness, 0), "", "pane 0 was not sent anything");
    {
        let host = harness.state_mut().terminal.tabs.active_mut().expect("a tab");
        terminal_layer::with_pane(host, 1, |pane| pane.feed(b"written in pane one\r\n"));
    }
    let read = did(&mut harness, "terminal read --pane 1");
    assert!(read["text"].as_str().expect("text").contains("written in pane one"));
    assert_eq!(refused(&mut harness, "terminal read --pane 7"), "not-found");

    let listed = did(&mut harness, "terminal panes");
    assert_eq!(listed["panes"][1]["focused"], true);
    did(&mut harness, "terminal focus-pane up");
    assert_eq!(did(&mut harness, "terminal panes")["panes"][0]["focused"], true);
    assert_eq!(did(&mut harness, "terminal zoom-pane")["zoomed"], true);
    did(&mut harness, "terminal zoom-pane");
    did(&mut harness, "terminal close-pane 1");
    assert_eq!(did(&mut harness, "terminal panes")["panes"].as_array().expect("panes").len(), 1);
    assert_eq!(refused(&mut harness, "terminal close-pane 0"), "not-found");

    let keys = did(&mut harness, "terminal keys");
    let actions = keys["keys"].as_array().expect("keys");
    assert!(actions.iter().any(|key| key["action"] == "termoil-split-right"));
}

#[test]
fn the_terminal_theme_is_a_setting_and_colors_every_terminal() {
    let mut harness = with_terminal("", 20, 80);
    let ground = |harness: &Harness<'static, UnluminousApp>| {
        harness.state().terminal.tabs.active().expect("a tab").snapshot().background
    };
    let window_ground = ground(&harness);
    let chosen = did(&mut harness, "terminal theme Dracula");
    assert_eq!(chosen["theme"], "Dracula");
    assert_eq!(harness.state().settings.terminal_theme, "Dracula");
    let dracula = unluminous_terminal::themes::named("Dracula").expect("bundled");
    assert_eq!(ground(&harness), dracula.background, "the terminal is drawn in the theme");

    let listed = did(&mut harness, "terminal themes dracula");
    assert!(listed["themes"].as_array().expect("themes").iter().any(|name| name == "Dracula"));
    assert_eq!(refused(&mut harness, "terminal theme Not A Theme"), "not-found");

    did(&mut harness, "terminal theme window");
    assert_eq!(harness.state().settings.terminal_theme, "");
    assert_eq!(ground(&harness), window_ground, "back to the window's own");
}

#[test]
fn the_theme_picker_previews_each_theme_and_puts_the_chosen_one_back() {
    use unluminous_app::components::terminal_appearance::Column;
    let mut harness = a_terminal_with_the_keyboard();
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Termoil(TermoilCommand::ChooseTheme), &ctx);
    steady(&mut harness);
    assert!(harness.state().palette.is_none(), "the appearance picker, not the command palette");
    let picker = harness.state_mut().terminal_appearance.as_mut().expect("the picker opened");
    assert_eq!(picker.column, Column::Theme, "Choose Terminal Theme gives the themes the keys");
    let themes = &mut picker.lists[0];
    assert_eq!(themes.results()[0].command.label, "Follow the Window's Theme");
    assert_eq!(
        themes.results()[1].command.label,
        "Unluminous Dark",
        "the shipped themes come first"
    );
    let at =
        themes.results().iter().position(|found| found.command.label == "Dracula").expect("listed");
    themes.chosen = at;
    steady(&mut harness);
    let dracula = unluminous_terminal::themes::named("Dracula").expect("bundled");
    let current = || unluminous_terminal::Palette::current().background;
    assert_eq!(current(), dracula.background, "the highlighted theme is previewed");

    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert!(harness.state().terminal_appearance.is_none(), "Escape put the picker away");
    assert_ne!(current(), dracula.background, "and the terminals went back");
    assert_eq!(harness.state().settings.terminal_theme, "");

    harness.state_mut().run_action(Action::SetTerminalTheme("Dracula".to_owned()), &ctx);
    steady(&mut harness);
    assert_eq!(harness.state().settings.terminal_theme, "Dracula", "a row that is run is kept");
}

/// `task-2601`: *"The theme and shader should be in the same option modal, with theme on left, shader, and also
/// cursor theme, like it is for termoil."* One modal, three lists, all three previewed at once, Enter keeps all
/// three, and Escape puts all three back.
#[test]
fn the_appearance_picker_holds_themes_shaders_and_cursor_effects_and_keeps_all_three() {
    use unluminous_app::components::terminal_appearance::Column;
    use unluminous_app::services::{terminal_cursor_effect, terminal_shader};
    let mut harness = a_terminal_with_the_keyboard();
    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::Termoil(TermoilCommand::ChooseShader), &ctx);
    steady(&mut harness);
    let picker = harness.state_mut().terminal_appearance.as_mut().expect("the picker opened");
    assert_eq!(picker.column, Column::Shader, "Choose Terminal Shader gives the shaders the keys");
    assert_eq!(picker.lists[1].results()[0].command.label, "No Shader");
    assert_eq!(picker.lists[1].results().len(), 23, "none and Termoil's 22");
    assert_eq!(picker.lists[2].results()[0].command.label, "No Cursor Effect");
    assert_eq!(picker.lists[2].results().len(), 21, "none and Termoil's 20");
    let _ = harness.get_by_label("Run Dracula on Termoil");
    let shown = &terminal_shader::BUILTINS[0];
    let _ = harness.get_by_label(&format!("Run {} on {}", shown.name, shown.summary));
    let glow = &terminal_cursor_effect::EFFECTS[0];
    let _ = harness.get_by_label(&format!("Run {} on {}", glow.name, glow.summary));

    // Down in the shader list, Right, and Down in the cursor effects: both are previewed.
    harness.key_press(egui::Key::ArrowDown);
    steady(&mut harness);
    let first = terminal_shader::BUILTINS[0].name;
    assert_eq!(terminal_shader::in_use().map(|shader| shader.name), Some(first), "previewed");
    harness.key_press(egui::Key::ArrowRight);
    steady(&mut harness);
    harness.key_press(egui::Key::ArrowDown);
    steady(&mut harness);
    let column = harness.state().terminal_appearance.as_ref().map(|picker| picker.column);
    assert_eq!(column, Some(Column::Cursor), "Right gave the cursor effects the keys");
    let effect = terminal_cursor_effect::EFFECTS[0].name;
    assert_eq!(terminal_cursor_effect::in_use().map(|effect| effect.name), Some(effect));
    assert_eq!(terminal_shader::in_use().map(|shader| shader.name), Some(first), "still previewed");

    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert!(terminal_shader::in_use().is_none(), "Escape put the shader back");
    assert!(terminal_cursor_effect::in_use().is_none(), "and the cursor effect");
    assert_eq!(harness.state().settings.terminal_shader, "");
    assert_eq!(harness.state().settings.terminal_cursor_effect, "");

    // Opened again, and Enter keeps the shader and the cursor effect together.
    harness.state_mut().run_action(Action::Termoil(TermoilCommand::ChooseCursorEffect), &ctx);
    steady(&mut harness);
    let column = harness.state().terminal_appearance.as_ref().map(|picker| picker.column);
    assert_eq!(column, Some(Column::Cursor));
    harness.key_press(egui::Key::ArrowDown);
    steady(&mut harness);
    harness.key_press(egui::Key::ArrowLeft);
    steady(&mut harness);
    harness.key_press(egui::Key::ArrowDown);
    steady(&mut harness);
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    assert!(harness.state().terminal_appearance.is_none(), "Enter shut the picker");
    assert_eq!(harness.state().settings.terminal_shader, first, "the shader was kept");
    assert_eq!(harness.state().settings.terminal_cursor_effect, effect, "and the cursor effect");
    assert_eq!(terminal_shader::in_use().map(|shader| shader.name), Some(first));
    assert_eq!(terminal_cursor_effect::in_use().map(|effect| effect.name), Some(effect));

    did(&mut harness, "terminal shader none");
    did(&mut harness, "terminal cursor-effect none");
    assert!(terminal_cursor_effect::in_use().is_none());
}

/// `task-2601`: *"The shader of terminal impacts the modal over it."* The shader is drawn after egui has drawn
/// everything, so the appearance picker previewing a shader was drawn over by it. The modal's own rectangle is cut
/// out of what the shader covers, and with no modal nothing is.
#[test]
fn the_shader_is_kept_out_of_a_modal_drawn_over_the_terminal() {
    use unluminous_app::services::terminal_shader;
    let mut harness = a_terminal_with_the_keyboard();
    let ctx = harness.ctx.clone();
    did(&mut harness, "terminal shader crt");
    let _ = terminal_shader::take_the_last_shaded();
    steady(&mut harness);
    let (grid, holes) = terminal_shader::take_the_last_shaded().expect("the terminal was shaded");
    assert!(holes.is_empty(), "nothing is over the terminal: {holes:?}");

    harness.state_mut().run_action(Action::Termoil(TermoilCommand::ChooseShader), &ctx);
    steady(&mut harness);
    steady(&mut harness);
    let (_, holes) =
        terminal_shader::take_the_last_shaded().expect("still shaded while previewing");
    let dialog = unluminous_app::components::modal::drawn(
        &ctx,
        unluminous_app::components::terminal_appearance::ID,
    )
    .expect("the picker was drawn");
    let over = dialog.intersect(grid);
    assert!(over.width() > 0.0 && over.height() > 0.0, "the picker is over the terminal");
    assert!(
        holes.iter().any(
            |hole| (hole.min - over.min).length() < 1.0 && (hole.max - over.max).length() < 1.0
        ),
        "the part of the picker over the terminal is cut out of the shader: {holes:?} {over:?}"
    );
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    did(&mut harness, "terminal shader none");
}

#[test]
fn a_canvas_terminal_node_splits_on_termoil_s_keys_too() {
    use unluminous_app::services::realm::Kind;
    let mut harness = harness("");
    did(&mut harness, "realm show");
    let node = harness.state_mut().new_detached_realm_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    steady(&mut harness);
    harness.state_mut().realm.realm.choose(Some(node));
    harness.state_mut().focus = unluminous_app::app::Focus::Realm;
    steady(&mut harness);
    press_termoil(&mut harness, TermoilCommand::SplitRight);
    let session = harness.state().realm.live.terminal(node).expect("the node's terminal");
    assert_eq!(terminal_layer::panes_of(session).0.len(), 2, "the node's terminal is split");
    let _ = harness.get_by_label(&format!("Grid: realm-terminal-{node}-pane-1"));
}

#[test]
fn a_right_click_on_a_terminal_opens_termoil_s_menu_and_its_rows_run() {
    let mut harness = a_terminal_with_the_keyboard();
    let grid = harness.get_by_label("Grid: terminal-grid").rect();
    right_click_at(&mut harness, grid.center());
    harness.get_by_label("Split Pane Right").click();
    steady(&mut harness);
    assert_eq!(panes_of_the_tab(&harness).len(), 2, "the menu row split the terminal");
    assert!(harness.query_by_label("Split Pane Right").is_none(), "the menu went away");
}

/// `task-2612`: *"if termoil has a shader, and unluminous has a shader, the termoil shader should not be used, unless
/// that toggle is on, in which case that part of the window shouldn't have the unluminous shader on top of it, and
/// the termoil shader should be shown instead."* The window's pass is recorded over the whole window every frame.
/// With `--terminals cover`, the default, the terminal records no pass of its own, so the window's shader is what is
/// drawn there. With `--terminals keep` the terminal records its own pass, which the hook cuts out of the window's
/// (`terminal_shader::order_the_jobs`).
#[test]
fn a_window_shader_covers_a_terminal_s_own_unless_the_terminals_keep_it() {
    use unluminous_app::services::{terminal_shader, window_shader};
    let mut harness = a_terminal_with_the_keyboard();
    did(&mut harness, "terminal shader amber");
    let _ = terminal_shader::take_the_last_shaded();
    steady(&mut harness);
    assert!(
        terminal_shader::take_the_last_shaded().is_some(),
        "the terminal's own shader with no window shader"
    );
    assert!(window_shader::take_the_last_shaded().is_none(), "no window pass with nothing chosen");

    did(&mut harness, "theme shader crt");
    assert!(!harness.state().settings.terminals_keep_their_shader, "off by default");
    let _ = terminal_shader::take_the_last_shaded();
    steady(&mut harness);
    let window = window_shader::take_the_last_shaded().expect("the window pass");
    assert_eq!(window.shader, "crt");
    assert_eq!(window.rect, harness.ctx.content_rect(), "the whole window");
    assert!(
        terminal_shader::take_the_last_shaded().is_none(),
        "the window's shader is drawn over the terminal instead"
    );

    did(&mut harness, "theme shader crt --terminals keep");
    assert!(harness.state().settings.terminals_keep_their_shader);
    steady(&mut harness);
    assert!(terminal_shader::take_the_last_shaded().is_some(), "the terminal keeps its own shader");
    assert!(
        window_shader::take_the_last_shaded().is_some(),
        "and the window pass still runs round it"
    );

    // A cursor effect on the window follows the terminal's cursor while the terminal has the keyboard, and does not
    // take the terminal's own look away.
    did(&mut harness, "theme shader none --terminals cover");
    did(&mut harness, "theme cursor-effect sonar");
    steady(&mut harness);
    let window =
        window_shader::take_the_last_shaded().expect("the window pass for the cursor effect");
    assert_eq!((window.shader.as_str(), window.cursor_effect.as_str()), ("", "sonar"));
    let caret = window.caret.expect("the terminal's cursor");
    let grid = harness.get_by_label("Grid: terminal-grid").rect();
    assert!(grid.expand(1.0).contains(caret), "the caret {caret:?} is in the terminal {grid:?}");
    assert!(
        terminal_shader::take_the_last_shaded().is_some(),
        "a window cursor effect alone leaves the terminal its look"
    );
    did(&mut harness, "theme cursor-effect none");
    did(&mut harness, "terminal shader none");
}

#[test]
fn a_shader_chosen_from_the_command_line_is_drawn_and_none_turns_it_off() {
    use unluminous_app::services::terminal_shader;
    let mut harness = a_terminal_with_the_keyboard();
    did(&mut harness, "terminal shader neon-glow");
    assert_eq!(harness.state().settings.terminal_shader, "neon-glow");
    assert_eq!(terminal_shader::in_use().map(|shader| shader.name), Some("neon-glow"));
    did(&mut harness, "terminal shader none");
    assert!(terminal_shader::in_use().is_none());
}

/// One frame of `harness` with the hook the window runs after egui (`AfterTheFrameHook`) drawn over it, the way the
/// window's painter runs it (`vendor/egui-wgpu/src/winit.rs`). A test harness renders without the scene texture, so
/// the hook is taken out of the renderer's resources after a frame has recorded its jobs, run over a copy of that
/// frame, and put back. Answers the frame before the hook and after it.
fn a_frame_with_the_hook(
    harness: &mut Harness<'static, UnluminousApp>,
    render_state: &eframe::egui_wgpu::RenderState,
) -> (image::RgbaImage, image::RgbaImage) {
    use eframe::egui_wgpu::{wgpu, AfterTheFrameHook};
    let before = harness.render().expect("a frame");
    let (width, height) = before.dimensions();
    let device = &render_state.device;
    let frame = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("the frame the hook draws over"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    render_state.queue.write_texture(
        frame.as_image_copy(),
        before.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: None,
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut hook = render_state.renderer.write().callback_resources.remove::<AfterTheFrameHook>();
    if let Some(hook) = hook.as_mut() {
        hook.0.draw(device, &render_state.queue, &mut encoder, &frame, &frame);
    }
    if let Some(hook) = hook {
        render_state.renderer.write().callback_resources.insert(hook);
    }
    let row = (4 * width).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("the frame read back"),
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        frame.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    render_state.queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
        .expect("the frame was read back");
    let mapped = buffer.slice(..).get_mapped_range().expect("the mapped frame");
    let pixels: Vec<u8> =
        mapped.chunks(row as usize).flat_map(|line| line[..4 * width as usize].to_vec()).collect();
    drop(mapped);
    (before, image::RgbaImage::from_raw(width, height, pixels).expect("a whole frame"))
}

/// Whether every pixel of `a` and `b` inside `rect` (or outside it, when `inside` is false) is the same.
fn same_pixels(a: &image::RgbaImage, b: &image::RgbaImage, rect: egui::Rect, inside: bool) -> bool {
    a.enumerate_pixels().all(|(x, y, pixel)| {
        let here = rect.contains(egui::pos2(x as f32 + 0.5, y as f32 + 0.5));
        here != inside || b.get_pixel(x, y) == pixel
    })
}

/// `task-2612`: the window's shader, drawn by the real hook over real frames, for the three cases the ticket names.
///
/// 1. A terminal with its own shader and no window shader: only the terminal changes.
/// 2. A window shader with `--terminals cover`, the default: the window changes, and the terminal is drawn with the
///    window's shader rather than its own.
/// 3. The same with `--terminals keep`: the terminal is exactly what it was in 1, and the rest of the window is
///    exactly what it was in 2.
///
/// `UNLUMINOUS_SHADER_PICTURES=<folder>` writes each frame there as well.
#[test]
fn the_window_shader_is_drawn_over_the_window_and_the_terminal_keeps_its_own_only_when_asked() {
    use unluminous_app::services::terminal_shader;
    let render_state = shared_render_state();
    let folder = sample_folder();
    let mut harness = builder_on(render_state.clone())
        .with_size(vec2(WINDOW[0], WINDOW[1]))
        .build_eframe(move |cc| {
            let mut app = UnluminousApp::with_text(
                folder,
                "fn main() {\n    println!(\"Unluminous window shaders\");\n}\n",
            );
            app.prepare(&cc.egui_ctx);
            app.draw_deterministically();
            app
        });
    harness.state_mut().new_detached_terminal_tab(16, 90);
    steady(&mut harness);
    feed(
        &mut harness,
        b"\x1b[1;32m~/sample\x1b[0m $ ls -la\r\n\x1b[34mchapters\x1b[0m  \x1b[34mnotes\x1b[0m  person.sqlite3  welcome.md\r\n\x1b[1;32m~/sample\x1b[0m $ echo task-2612\r\ntask-2612\r\n",
    );
    harness.state_mut().focus = unluminous_app::app::Focus::Terminal;
    steady(&mut harness);
    let pixels_per_point = harness.ctx.pixels_per_point();
    let to_pixels = |rect: egui::Rect| rect * pixels_per_point;
    let pictures = std::env::var_os("UNLUMINOUS_SHADER_PICTURES").map(std::path::PathBuf::from);
    let keep = |name: &str, picture: &image::RgbaImage| {
        if let Some(folder) = &pictures {
            std::fs::create_dir_all(folder).expect("the pictures folder");
            picture.save(folder.join(name)).expect("write the picture");
        }
    };

    did(&mut harness, "terminal shader amber");
    steady(&mut harness);
    let (plain, terminal_only) = a_frame_with_the_hook(&mut harness, &render_state);
    let (terminal, _) = terminal_shader::take_the_last_shaded().expect("the terminal was shaded");
    let terminal = to_pixels(terminal);
    keep("0-no-shader.png", &plain);
    keep("1-terminal-amber-no-window-shader.png", &terminal_only);
    assert!(
        same_pixels(&plain, &terminal_only, terminal.expand(1.0), false),
        "only the terminal changes"
    );
    assert!(
        !same_pixels(&plain, &terminal_only, terminal.shrink(2.0), true),
        "the terminal is amber"
    );

    did(&mut harness, "theme shader crt");
    steady(&mut harness);
    let (_, covered) = a_frame_with_the_hook(&mut harness, &render_state);
    keep("2-window-crt-covers-the-terminal.png", &covered);
    assert!(!same_pixels(&plain, &covered, terminal, false), "the window has the crt shader");
    assert!(
        !same_pixels(&terminal_only, &covered, terminal.shrink(2.0), true),
        "the terminal is drawn with the window's shader, not amber"
    );

    did(&mut harness, "theme shader crt --terminals keep");
    steady(&mut harness);
    let (_, kept) = a_frame_with_the_hook(&mut harness, &render_state);
    keep("3-window-crt-terminal-keeps-amber.png", &kept);
    assert!(
        same_pixels(&terminal_only, &kept, terminal.shrink(1.0), true),
        "the terminal is amber, as in 1"
    );
    assert!(
        same_pixels(&covered, &kept, terminal.expand(1.0), false),
        "the rest of the window is crt, as in 2"
    );

    // A window cursor effect with no window shader, around the editor's caret once the editor has the keyboard.
    did(&mut harness, "theme shader none --terminals cover");
    did(&mut harness, "terminal shader none");
    did(&mut harness, "theme cursor-effect neon-halo");
    harness.state_mut().focus = unluminous_app::app::Focus::Editor;
    steady(&mut harness);
    let (unshaded, haloed) = a_frame_with_the_hook(&mut harness, &render_state);
    keep("4-window-cursor-effect-at-the-editor-caret.png", &haloed);
    let caret = unluminous_app::services::window_shader::take_the_last_shaded()
        .and_then(|shaded| shaded.caret)
        .expect("the editor's caret");
    assert!(
        !terminal.contains(caret * pixels_per_point),
        "the caret is the editor's, not the terminal's"
    );
    let around = egui::Rect::from_center_size(caret * pixels_per_point, vec2(40.0, 40.0));
    assert!(!same_pixels(&unshaded, &haloed, around, true), "the effect is drawn around the caret");
    did(&mut harness, "theme cursor-effect none");
}
