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
use egui_kittest::kittest::Queryable;
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
fn a_second_terminal_tab_is_added_and_shown_in_front() {
    let mut harness = with_terminal("", 10, 60);
    feed(&mut harness, b"the first tab");
    harness.state_mut().new_detached_terminal_tab(10, 60);
    harness.run();
    feed(&mut harness, b"the second tab");
    assert_eq!(harness.state().terminal.tabs.count(), 2);
    assert_eq!(harness.state().terminal.tabs.active_index(), 1);
    let screen = harness.state().terminal.tabs.active().expect("a tab").snapshot();
    assert!(screen.contains("the second tab"));
    harness.snapshot(shot("terminal_tabs"));

    // Going back to the first tab shows what was in it, so a tab keeps its own screen.
    harness.get_by_label("Terminal tab: detached").click();
    harness.run();
    assert_eq!(harness.state().terminal.tabs.active_index(), 0);
    let screen = harness.state().terminal.tabs.active().expect("a tab").snapshot();
    assert!(screen.contains("the first tab"), "the first tab kept its screen");
}

#[test]
fn a_terminal_tab_is_renamed_from_its_own_menu() {
    let mut harness = with_terminal("", 8, 60);
    harness.state_mut().new_detached_terminal_tab(8, 60);
    harness.run();
    assert_eq!(harness.state().terminal.tabs.names(), vec!["detached", "detached 2"]);

    // Opened through the window's own state, as the gutter's and a file tab's menus are, because
    // the harness cannot press the right mouse button. To the right of the strip, so the picture
    // holds the tabs the menu is about as well as the menu.
    let at = harness.state().terminal.grid_area().left_top() + vec2(420.0, -14.0);
    harness.state_mut().terminal_menu = Some((at, 1));
    harness.run();
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
    harness.run();
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
    harness.run();
    did(&mut harness, "terminal rename --tab 0 first");
    did(&mut harness, "terminal rename --tab 1 second");
    harness.run();
    assert_eq!(harness.state().terminal.tabs.names(), vec!["first", "second"]);

    // The first tab dragged past the middle of the second, which is where a drop lands after it.
    let from = harness.get_by_label("Terminal tab: first").rect().center();
    let onto = harness.get_by_label("Terminal tab: second").rect();
    let to = egui::pos2(onto.right() - 2.0, onto.center().y);
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
    harness.input_mut().events.push(egui::Event::PointerMoved(to));
    harness.run();
    // Held, so the picture shows the tab outlined in the air and the accent mark saying where it
    // would land. It is the same mark the file tabs draw, from the same two functions.
    harness.snapshot(shot("terminal_tab_dragging"));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    harness.run();
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
        harness.run();
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
    harness.get_by_label_contains("Close detached").click();
    harness.run();
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
    harness.run();
    let after = harness.state().terminal.tabs.active().expect("a tab").size();
    assert!(
        after.columns < before.columns && after.rows <= before.rows,
        "a bigger font means fewer cells fit: {before:?} then {after:?}"
    );
    harness.snapshot(shot("terminal_large_font"));
}

#[test]
fn the_view_menu_shows_and_hides_the_terminal() {
    let mut harness = harness("");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    harness.run();
    assert!(!harness.state().terminal.visible);

    let ctx = harness.ctx.clone();
    harness.state_mut().run_action(Action::ToggleTerminal, &ctx);
    harness.run();
    assert!(harness.state().terminal.visible, "the terminal should have opened");
    assert_eq!(harness.state().terminal.tabs.count(), 1, "with a shell in it");
    assert_eq!(
        harness.state().focus,
        unluminous_app::app::Focus::Terminal,
        "and the keyboard in it"
    );

    harness.state_mut().run_action(Action::ToggleTerminal, &ctx);
    harness.run();
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
            "the shell did not answer in thirty seconds, the terminal holds {:?}",
            harness.state().terminal.tabs.active().map(|session| session.snapshot().text())
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
    harness.run();
    harness
}

/// Feed bytes to the run that is showing, as a program writing them would.
fn feed_run(harness: &mut Harness<'static, UnluminousApp>, bytes: &[u8]) {
    harness.state_mut().run.active_mut().expect("a run").session.feed(bytes);
    harness.run();
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
    harness.run();
    feed_run(&mut harness, b"Listening on http://localhost:3000\r\n");
    let at = harness.state().run.index_of("cargo test").expect("the first run");
    harness.state_mut().run.end_detached(at, Some(101));
    harness.run();
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
    harness.run();
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
    harness.run();
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
    both.run();
    both.get_by_label("Run the selected configuration");
    both.get_by_label("Debug the selected configuration");

    // And one nothing can debug has one button, which is Unluminous's rule for a control that cannot
    // apply: absent rather than dimmed. Nothing here names a debugger — not the command line, not
    // the plugins, and not the untitled document that is showing.
    let mut plain = harness("");
    plain.state_mut().run_configurations.add_permanent(configuration("Format", "black app"));
    plain.state_mut().run_selected = Some("Format".to_owned());
    plain.run();
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
    pressed.run();
    pressed.get_by_label("Debug the selected configuration").click();
    // The press starts a real adapter and, behind it, a real program, and their output keeps the
    // window redrawing, so the settle is a pump rather than a run — the shape `git_harness` uses for
    // a test whose answer arrives on another thread.
    for _ in 0..600 {
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
    harness.run();
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
    harness.run();
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
    harness.run();
    harness.get_by_label("Run the selected configuration").click();
    harness.run();
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
    let folder = std::env::temp_dir().join("unluminous-run-suggestions");
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
    harness.run();

    let rows: Vec<String> = harness.state().run_rows().into_iter().map(|row| row.name).collect();
    assert_eq!(
        rows,
        vec!["Dev server", "server.js", "npm run build", "npm run dev"],
        "permanents, then temporaries, then what the detectors suggest, in name order"
    );

    harness.get_by_label("Choose a run configuration").click();
    harness.run();
    harness.get_by_label("npm run dev");
    harness.get_by_label("Edit Configurations...");
    harness.snapshot(shot("run_flyout"));
}

#[test]
fn running_a_suggestion_keeps_it_as_a_temporary_so_it_can_be_run_again() {
    let folder = std::env::temp_dir().join("unluminous-run-suggestion-kept");
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
    let folder = std::env::temp_dir().join("unluminous-run-current-file");
    std::fs::create_dir_all(&folder).expect("make the project");
    std::fs::write(folder.join("server.js"), "console.log('hello')\n").expect("write it");
    std::fs::write(folder.join("main.rs"), "fn main() {}\n").expect("write it");
    let mut harness = harness_in(&folder);

    harness.state_mut().open_path_permanently(&folder.join("server.js")).expect("the file opens");
    harness.run();
    assert_eq!(harness.state().run_file_template().as_deref(), Some("node {file}"));

    harness.state_mut().open_path_permanently(&folder.join("main.rs")).expect("the file opens");
    harness.run();
    assert_eq!(
        harness.state().run_file_template(),
        None,
        "running one file of a Cargo project is not a thing cargo does"
    );

    harness.state_mut().open_path_permanently(&folder.join("server.js")).expect("the file opens");
    harness.run();
    harness.state_mut().set_plugin_enabled("javascript", false);
    harness.run();
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
    let folder = std::env::temp_dir().join("unluminous-run-prints-and-stops");
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
    harness.get_by_label("Run configuration environment");
    harness.get_by_label("Add");
    harness.get_by_label("Remove");
    harness.get_by_label("Done");
    harness.snapshot(shot("run_dialog"));

    // Add makes one with a name nothing else has, and chooses it.
    harness.get_by_label("Add").click();
    harness.run();
    assert_eq!(harness.state().run_dialog.chosen.as_deref(), Some("Unnamed"));
    assert_eq!(harness.state().run_configurations.permanent().len(), 3);

    // Done shuts it.
    harness.get_by_label("Done").click();
    harness.run();
    assert!(!harness.state().run_dialog.open);
}

#[test]
fn the_dialog_asks_before_removing_a_configuration_whose_program_is_still_running() {
    // Silently killing a server somebody is watching is worse than one extra click.
    let mut harness = with_run("Dev server", "node server.js", 8);
    choose(&mut harness, Action::Run(RunAction::Edit));
    harness.get_by_label("Remove").click();
    harness.run();
    let question = harness.state().confirmation.clone().expect("a question is asked");
    assert!(question.note.contains("Dev server"), "and it says what is about to be stopped");
    assert_eq!(harness.state().run_configurations.len(), 1, "nothing has gone yet");

    harness.state_mut().answer_the_question(question.answer);
    harness.run();
    assert!(harness.state().run_configurations.is_empty());
    assert!(harness.state().run.is_empty(), "and its run went with it");
}

#[test]
fn a_project_comes_back_with_its_configurations_and_the_choice_it_still_has() {
    // What `.unluminous` remembers: the permanents in a file of their own, and which of them was chosen
    // in `workspace.conf` beside the terminal's flags. A **temporary** is deliberately not written
    // down, so a project that had one chosen comes back with nothing chosen rather than offering to
    // run something that is not there.
    let root = std::env::temp_dir().join("unluminous-run-remembered");
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
    harness.run();
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
    harness.run();
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
    harness.run();
    assert!(harness.query_by_label("Save").is_none(), "a permanent is already kept");

    harness.state_mut().run_dialog.chosen = Some("server.js".to_owned());
    harness.run();
    harness.get_by_label("Save").click();
    harness.run();
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
    harness.run();
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
    harness.run();
    let going = did(&mut harness, "run status");
    assert_eq!(going["state"], "running");
    assert_eq!(going["running"], true);

    let at = harness.state().run.index_of("cargo test").expect("the run");
    harness.state_mut().run.end_detached(at, Some(101));
    harness.run();
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
    harness.run();
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
            "`run {verb}` is in the catalogue and the window does not know it"
        );
    }
}
