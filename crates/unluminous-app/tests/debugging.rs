//! Debugging, and typing in a code file keystroke by keystroke.
//!
//! The gutter's breakpoints, the debug tile, the execution point, the inline values, the value
//! tooltip, and what a missing adapter says. Nearly every one is drawn from a **detached** session —
//! one with no adapter behind it, fed fixed DAP messages — so the state machine runs over exactly
//! the messages a real adapter would have sent and the picture is the same on every run.
//!
//! Three tests are the exception and drive a real program: two start a real lldb adapter and one a
//! real js-debug against a real Node program. Each of those checks for its adapter first and prints
//! a line and returns when there is none, so a machine with no LLVM installed reports `ok` for a
//! test that verified nothing.
//!
//! The keystroke tests share this file because they share its fixtures. They come from a crash
//! reported while typing a getter into a JavaScript class, and what they do is paint a frame after
//! every key press, which nothing else does.
//!
//! **5 of the 22 tests here take a picture**, and the rest read the window's state back.

mod common;

use common::*;

use egui_kittest::kittest::Queryable;
use unluminous_app::app::actions::Action;
use unluminous_app::app::actions::DebugAction;
use unluminous_core::Command;
use unluminous_dap::Message;

// ============================================================================================
// Debugging: the gutter, the tile, the execution point and the inline values.
//
// `task-1687`. Every picture here is drawn from a **detached** session — one with no adapter behind
// it, fed fixed DAP messages — which is the trick the terminal's own pictures and the run tile's
// already use: when a real debugger answers is not something a test can know, and a picture that
// depended on it would differ between runs. The session runs the whole state machine over those
// messages, so what is drawn is what a real adapter sending them would have drawn.

/// The reverse request js-debug sends, with the shape a real one has.
///
/// `__pendingTargetId` is the adapter's own handle for the program it has already started. It is
/// passed through untouched, so it is written here as a real one is: opaque.
// Two tests were written here against a `DebugState` that kept the connections a target had been
// handed over from — `a_target_handed_over_is_debugged_by_the_session_it_was_handed_to` and
// `the_run_ends_when_the_connection_it_was_launched_on_ends`. `task-1692` answers `startDebugging`
// its own way, by dialling the adapter again and reading both connections onto one channel with the
// child's replies tagged, so there is no retired connection for a test to read and the two could not
// be carried across. What they were about is covered by
// `session::tests::a_child_session_is_reported_with_the_configuration_that_opens_it`, by the rule
// that an answer which is not one for one with what was asked is not taken, and by
// `a_real_node_debugger_stops_at_a_breakpoint_and_reads_a_variable`, which runs a real
// js-debug against a real Node program.

#[test]
fn the_gutter_draws_an_enabled_a_disabled_an_unverified_and_a_conditional_breakpoint() {
    let mut harness = debug_harness("gutter");
    let folder = debug_folder("gutter");
    let path = folder.join("main.rs");
    // Line 2 plain, line 3 conditional, line 4 disabled, line 5 unverified — one of each, so the
    // picture is the whole vocabulary at once.
    did(&mut harness, &format!("debug breakpoint add {} 2", path.display()));
    did(
        &mut harness,
        &format!("debug breakpoint add {} 3 --condition \"attempts > 3\"", path.display()),
    );
    did(&mut harness, &format!("debug breakpoint add {} 4", path.display()));
    did(&mut harness, &format!("debug breakpoint disable {} 4", path.display()));
    did(&mut harness, &format!("debug breakpoint add {} 5", path.display()));
    harness.run();
    assert_eq!(harness.state().document().breakpoints().len(), 4);
    // One of each, which is what the picture is of.
    let conditional: Vec<bool> = harness
        .state()
        .document()
        .breakpoints()
        .iter()
        .map(unluminous_core::Breakpoint::is_conditional)
        .collect();
    assert_eq!(conditional, vec![false, true, false, false]);

    // A session that answered "I could not bind the last one", which is what makes it hollow. Unluminous
    // draws the adapter's answer rather than its own hope.
    harness
        .state_mut()
        .new_detached_debug_session("lldb", configuration("app", "target/debug/app.exe"));
    harness.run();
    let initialize = asked_for(&mut harness, "initialize");
    feed_debug(&mut harness, answer(initialize, "initialize", capabilities()));
    feed_debug(&mut harness, Message::Initialized);
    let breakpoints = asked_for(&mut harness, "setBreakpoints");
    feed_debug(
        &mut harness,
        answer(
            breakpoints,
            "setBreakpoints",
            // Three sent — the disabled one is not — and the last could not be bound.
            serde_json::json!({ "breakpoints": [
                { "id": 1, "verified": true, "line": 2 },
                { "id": 2, "verified": true, "line": 3 },
                { "id": 3, "verified": false, "message": "no code on that line" }
            ]}),
        ),
    );
    harness.get_by_label("Remove breakpoint on line 2");
    harness.get_by_label("Set breakpoint on line 1");
    harness.snapshot(shot("debug_gutter"));
}

/// A project with two long files, so scrolling really moves and line 50 really exists.
///
/// Two, because the shape `task-1794` reports needs both halves of the ownership rule at once: a
/// file that is **open**, whose breakpoints belong to its `Document`, and one that is **not**, whose
/// breakpoints belong to `services::breakpoint_store`. Only the second was broken, and with one file
/// there is nothing to tell them apart.
fn scrolled_folder(name: &str) -> std::path::PathBuf {
    let mut text = String::from("fn start() {\n");
    for line in 2..=59 {
        text.push_str(&format!("    let value{line} = {line};\n"));
    }
    text.push_str("}\n");
    fixture(
        &format!("unluminous-screenshot-debug/{name}"),
        &[("src/main.rs", &text), ("src/report.rs", &text)],
    )
}

/// The `source.path` and lines of the `setBreakpoints` in a batch of what the session asked for.
///
/// The batch is passed in rather than drained here, because `setBreakpoints` and
/// `configurationDone` go out together and a caller needs the seq of both — the same reason
/// `seq_of` reads a batch whole rather than one request at a time.
fn breakpoint_path_sent(batch: &[serde_json::Value]) -> (i64, String, Vec<i64>) {
    let frame = batch
        .iter()
        .find(|frame| frame["command"] == "setBreakpoints")
        .unwrap_or_else(|| panic!("the session should have asked for setBreakpoints: {batch:#?}"));
    let lines = frame["arguments"]["breakpoints"]
        .as_array()
        .map(|all| all.iter().filter_map(|one| one["line"].as_i64()).collect())
        .unwrap_or_default();
    (
        frame["seq"].as_i64().expect("a seq"),
        frame["arguments"]["source"]["path"].as_str().unwrap_or_default().to_owned(),
        lines,
    )
}

/// The byte a **one-based** line starts at, counted the way the store counts it.
fn offset_of_line(text: &str, line: usize) -> usize {
    text.split_inclusive('\n').take(line - 1).map(str::len).sum()
}

/// `task-1794`: putting the project back underneath a running window leaves the breakpoints working.
///
/// A breakpoint is a byte offset into a file, and a `git checkout` — or a branch switch, or a revert
/// — puts the file **and** `.unluminous/breakpoints.conf` back together, consistent with each other. A
/// window that goes on holding the offsets it had is then holding them against bytes that have gone,
/// and the adapter declines to bind: the same silent "the program just does not stop" as the mixed
/// separator, reached from the other side.
///
/// Unluminous already re-reads a **tab** whose file changed underneath it. This is that rule applied to
/// the project's own state, and it is asked in two places: on the timer that already asks the
/// explorer's folders, and at the moment of use, which for a breakpoint is a session start.
#[test]
fn a_checkout_under_a_running_window_leaves_the_breakpoints_working() {
    let folder = std::env::temp_dir().join("unluminous-breakpoints-checkout");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the project");
    let source = folder.join("main.rs");
    let before = "fn main() {\n    let a = 1;\n    let b = a + 1;\n}\n";
    std::fs::write(&source, before).expect("write main.rs");

    let mut harness = harness_in(&folder);
    // `restore_project` is what turns reading and writing `.unluminous` on: a test neither reads nor
    // writes a person's files unless it says so.
    harness.state_mut().restore_project();
    harness.state_mut().open_path_permanently(&source).expect("the file opens");
    harness.run();
    did(&mut harness, &format!("debug breakpoint add {} 3", source.display()));
    for _ in 0..8 {
        harness.step();
    }
    let written = folder.join(".unluminous").join("breakpoints.conf");
    assert!(written.exists(), "the window should have written its own file first");
    assert_eq!(harness.state().document().breakpoints().len(), 1);

    // The checkout: something outside Unluminous puts back a longer `main.rs` **and** the breakpoints file
    // that belongs with it, where the same statement is now line 5. Both at once, which is what makes
    // this recoverable at all — the offset and the bytes it counts into stay consistent.
    let after =
        "// restored\n// by a checkout\nfn main() {\n    let a = 1;\n    let b = a + 1;\n}\n";
    std::fs::write(&source, after).expect("check the file out again");
    let restored = offset_of_line(after, 5);
    std::fs::write(
        &written,
        format!(
            "# The breakpoints in this project. Written by Unluminous, and safe to edit by hand.\n\
             breakpoint.1.path = main.rs\nbreakpoint.1.offset = {restored}\n"
        ),
    )
    .expect("check the breakpoints file out again");

    // The timer is what notices with nobody doing anything, so it is given its interval and some
    // frames. `pump` rather than `run`, which is the rule about a loop that waits.
    std::thread::sleep(unluminous_app::app::WATCH_INTERVAL + std::time::Duration::from_millis(120));
    for _ in 0..8 {
        pump(&mut harness);
    }

    // The store took the file, and so did the open tab — the ownership rule's other half.
    assert_eq!(
        harness.state().breakpoints_of(&source).iter().next().expect("still one").offset,
        restored,
        "the window should have adopted the offsets the checkout brought back"
    );
    assert_eq!(
        harness.state().document().breakpoints().iter().next().expect("still one").offset,
        restored,
        "a file that is open is owned by its Document, so the Document has to be told"
    );

    // And what reaches the adapter is line 5, which is where that statement now is. Before this was
    // fixed it was line 3 — a line the restored file has a comment on, which binds nothing.
    harness
        .state_mut()
        .new_detached_debug_session("lldb", configuration("app", "target/debug/app.exe"));
    harness.run();
    let initialize = asked_for(&mut harness, "initialize");
    feed_debug(&mut harness, answer(initialize, "initialize", capabilities()));
    feed_debug(&mut harness, Message::Initialized);
    let configuring = asked(&mut harness);
    let (_, sent, lines) = breakpoint_path_sent(&configuring);
    assert_eq!(lines, vec![5], "the restored offset is line 5 of the restored file");
    assert_eq!(std::path::PathBuf::from(&sent), source);

    // The window must not write its stale copy back over what the checkout brought in.
    for _ in 0..8 {
        harness.step();
    }
    let now = std::fs::read_to_string(&written).expect("still there");
    assert!(
        now.contains(&format!("offset = {restored}")),
        "the window wrote its own stale copy back over the checkout:\n{now}"
    );
}

/// `task-1794`, the cause: a breakpoint's line is counted the way a `Document` counts it.
///
/// **This is what the shoot actually hit.** A breakpoint is a byte offset into the text a `Document`
/// holds, and `Document::open` turns `\r\n` into `\n` on the way in so that "offsets and line counts
/// have one meaning". But a file that is **not open** is owned by the store, and the three places
/// that turn its offset into a line read the file's own bytes — raw. On a file with Windows line
/// breaks the two readings disagree by one byte for every line before the offset, so the line the
/// adapter is told about is not the line the breakpoint is on: measured against a real CodeLLDB, a
/// breakpoint set on line 43 of a shut file went out as line 46 and came back unmatched, which the
/// gutter draws hollow and which on a line with no code on it binds nothing at all.
///
/// A `git checkout` on a machine with `core.autocrlf` set — which is the machine this is developed
/// on — puts every file in the project into exactly that state, which is why this and the checkout
/// half of the ticket are the same bug seen from two sides.
///
/// The scripted adapter is used rather than a real one because what is asserted is **what Unluminous
/// sends**, which is Unluminous's half and is the same on every machine.
/// `a_real_debugger_binds_a_breakpoint_in_a_file_that_is_not_open` is the other half.
#[test]
fn a_breakpoint_in_a_shut_file_with_windows_line_breaks_names_the_line_it_is_on() {
    let folder = std::env::temp_dir().join("unluminous-screenshot-debug").join("crlf");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(folder.join("src")).expect("make src");

    let mut source = String::from("fn report() {\n");
    for line in 2..=48 {
        source.push_str(&format!("    let value{line} = {line};\n"));
    }
    source.push_str("    let answer = 1;\n}\n");
    let stop_at =
        source.lines().position(|line| line.contains("let answer")).expect("the line") + 1;
    // Written with Windows line breaks, which is the whole of the case.
    std::fs::write(folder.join("src").join("report.rs"), source.replace('\n', "\r\n"))
        .expect("write report.rs");
    let main = folder.join("src").join("main.rs");
    std::fs::write(&main, "fn main() {}\n").expect("write main.rs");

    let mut harness = harness_in(&folder);
    harness.state_mut().open_path_permanently(&main).expect("the file opens");
    harness.run();

    // **The breakpoint is set while `report.rs` is open, and the tab is then closed.** That is the
    // pair that comes apart, and it is what the shoot did without noticing: the offset is made by
    // the `Document`, which has normalised the line breaks away, and is later resolved against the
    // file's raw bytes because by then nothing has it open. Two raw readings agree with each other
    // and hide the fault, which is why setting it on a file that was never open does not show it.
    let report = folder.join("src").join("report.rs");
    harness.state_mut().open_path_permanently(&report).expect("the file opens");
    harness.run();
    did(&mut harness, &format!("debug breakpoint add src/report.rs {stop_at}"));
    let tab = harness.state().files.index_of(&report).expect("report.rs is open");
    harness.state_mut().close_tab(tab);
    harness.run();
    assert!(
        harness.state().files.index_of(&report).is_none(),
        "report.rs has to be shut for the send to go through the disk"
    );

    // What Unluminous believes, read back the way `debug breakpoint list` reads it.
    let listed = did(&mut harness, "debug breakpoint list");
    let rows: Vec<String> = listed["lines"]
        .as_array()
        .expect("the rows")
        .iter()
        .map(|row| row.as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        rows.iter().any(|row| row.contains(&format!(":{stop_at}"))),
        "the listing should say the line it was set on: {rows:#?}"
    );

    harness
        .state_mut()
        .new_detached_debug_session("lldb", configuration("app", "target/debug/app.exe"));
    harness.run();
    let initialize = asked_for(&mut harness, "initialize");
    feed_debug(&mut harness, answer(initialize, "initialize", capabilities()));
    feed_debug(&mut harness, Message::Initialized);

    let configuring = asked(&mut harness);
    let (seq, _, lines) = breakpoint_path_sent(&configuring);
    assert_eq!(
        lines,
        vec![stop_at as i64],
        "the adapter is told the line the breakpoint is on, not one counted in the raw bytes"
    );

    // And the answer is matched back to it, which is what stops the gutter drawing it hollow.
    feed_debug(
        &mut harness,
        answer(
            seq,
            "setBreakpoints",
            serde_json::json!({ "breakpoints": [{ "id": 1, "verified": true, "line": stop_at }] }),
        ),
    );
    let offset = harness.state().breakpoints_of(&report).iter().next().expect("one").offset;
    let verified = harness
        .state()
        .debug
        .as_ref()
        .expect("a session")
        .verified(&report, offset)
        .expect("the adapter's answer is filed against the breakpoint that was sent");
    assert!(verified.verified);
}

/// `task-1794`: a breakpoint binds wherever the editor happens to be scrolled.
///
/// The ticket bisected this to the scroll, because the scroll is what the shoot did differently. It
/// is not the scroll: it is **whether the file the breakpoint is in is the open one**, which the
/// scroll only correlated with. The ticket's own tell says so — during a session, `debug breakpoint
/// list` printed the bound one as `src\main.rs` and the unbound one as `src/report.rs`, and that
/// forward slash is the whole fault.
///
/// A file that is open is owned by its `Document` and every other file is owned by the store, and a
/// store key is built by joining the project root to the relative path the file was written down
/// with. `Path::join` does not normalise, so on Windows that key is `C:\project\src/report.rs`.
/// Unluminous cannot see the difference, because `Path` compares components; the adapter can, and takes
/// it, and matches it against no compile unit. Nothing is reported anywhere: the program runs to
/// completion and the debug tile stays empty, which is exactly what a program that never stops looks
/// like.
///
/// So this asserts the wire, and then asserts the session really pauses — both for the open file and
/// for the shut one, scrolled first in every case.
#[test]
fn a_breakpoint_binds_wherever_the_editor_is_scrolled() {
    for open in [true, false] {
        let name = if open { "scrolled-open" } else { "scrolled-shut" };
        let folder = scrolled_folder(name);
        let report = folder.join("src").join("report.rs");
        let mut harness = harness_in(&folder);

        // `main.rs` is always the open tab, so that there is something to scroll in both cases and
        // the shut case still has a document the window is drawing.
        harness
            .state_mut()
            .open_path_permanently(&folder.join("src").join("main.rs"))
            .expect("the file opens");
        harness.run();
        if open {
            harness.state_mut().open_path_permanently(&report).expect("the file opens");
            harness.run();
        }
        did(&mut harness, "editor scroll --line 30");
        assert!(
            harness.state().files.active().scroll > 0.0,
            "the editor really has to be scrolled for this to be the reported case"
        );

        // Written the way the ticket wrote it and the way an agent types one: relative, with the
        // separator every one of these files uses.
        did(&mut harness, "debug breakpoint add src/report.rs 50");

        harness
            .state_mut()
            .new_detached_debug_session("lldb", configuration("app", "target/debug/app.exe"));
        harness.run();
        let initialize = asked_for(&mut harness, "initialize");
        feed_debug(&mut harness, answer(initialize, "initialize", capabilities()));
        feed_debug(&mut harness, Message::Initialized);

        let configuring = asked(&mut harness);
        let (seq, sent, lines) = breakpoint_path_sent(&configuring);
        assert_eq!(lines, vec![50], "{name}: line 50 is what was asked for");
        // It names the right file — and note that this passes **even with the fault**, because
        // `PathBuf` compares components and reads `/` and `\` alike. That is precisely why the fault
        // was invisible from inside Unluminous, so it is asserted first and is not the assertion that
        // catches anything.
        assert_eq!(
            std::path::PathBuf::from(&sent),
            report,
            "{name}: the adapter is handed the path Unluminous holds"
        );
        // This is the one the fault fails. What crosses the wire is **text**, and the adapter has no
        // `Path` to compare with — it matches the string against its debug information.
        if cfg!(windows) {
            assert!(
                !sent.contains('/'),
                "{name}: a Windows path with a forward slash in it: {sent}"
            );
        }

        // The adapter binds it, and the program stops there.
        feed_debug(
            &mut harness,
            answer(
                seq,
                "setBreakpoints",
                serde_json::json!({ "breakpoints": [{ "id": 1, "verified": true, "line": 50 }] }),
            ),
        );
        let done = seq_of(&configuring, "configurationDone");
        feed_debug(&mut harness, answer(done, "configurationDone", serde_json::Value::Null));
        feed_debug(
            &mut harness,
            Message::Stopped(unluminous_dap::Stopped {
                reason: "breakpoint".to_owned(),
                thread: Some(1),
                description: None,
                text: None,
                all_threads: true,
            }),
        );
        let batch = asked(&mut harness);
        let threads = seq_of(&batch, "threads");
        let stack = seq_of(&batch, "stackTrace");
        feed_debug(
            &mut harness,
            answer(
                threads,
                "threads",
                serde_json::json!({ "threads": [{ "id": 1, "name": "main" }] }),
            ),
        );
        feed_debug(
            &mut harness,
            answer(
                stack,
                "stackTrace",
                serde_json::json!({ "stackFrames": [
                    { "id": 1000, "name": "app::report", "line": 50,
                      "source": { "path": report.to_string_lossy() } }
                ]}),
            ),
        );

        assert!(
            harness.state().debug.as_ref().expect("a session").is_paused(),
            "{name}: the session should have paused at the breakpoint"
        );
        // And Unluminous knows the adapter bound it, which is what stops the gutter drawing it hollow:
        // the answer is filed under the same key the request was sent under.
        let offset = harness.state().breakpoints_of(&report).iter().next().expect("one").offset;
        let verified = harness
            .state()
            .debug
            .as_ref()
            .expect("a session")
            .verified(&report, offset)
            .expect("the adapter answered about the breakpoint that was sent");
        assert!(verified.verified, "{name}: the adapter said it bound");
    }
}

#[test]
fn the_debug_tile_shows_the_frames_the_variables_and_a_watch() {
    let mut harness = paused_harness("tile");
    assert!(harness.state().debug.as_ref().expect("a session").is_paused());
    assert_eq!(harness.state().debug.as_ref().expect("a session").frames.len(), 2);

    // A watch, answered as a debugger would answer one.
    harness.state_mut().debug.as_mut().expect("a session").add_watch("items.len()");
    harness.run();
    let evaluate = asked_for(&mut harness, "evaluate");
    feed_debug(
        &mut harness,
        answer(evaluate, "evaluate", serde_json::json!({ "result": "3", "type": "usize" })),
    );

    // And a structure opened, which is the whole of the lazy model: nothing deeper was fetched
    // until this row was clicked.
    harness.state_mut().debug.as_mut().expect("a session").toggle_row("Locals/items");
    harness.run();
    let children = asked_for(&mut harness, "variables");
    feed_debug(
        &mut harness,
        answer(
            children,
            "variables",
            serde_json::json!({ "variables": [
                { "name": "[0]", "value": "1", "type": "i32", "variablesReference": 0 },
                { "name": "[1]", "value": "2", "type": "i32", "variablesReference": 0 },
                { "name": "[2]", "value": "3", "type": "i32", "variablesReference": 0 }
            ]}),
        ),
    );

    harness.get_by_label("Frame: app::main");
    harness.get_by_label("Variable: attempts = 3");
    harness.get_by_label_contains("Remove watch: items.len()");
    // The stepping buttons are all there, and so is the stop.
    for button in ["Resume", "Step Over", "Step Into", "Step Out", "Stop Debugging"] {
        harness.get_by_label(button);
    }
    harness.snapshot(shot("debug_tile"));
}

#[test]
fn the_execution_point_and_the_inline_values_are_drawn_over_the_source() {
    let mut harness = paused_harness("point");
    let folder = debug_folder("point");
    // The window jumped to the file the program stopped in and put the caret on line 4.
    assert!(harness.state().document().path().expect("a file").ends_with("main.rs"));
    let (path, line) =
        harness.state().debug.as_ref().expect("a session").location().expect("stopped somewhere");
    assert!(path.ends_with("main.rs"), "{}", path.display());
    assert_eq!(line, 4);
    assert!(path.starts_with(&folder));

    // A value the debugger could not read is not painted at the end of a line. It is still in the
    // tree, in the debugger's own words — but `step = <optimized out>` beside somebody's code is the
    // debugger declining to answer dressed as information, and the reference editor paints nothing there either.
    let painted = harness.state_mut().inline_values_for_test();
    assert!(
        painted.iter().any(|(_, text)| text == "attempts = 3"),
        "a value the debugger read is painted: {painted:?}"
    );
    assert!(
        !painted.iter().any(|(_, text)| text.contains("step")),
        "and one it could not is not: {painted:?}"
    );

    harness.snapshot(shot("debug_execution_point"));
}

/// `task-1696`: the value tooltip, asked for at the caret and answered as a debugger answers.
///
/// It is driven through `Debug -> Show Value` rather than by moving a pointer, because what the
/// pointer adds is a 350 ms rest and a rectangle — both of which are unit tested with no window —
/// and both paths end in the same `open_the_value_tooltip`.
#[test]
fn the_value_tooltip_shows_a_structure_and_opens_it_into_its_fields() {
    let mut harness = paused_harness("hover");
    // Line 4 is `let total = attempts + items.len();`. The caret goes on `items`, which is where a
    // pointer resting on that word would put the question.
    let text = harness.state().document().text().to_string();
    let offset = text.find("items.len()").expect("the call is in the file");
    // A frame is drawn between the two on purpose. The execution point is followed **once a stop**
    // rather than once a frame — before `task-1696` this jump ran every frame it was true, so the
    // caret could not be moved at all while a program was stopped and this would ask about the first
    // word on the stopped line every time.
    harness
        .state_mut()
        .document_mut()
        .apply(unluminous_core::Command::PlaceCaret { offset: offset + 2, extend: false });
    harness.run();
    assert_eq!(
        harness.state().document().selection().head,
        offset + 2,
        "a stopped program does not take the caret back on every frame"
    );
    choose(&mut harness, Action::Debug(DebugAction::ShowValue));
    harness.run();

    // The expression it read is the whole field path ending at the pointer, which is what the reference editor
    // shows the value of.
    let asking = harness
        .state()
        .value_tooltip
        .as_ref()
        .unwrap_or_else(|| {
            panic!(
                "a tooltip: msg={:?} hover={:?}",
                harness.state().message,
                harness
                    .state()
                    .debug
                    .as_ref()
                    .and_then(|d| d.hover.as_ref())
                    .map(|h| h.expression.clone())
            )
        })
        .expression
        .clone();
    assert_eq!(asking, "items", "the word the caret is on");

    let evaluate = asked_for(&mut harness, "evaluate");
    feed_debug(
        &mut harness,
        answer(
            evaluate,
            "evaluate",
            serde_json::json!({
                "result": "Vec<i32>(len:3)",
                "type": "alloc::vec::Vec<i32>",
                "variablesReference": 41
            }),
        ),
    );
    // The root opens itself, which is what a person means by "show me the object" — so the children
    // are asked for with no click at all.
    let children = asked_for(&mut harness, "variables");
    feed_debug(
        &mut harness,
        answer(
            children,
            "variables",
            serde_json::json!({ "variables": [
                { "name": "[0]", "value": "1", "type": "i32", "variablesReference": 0 },
                { "name": "[1]", "value": "2", "type": "i32", "variablesReference": 0 },
                { "name": "[2]", "value": "3", "type": "i32", "variablesReference": 0 }
            ]}),
        ),
    );

    // `Value:` rather than `Variable:`, because the tile is showing the same variable at the
    // same moment and two controls must not share a name.
    harness.get_by_label("Value: items = Vec<i32>(len:3)");
    harness.get_by_label("Value: [1] = 2");
    harness.snapshot(shot("debug_value_tooltip"));
}

/// A row being typed over. The field is `show_row`'s, which is the same function the tile draws its
/// own rows with, so this is one control in two places rather than two that resemble each other.
#[test]
fn a_row_of_the_value_tooltip_can_be_typed_over() {
    let mut harness = paused_harness("hover-edit");
    let text = harness.state().document().text().to_string();
    let offset = text.find("attempts + items").expect("line 4 is in the file");
    // A frame is drawn between the two on purpose. The execution point is followed **once a stop**
    // rather than once a frame — before `task-1696` this jump ran every frame it was true, so the
    // caret could not be moved at all while a program was stopped and this would ask about the first
    // word on the stopped line every time.
    harness
        .state_mut()
        .document_mut()
        .apply(unluminous_core::Command::PlaceCaret { offset: offset + 2, extend: false });
    harness.run();
    choose(&mut harness, Action::Debug(DebugAction::ShowValue));
    harness.run();
    let evaluate = asked_for(&mut harness, "evaluate");
    feed_debug(
        &mut harness,
        answer(evaluate, "evaluate", serde_json::json!({ "result": "3", "type": "i32" })),
    );

    // The root of a tooltip has no container reference, so `setVariable` cannot name it and
    // `setExpression` is what changes it. The adapter offered it, so the field is drawn.
    harness.state_mut().value_tooltip.as_mut().expect("a tooltip").editing =
        Some(("attempts".to_owned(), "9".to_owned()));
    harness.run();
    harness.get_by_label("Set Value: attempts");
    harness.snapshot(shot("debug_value_tooltip_editing"));
}

#[test]
fn the_bottom_of_the_window_holds_one_of_three_tiles_and_never_two() {
    // Two grids stacked take the editing area below the fold of anything, so showing any of the
    // three puts the other two away. `task-1683` made this a pair; this is the trio.
    let mut harness = paused_harness("tiles");
    assert!(harness.state().debug_panel.visible);
    assert!(!harness.state().run.visible && !harness.state().terminal.visible);

    choose(&mut harness, Action::ToggleTerminal);
    assert!(harness.state().terminal.visible);
    assert!(!harness.state().debug_panel.visible && !harness.state().run.visible);

    choose(&mut harness, Action::ToggleRunTile);
    assert!(harness.state().run.visible);
    assert!(!harness.state().debug_panel.visible && !harness.state().terminal.visible);

    choose(&mut harness, Action::ToggleDebugTile);
    assert!(harness.state().debug_panel.visible);
    assert!(!harness.state().run.visible && !harness.state().terminal.visible);

    // And the rail has a button for each of the three, at the bottom of the window.
    harness.get_by_label("Debug tile");
    harness.get_by_label("Terminal tile");
    harness.get_by_label("Run tile");

    // The command line goes down the same path, which is what `show_the_*_tile` exists for.
    did(&mut harness, "terminal show");
    assert!(!harness.state().debug_panel.visible, "terminal show puts the debug tile away");
}

#[test]
fn stepping_lets_go_of_the_frame_and_the_execution_point() {
    let mut harness = paused_harness("stepping");
    assert!(!harness.state().debug.as_ref().expect("a session").rows.is_empty());

    choose(&mut harness, Action::Debug(DebugAction::StepOver));
    let debug = harness.state().debug.as_ref().expect("a session");
    assert!(!debug.is_paused(), "the program is going again");
    assert!(debug.rows.is_empty(), "every variablesReference died the moment it was told to go on");
    assert!(debug.location().is_none(), "and so did the execution point");
    // The request really went out, with the thread the adapter named.
    let stepped = harness.state_mut().debug.as_mut().expect("a session").requested();
    let next = stepped.iter().find(|frame| frame["command"] == "next").expect("a next request");
    assert_eq!(next["arguments"]["threadId"], 1);

    // Stepping again while it runs is refused with a sentence rather than sent into the dark.
    assert_eq!(refused(&mut harness, "debug step-over"), "not-applicable");
}

#[test]
fn a_breakpoint_moves_with_the_text_and_an_edit_is_not_a_reason_to_re_send_it() {
    let mut harness = paused_harness("moved");
    let path = debug_folder("moved").join("main.rs");
    did(&mut harness, &format!("debug breakpoint add {} 4", path.display()));
    harness.state_mut().debug.as_mut().expect("a session").requested();

    // A line typed at the top of the file, which moves every byte below it.
    harness.state_mut().document_mut().apply(Command::PlaceCaret { offset: 0, extend: false });
    harness.state_mut().document_mut().apply(Command::Insert("// a note\n".to_owned()));
    harness.run();
    // The dot followed the text: the same line of the program, one further down the file.
    let listed = did(&mut harness, "debug breakpoint list");
    let rows = listed["breakpoints"].as_array().expect("the list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["line"], 5, "it moved with the text: {rows:#?}");

    // **Editing text during a session does not re-send**: the running program's code has not
    // changed, so the adapter's positions stand — which is what every surveyed editor does.
    let asked = harness.state_mut().debug.as_mut().expect("a session").requested();
    assert!(
        !asked.iter().any(|frame| frame["command"] == "setBreakpoints"),
        "an edit is not a reason to tell the debugger anything: {asked:#?}"
    );

    // Toggling one **is**, and it goes out with the lines the file has now.
    did(&mut harness, &format!("debug breakpoint add {} 2", path.display()));
    let asked = harness.state_mut().debug.as_mut().expect("a session").requested();
    let sent = asked
        .iter()
        .find(|frame| frame["command"] == "setBreakpoints")
        .expect("the file was re-sent");
    let lines: Vec<i64> = sent["arguments"]["breakpoints"]
        .as_array()
        .expect("the breakpoints")
        .iter()
        .map(|one| one["line"].as_i64().unwrap_or(0))
        .collect();
    assert_eq!(lines, vec![2, 5]);
}

#[test]
fn a_file_whose_language_names_no_debugger_has_no_debug_controls_at_all() {
    // Unluminous's rule for a control that can never apply: absent, not dimmed. A stylesheet has nothing
    // to step through and never will.
    let mut harness = harness_in(&sample_folder());
    harness.get_by_label_contains("notes.txt").click();
    harness.run();
    let state = harness.state().menu_state();
    assert!(!state.debug_applies, "nothing claims a .txt");
    let entries = unluminous_app::app::actions::gutter_menu(&state);
    let names: Vec<String> = entries
        .iter()
        .filter_map(|entry| match entry {
            unluminous_app::app::actions::Entry::Item { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert!(!names.iter().any(|name| name.contains("Breakpoint")), "{names:?}");
    // And asking anyway is a sentence rather than a dot no debugger would ever honour.
    choose(&mut harness, Action::Debug(DebugAction::ToggleBreakpoint));
    assert!(harness.state().document().breakpoints().is_empty());
}

#[test]
fn setting_a_value_shows_what_the_debugger_now_holds_rather_than_what_was_typed() {
    let mut harness = paused_harness("set-value");
    did(&mut harness, "debug set-value Locals/attempts 9");
    let asked = asked_for(&mut harness, "setVariable");
    // A debugger that rounded a float, or interned a string, is telling the truth about what the
    // program holds — so its answer is what the row shows.
    feed_debug(
        &mut harness,
        answer(asked, "setVariable", serde_json::json!({ "value": "9", "type": "i32" })),
    );
    let debug = harness.state().debug.as_ref().expect("a session");
    let row = debug.rows.iter().find(|row| row.key == "Locals/attempts").expect("the row");
    assert_eq!(row.value, "9");
}

#[test]
fn the_command_line_can_set_a_breakpoint_read_the_stack_and_read_a_variable() {
    // The sequence the whole feature is an acceptance test of, and its second customer: an agent
    // driving Unluminous can observe a program's actual state instead of reasoning about it.
    let mut harness = paused_harness("cli");
    let path = debug_folder("cli").join("main.rs");
    did(&mut harness, &format!("debug breakpoint add {} 4", path.display()));

    let status = did(&mut harness, "debug status");
    assert_eq!(status["paused"], true);
    assert_eq!(status["line"], 4);
    assert_eq!(status["adapter"], "lldb");

    let frames = did(&mut harness, "debug frames --include-subtle");
    let listed = frames["lines"].as_array().expect("the frames");
    assert_eq!(listed.len(), 2);
    assert!(listed[0].as_str().expect("a line").contains("app::main"));

    let variables = did(&mut harness, "debug variables");
    let printed = variables["lines"].as_array().expect("the rows");
    assert!(
        printed.iter().any(|line| line.as_str().expect("a line").contains("attempts: i32 = 3")),
        "{printed:#?}"
    );

    // `evaluate` waits for the debugger's answer rather than reporting the question, which is what
    // its own `--timeout` flag promises. The answer arrives on a later frame, so the request is held
    // — which is what `run_command_line` returning `None` means.
    let ctx = harness.ctx.clone();
    let held = harness.state_mut().run_command_line("debug evaluate attempts", &ctx);
    assert!(held.is_none(), "an evaluation is answered on a later frame");
    let asked = asked_for(&mut harness, "evaluate");
    feed_debug(
        &mut harness,
        answer(asked, "evaluate", serde_json::json!({ "result": "3", "type": "i32" })),
    );

    // And the tile is reachable from the command line too, which is the fourth rule of the CLI.
    did(&mut harness, "action run toggle-debug-tile");
    assert!(!harness.state().debug_panel.visible);
}

#[test]
fn concise_debug_replies_lead_with_the_paused_frame_and_locals() {
    let mut harness = paused_harness("concise-debug-reply");

    let status = did(&mut harness, "debug status");
    assert_eq!(status["pausedFrame"]["name"], "app::main");
    assert!(
        status["locals"]
            .as_array()
            .expect("the fetched locals")
            .iter()
            .any(|row| row["name"] == "total" && row["value"] == "6"),
        "the runtime value is an immediate debugger answer: {status:#?}"
    );
    assert!(status.get("frames").is_none(), "ordinary replies do not carry a stack: {status:#?}");
    assert!(status.get("variables").is_none());
    assert!(status.get("watches").is_none());
    assert!(status["lines"]
        .as_array()
        .expect("spoken locals")
        .iter()
        .any(|line| line.as_str().is_some_and(|line| line.contains("total: usize = 6"))));

    let ordinary = did(&mut harness, "debug frames");
    assert_eq!(ordinary["lines"].as_array().map(Vec::len), Some(1));
    assert_eq!(ordinary["frames"].as_array().map(Vec::len), Some(1));
    assert_eq!(ordinary["hiddenFrames"], 1);
    assert!(ordinary.get("locals").is_none());
    assert!(ordinary.get("watches").is_none());

    let complete = did(&mut harness, "debug frames --include-subtle");
    assert_eq!(complete["lines"].as_array().map(Vec::len), Some(2));
    assert_eq!(complete["frames"].as_array().map(Vec::len), Some(2));
    assert_eq!(complete["hiddenFrames"], 0);

    let variables = did(&mut harness, "debug variables");
    assert!(variables["variables"].as_array().is_some_and(|rows| !rows.is_empty()));
    assert!(variables.get("frames").is_none());
    assert!(variables.get("locals").is_none());
    assert!(variables.get("watches").is_none());

    let watches = did(&mut harness, "debug watch list");
    assert_eq!(watches["watches"].as_array().map(Vec::len), Some(0));
    assert!(watches.get("frames").is_none());
    assert!(watches.get("locals").is_none());
    assert!(watches.get("variables").is_none());
}

/// `task-1794`, against a **real** debugger: a breakpoint in a file that is not open really binds.
///
/// The scripted test above asserts the bytes that go on the wire, which is Unluminous's half. This is the
/// other half, and it is the half the ticket measured by hand: the shoot set a breakpoint on
/// `src/report.rs:50` in a project whose open tab was `src/main.rs`, pressed Debug, and the program
/// ran to completion with the debug tile empty — four re-takes, and nothing anywhere saying why.
///
/// The fault was that a file which is not open is owned by `services::breakpoint_store`, whose key is
/// the project root joined to the relative path it was written down with. `Path::join` does not
/// normalise, so on Windows that key is `C:\project\src/report.rs`, and `to_string_lossy` put exactly
/// that on the wire. lldb takes it, matches it against no compile unit, and binds nothing.
///
/// So this is the reported shape exactly: two files, only one of them open, the editor scrolled, and
/// the breakpoint named the way an agent names one — relative, with a forward slash.
///
/// **`#[ignore]`d**, which is `agent_board.rs`'s rule and `task-1922`'s correction: a test that
/// returns early with a message on a machine with no adapter *reports a pass*, so a suite with three
/// of them in it says it checked something it never ran. `tools/nightly.ps1` runs it with `--ignored`
/// on a machine where an adapter is installed, and the early return below stays as the second guard
/// for that run.
#[test]
#[ignore = "needs codelldb or lldb-dap on PATH, or UNLUMINOUS_LLDB_ADAPTER"]
fn a_real_debugger_binds_a_breakpoint_in_a_file_that_is_not_open() {
    let Some(adapter) = std::env::var_os("UNLUMINOUS_LLDB_ADAPTER")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| unluminous_app::services::debuggers::on_path("codelldb"))
        .or_else(|| unluminous_app::services::debuggers::on_path("lldb-dap"))
    else {
        eprintln!(
            "skipped: no lldb adapter on this machine. `pwsh tools/get-debug-adapter.ps1` fetches \
             CodeLLDB and prints the path; point UNLUMINOUS_LLDB_ADAPTER at it."
        );
        return;
    };

    let folder = std::env::temp_dir().join("unluminous-real-debug-not-open");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(folder.join("src")).expect("make the project");

    // A second module, long enough that the breakpoint is well down it and the editor really
    // scrolls. The line to stop on is found rather than counted, so the fixture can change.
    let mut report = String::from("pub fn run() -> i64 {\n    let mut total: i64 = 0;\n");
    for step in 1..=40 {
        report.push_str(&format!("    total += {step};\n"));
    }
    report.push_str("    let answer = total;\n    answer\n}\n");
    let stop_at = report
        .lines()
        .position(|line| line.contains("let answer = total;"))
        .expect("the line to stop on")
        + 1;
    std::fs::write(folder.join("src").join("report.rs"), &report).expect("write report.rs");
    let main = folder.join("src").join("main.rs");
    std::fs::write(&main, "mod report;\n\nfn main() {\n    println!(\"{}\", report::run());\n}\n")
        .expect("write main.rs");

    let binary = folder.join(if cfg!(windows) { "fleet.exe" } else { "fleet" });
    let built = std::process::Command::new("rustc")
        .arg("-g")
        .arg("-C")
        .arg("opt-level=0")
        .arg("-o")
        .arg(&binary)
        .arg(&main)
        .output()
        .expect("run rustc");
    assert!(
        built.status.success(),
        "the fixture would not build: {}",
        String::from_utf8_lossy(&built.stderr)
    );

    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    harness.state_mut().settings.debug_adapters =
        vec![("lldb".to_owned(), adapter.to_string_lossy().to_string())];

    // **`main.rs` is the open tab and `report.rs` is not.** That is the whole of the reported case,
    // and it is what the ticket's own tell says: during the session `debug breakpoint list` printed
    // the one that bound as `src\main.rs` and the one that did not as `src/report.rs`.
    harness.state_mut().open_path_permanently(&main).expect("the file opens");
    harness.run();
    let scrolled = harness
        .state_mut()
        .run_command_line("editor scroll --bottom", &ctx)
        .expect("answered at once");
    assert!(scrolled.ok, "{}", scrolled.message);

    // Named relatively, with the separator every settings file and every agent uses.
    let set = harness
        .state_mut()
        .run_command_line(&format!("debug breakpoint add src/report.rs {stop_at}"), &ctx)
        .expect("answered at once");
    assert!(set.ok, "{}", set.message);
    assert!(
        harness.state().files.index_of(&folder.join("src").join("report.rs")).is_none(),
        "report.rs must not be open, or this is not the case that was broken"
    );

    harness.state_mut().run_configurations.add_permanent(configuration(
        "fleet",
        &unluminous_app::services::run_configurations::quote_part(&binary.to_string_lossy()),
    ));
    harness.state_mut().run_selected = Some("fleet".to_owned());
    choose(&mut harness, Action::Debug(DebugAction::Start(None)));
    assert!(
        harness.state().debug.is_some(),
        "the session should have started: {:?}",
        harness.state().message
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        harness.step();
        let debug = harness.state().debug.as_ref().expect("the session");
        if debug.is_ready() {
            break;
        }
        // This is the failure the ticket describes, so it is worth saying in those words: with the
        // fault, the session ends here having run the program to completion and stopped nowhere.
        assert!(
            debug.is_alive(),
            "the program ran to completion without stopping \u{2014} the breakpoint never bound: {:?}",
            harness.state().message
        );
        assert!(
            std::time::Instant::now() < deadline,
            "the program did not stop in sixty seconds; it is {}",
            debug.where_it_is()
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let status =
        harness.state_mut().run_command_line("debug status", &ctx).expect("answered at once");
    assert_eq!(status.result["paused"], true, "{}", status.message);
    assert_eq!(status.result["line"], stop_at, "{}", status.message);

    // And the adapter says it bound it, which is what stops the gutter drawing it hollow.
    let listed = harness
        .state_mut()
        .run_command_line("debug breakpoint list", &ctx)
        .expect("answered at once");
    let rows: Vec<String> = listed.result["lines"]
        .as_array()
        .expect("the rows")
        .iter()
        .map(|line| line.as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        rows.iter().any(|row| row.contains("verified")),
        "the debugger should have bound it: {rows:#?}"
    );

    // The value the program really computed, read out of the file that was never opened: 1+..+40.
    let variables =
        harness.state_mut().run_command_line("debug variables", &ctx).expect("answered at once");
    let printed: Vec<String> = variables.result["lines"]
        .as_array()
        .expect("the rows")
        .iter()
        .map(|line| line.as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        printed.iter().any(|line| line.contains("total") && line.contains("820")),
        "the debugger should have read `total` as 820: {printed:#?}"
    );

    harness.state_mut().run_command_line("debug stop", &ctx);
    harness.step();
}

/// The one debug test that starts a **real** adapter, and it earns it.
///
/// Everything above is a scripted session: the pictures have to be the same on every run, so they are
/// taken of a state machine that was handed fixed messages. That proves Unluminous's half of the
/// conversation and nothing about the other half. This is the other half — a real program, built
/// here, stopped at a real breakpoint by a real debugger, with a real value read out of it.
///
/// **Skipped with a message on a machine that has no adapter**, which is `task-1687` §12's own rule:
/// a skipped test that says why is honest, and a red one that lies about Unluminous is not. `lldb-dap`
/// ships inside every LLVM distribution and `winget install LLVM.LLVM` is how this machine got one.
///
/// It waits with `pump` and a deadline rather than `Harness::run`, which gives the window four steps
/// to go quiet and panics otherwise — right for a settled window and wrong while a debugger is
/// loading a binary's debug information. `task-1654`'s rule about waiting loops, once more.
#[test]
#[ignore = "needs codelldb or lldb-dap on PATH, or UNLUMINOUS_LLDB_ADAPTER"]
fn a_real_debugger_stops_at_a_breakpoint_and_reads_a_variable() {
    // `UNLUMINOUS_LLDB_ADAPTER` first, which is the test's own spelling of the `debug.lldb` setting: an
    // adapter unpacked somewhere rather than installed is the ordinary case on a machine that has
    // not got LLVM, and a test that could only find one on `PATH` would skip on a machine that
    // plainly has one.
    let Some(adapter) = std::env::var_os("UNLUMINOUS_LLDB_ADAPTER")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| unluminous_app::services::debuggers::on_path("codelldb"))
        .or_else(|| unluminous_app::services::debuggers::on_path("lldb-dap"))
    else {
        eprintln!(
            "skipped: no lldb adapter on this machine. `debug start` would say so too — \
             lldb-dap ships with LLVM (`winget install LLVM.LLVM`), and CodeLLDB's own \
             `codelldb.exe` is inside its .vsix. Point UNLUMINOUS_LLDB_ADAPTER at either one."
        );
        return;
    };

    // A ten-line program, built here, so the test carries no binary and nothing is checked in.
    let folder = std::env::temp_dir().join("unluminous-real-debug");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the project");
    let source = folder.join("counter.rs");
    std::fs::write(
        &source,
        "fn main() {\n\
        \x20   let mut total: i64 = 0;\n\
        \x20   for step in 1..=4 {\n\
        \x20       total += step;\n\
        \x20   }\n\
        \x20   let answer = total;\n\
        \x20   println!(\"{answer}\");\n\
         }\n",
    )
    .expect("write counter.rs");
    let binary = folder.join(if cfg!(windows) { "counter.exe" } else { "counter" });
    // `-g` for debug information and `-C opt-level=0` so the locals are really there: an optimised
    // build has no `answer` to read, which would make this test fail for a reason that is not Unluminous.
    let built = std::process::Command::new("rustc")
        .arg("-g")
        .arg("-C")
        .arg("opt-level=0")
        .arg("-o")
        .arg(&binary)
        .arg(&source)
        .output()
        .expect("run rustc");
    assert!(
        built.status.success(),
        "the fixture would not build: {}",
        String::from_utf8_lossy(&built.stderr)
    );

    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    // The adapter is named explicitly rather than looked for again, so the test debugs the one it
    // decided to skip on the absence of.
    harness.state_mut().settings.debug_adapters =
        vec![("lldb".to_owned(), adapter.to_string_lossy().to_string())];
    harness.state_mut().open_path_permanently(&source).expect("the file opens");
    harness.run();

    // Line 6, `let answer = total;` — after the loop, so `total` is 10 by the time it is reached.
    let set = harness
        .state_mut()
        .run_command_line(&format!("debug breakpoint add {} 6", source.display()), &ctx)
        .expect("answered at once");
    assert!(set.ok, "{}", set.message);

    harness.state_mut().run_configurations.add_permanent(configuration(
        "counter",
        &unluminous_app::services::run_configurations::quote_part(&binary.to_string_lossy()),
    ));
    harness.state_mut().run_selected = Some("counter".to_owned());
    choose(&mut harness, Action::Debug(DebugAction::Start(None)));
    assert!(
        harness.state().debug.is_some(),
        "the session should have started: {:?}",
        harness.state().message
    );

    // Sixty seconds, which is far past what lldb takes to load a ten-line binary and short enough
    // that a machine where the adapter never answers says so rather than hanging the suite.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        harness.step();
        let debug = harness.state().debug.as_ref().expect("the session");
        // Stopped **and** the stack read, which is what there being something to look at means.
        if debug.is_ready() {
            break;
        }
        assert!(
            debug.is_alive(),
            "the session ended without stopping: {:?}",
            harness.state().message
        );
        assert!(
            std::time::Instant::now() < deadline,
            "the program did not stop in sixty seconds; it is {}",
            debug.where_it_is()
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    // Where it stopped, as the debugger says it — asserted on text, which is the only thing about a
    // real adapter that is the same on every machine.
    let status =
        harness.state_mut().run_command_line("debug status", &ctx).expect("answered at once");
    assert!(status.ok, "{}", status.message);
    assert_eq!(status.result["paused"], true);
    assert_eq!(status.result["line"], 6, "{}", status.message);

    let frames =
        harness.state_mut().run_command_line("debug frames", &ctx).expect("answered at once");
    let listed = frames.result["lines"].as_array().expect("the frames");
    assert!(
        listed.iter().any(|line| line.as_str().expect("a line").contains("counter.rs:6")),
        "the top frame should be the line it stopped on: {listed:#?}"
    );

    // And the value the program really computed. `total` is 1+2+3+4.
    let variables =
        harness.state_mut().run_command_line("debug variables", &ctx).expect("answered at once");
    let printed: Vec<String> = variables.result["lines"]
        .as_array()
        .expect("the rows")
        .iter()
        .map(|line| line.as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        printed.iter().any(|line| line.contains("total") && line.contains("10")),
        "the debugger should have read `total` as 10: {printed:#?}"
    );

    // Stepping over the assignment makes `answer` the same number, which is the other half of the
    // feature: the program really moved.
    let stepped =
        harness.state_mut().run_command_line("debug step-over", &ctx).expect("answered at once");
    assert!(stepped.ok, "{}", stepped.message);
    loop {
        harness.step();
        let debug = harness.state().debug.as_ref().expect("the session");
        // Ready, not merely paused: the same distinction the first wait draws, and the reason
        // `DebugState::is_ready` exists.
        if debug.is_ready() && debug.location().map(|(_, line)| line) == Some(7) {
            break;
        }
        assert!(debug.is_alive(), "the session ended while stepping");
        assert!(std::time::Instant::now() < deadline, "the step did not land in time");
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let variables =
        harness.state_mut().run_command_line("debug variables", &ctx).expect("answered at once");
    let printed: Vec<String> = variables.result["lines"]
        .as_array()
        .expect("the rows")
        .iter()
        .map(|line| line.as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        printed.iter().any(|line| line.contains("answer") && line.contains("10")),
        "stepping over the assignment should have made `answer` 10: {printed:#?}"
    );

    // Nothing ever orphans a child on purpose, which for a debugger is two of them: the adapter and
    // the program it is holding.
    harness.state_mut().stop_debugging();
    harness.state_mut().run.kill_everything();
    std::fs::remove_dir_all(&folder).ok();
}

/// The same again with **js-debug**, which is the adapter that hands its target to a second session.
///
/// It earns a second real-adapter test rather than being folded into the one above, because what it
/// proves is different. CodeLLDB debugs on the connection it was launched on, so it never exercises
/// the handover; js-debug does nothing else. Before `startDebugging` was answered this test's program
/// printed `Debugger attached.`, every breakpoint stayed unverified, and it waited for the sixty
/// seconds and failed — which is exactly what a person driving Unluminous saw.
///
/// It also proves the address. js-debug's own default host is `localhost`, which resolves to `::1`
/// before `127.0.0.1` on macOS, so an adapter left to its default binds where `unluminous_dap` never
/// dials and this test cannot start a session at all.
///
/// **Skipped with a message on a machine with no js-debug.** There is nothing to look for on `PATH`:
/// it ships as a `.js` file in a GitHub release asset rather than as a program, which is why
/// `debug.node` has no default and why `tools/get-debug-adapter.sh` exists.
#[test]
#[ignore = "needs node and js-debug, pointed at by UNLUMINOUS_NODE_ADAPTER"]
fn a_real_node_debugger_stops_at_a_breakpoint_and_reads_a_variable() {
    let Some(adapter) = std::env::var_os("UNLUMINOUS_NODE_ADAPTER")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
    else {
        eprintln!(
            "skipped: no js-debug on this machine. `debug start` would say so too. \
             `tools/get-debug-adapter.sh node` fetches one and prints the line; then point \
             UNLUMINOUS_NODE_ADAPTER at its dapDebugServer.js."
        );
        return;
    };
    let Some(node) = unluminous_app::services::debuggers::on_path("node") else {
        eprintln!("skipped: no node on this machine, and js-debug is a script node runs.");
        return;
    };

    let folder = std::env::temp_dir().join("unluminous-real-node-debug");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the project");
    let source = folder.join("counter.js");
    // Line 6 is `return sum;`, reached once the loop has run, so `sum` is 1+2+3+4 by then.
    std::fs::write(
        &source,
        "function total(upTo) {\n\
        \x20 let sum = 0;\n\
        \x20 for (let step = 1; step <= upTo; step++) {\n\
        \x20   sum += step;\n\
        \x20 }\n\
        \x20 return sum;\n\
         }\n\
         \n\
         const answer = total(4);\n\
         console.log(answer);\n",
    )
    .expect("write counter.js");

    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    harness.state_mut().settings.debug_adapters =
        vec![("node".to_owned(), adapter.to_string_lossy().to_string())];
    harness.state_mut().open_path_permanently(&source).expect("the file opens");
    harness.run();

    let set = harness
        .state_mut()
        .run_command_line(&format!("debug breakpoint add {} 6", source.display()), &ctx)
        .expect("answered at once");
    assert!(set.ok, "{}", set.message);

    let command = format!(
        "{} {}",
        unluminous_app::services::run_configurations::quote_part(&node.to_string_lossy()),
        unluminous_app::services::run_configurations::quote_part(&source.to_string_lossy())
    );
    harness.state_mut().run_configurations.add_permanent(configuration("counter", &command));
    harness.state_mut().run_selected = Some("counter".to_owned());
    choose(&mut harness, Action::Debug(DebugAction::Start(None)));
    assert!(
        harness.state().debug.is_some(),
        "the session should have started: {:?}",
        harness.state().message
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        harness.step();
        let debug = harness.state().debug.as_ref().expect("the session");
        if debug.is_ready() {
            break;
        }
        assert!(
            debug.is_alive(),
            "the session ended without stopping: {:?}",
            harness.state().message
        );
        assert!(
            std::time::Instant::now() < deadline,
            "the program did not stop in sixty seconds; it is {}. \
             Before `startDebugging` was answered this is where it hung for ever.",
            debug.where_it_is()
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let status =
        harness.state_mut().run_command_line("debug status", &ctx).expect("answered at once");
    assert!(status.ok, "{}", status.message);
    assert_eq!(status.result["paused"], true);
    assert_eq!(status.result["line"], 6, "{}", status.message);

    let frames =
        harness.state_mut().run_command_line("debug frames", &ctx).expect("answered at once");
    let listed = frames.result["lines"].as_array().expect("the frames");
    assert!(
        listed.iter().any(|line| line.as_str().expect("a line").contains("counter.js:6")),
        "the top frame should be the line it stopped on: {listed:#?}"
    );

    let variables =
        harness.state_mut().run_command_line("debug variables", &ctx).expect("answered at once");
    let printed: Vec<String> = variables.result["lines"]
        .as_array()
        .expect("the rows")
        .iter()
        .map(|line| line.as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        printed.iter().any(|line| line.contains("sum") && line.contains("10")),
        "the debugger should have read `sum` as 10: {printed:#?}"
    );

    // And the program really moves when it is stepped, which is the stepping request going to the
    // session attached to the target rather than to the one that launched it.
    //
    // Stepped **until the line changes** rather than once, because V8 steps by statement within a
    // line: the first `next` from `return sum;` lands on line 6 again at a different column, which is
    // correct and is not something a test should assert away. Six is far more than the two it takes
    // and is a bound rather than a wait.
    let mut moved = false;
    for _ in 0..6 {
        let before =
            harness.state().debug.as_ref().expect("the session").location().map(|(_, line)| line);
        let stepped = harness
            .state_mut()
            .run_command_line("debug step-over", &ctx)
            .expect("answered at once");
        assert!(stepped.ok, "{}", stepped.message);
        loop {
            harness.step();
            let debug = harness.state().debug.as_ref().expect("the session");
            if debug.is_ready() {
                break;
            }
            assert!(debug.is_alive(), "the session ended while stepping");
            assert!(std::time::Instant::now() < deadline, "the step did not land in time");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let after =
            harness.state().debug.as_ref().expect("the session").location().map(|(_, line)| line);
        if after != before {
            moved = true;
            break;
        }
    }
    assert!(moved, "stepping should have left the line the breakpoint was on");

    harness.state_mut().stop_debugging();
    harness.state_mut().run.kill_everything();
    std::fs::remove_dir_all(&folder).ok();
}

/// A breakpoint set in one window is there in the next one, which is the whole point of writing them
/// beside the project.
///
/// This is the half a unit test cannot reach. `services::breakpoint_store` proves the file
/// round-trips and `unluminous_core::breakpoints` proves the offsets move with the text, and both passed
/// while **the reading half was not wired up at all**: `.unluminous/breakpoints.conf` was written
/// faithfully and never read back. It was found by driving a real window, which is what the fourth
/// layer of tests is for — so this is that walk, kept.
#[test]
fn a_breakpoint_is_still_there_when_the_project_is_opened_again() {
    let folder = std::env::temp_dir().join("unluminous-breakpoints-persist");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the project");
    let source = folder.join("main.rs");
    std::fs::write(&source, "fn main() {\n    let a = 1;\n    let b = a + 1;\n}\n")
        .expect("write main.rs");

    {
        let mut harness = harness_in(&folder);
        let ctx = harness.ctx.clone();
        // `restore_project` is what turns the writing on: a test neither reads nor writes a person's
        // files unless it says so, which is the rule the project state and the marks already keep.
        harness.state_mut().restore_project();
        harness.state_mut().open_path_permanently(&source).expect("the file opens");
        harness.run();
        let set = harness
            .state_mut()
            .run_command_line(&format!("debug breakpoint add {} 3", source.display()), &ctx)
            .expect("answered at once");
        assert!(set.ok, "{}", set.message);
        // Written once the pointer is up and something has changed, which is the same terms the
        // marks are written on — so the window is run until it has settled.
        for _ in 0..8 {
            harness.step();
        }
    }

    let written = std::fs::read_to_string(folder.join(".unluminous/breakpoints.conf"))
        .expect("the file should have been written");
    assert!(written.contains("breakpoint.1.path = main.rs"), "{written}");

    // A second window on the same folder, which is what opening the project again is.
    let mut harness = harness_in(&folder);
    let ctx = harness.ctx.clone();
    harness.state_mut().restore_project();
    harness.run();
    let listed = harness
        .state_mut()
        .run_command_line("debug breakpoint list", &ctx)
        .expect("answered at once");
    let rows = listed.result["breakpoints"].as_array().expect("the list");
    assert_eq!(rows.len(), 1, "the breakpoint should have come back: {listed:?}");
    assert_eq!(rows[0]["line"], 3);

    // And the open document holds it, not just the store — which is the ownership rule's other half:
    // a file that is open is owned by its `Document`.
    harness.state_mut().open_path_permanently(&source).expect("the file opens");
    harness.run();
    assert_eq!(
        harness.state().document().breakpoints().len(),
        1,
        "the tab should have come up with its dot already there"
    );
    let marks = harness.state().breakpoint_marks(harness.state().files.active_index());
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].0, 2, "paragraph 2 is the third line");

    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn typing_a_getter_into_a_javascript_class_survives_every_keystroke() {
    // The sequence from the report: the class, a blank line inside it, the closing brace, and then a
    // getter typed on the blank line.
    let mut harness = javascript_harness();
    type_and_paint(&mut harness, "class Person {\n\n}");
    let text = harness.state().document().text().to_string();
    let inside = text.find("{\n").expect("the brace and the line under it") + 2;
    harness.state_mut().command(Command::PlaceCaret { offset: inside, extend: false });
    harness.run();

    type_and_paint(&mut harness, "get");
    let offered = completions(&harness);
    assert!(
        offered.contains(&"getName".to_owned()),
        "the list should be open on the project's own names, so this test is exercising it: {offered:?}"
    );

    type_and_paint(&mut harness, " fullName() {\nreturn this.name;\n");
    assert!(
        harness.state().document().text().to_string().contains("get fullName()"),
        "and what was typed is what is there"
    );
}

#[test]
fn accepting_a_completion_inside_a_class_survives() {
    let mut harness = javascript_harness();
    type_and_paint(&mut harness, "class Person {\n\n}");
    let text = harness.state().document().text().to_string();
    let inside = text.find("{\n").expect("the brace") + 2;
    harness.state_mut().command(Command::PlaceCaret { offset: inside, extend: false });
    harness.run();
    type_and_paint(&mut harness, "get");
    assert!(!completions(&harness).is_empty(), "the list is open");

    // Tab takes the whole word, which is the acceptance that also has to replace what is to the right
    // of the caret, and the one that can add an import.
    harness.key_press(egui::Key::Tab);
    harness.run();
    harness.render().expect("paint what acceptance left behind");
    let after = harness.state().document().text().to_string();
    assert!(after.contains("get"), "something was accepted: {after:?}");
    assert!(harness.state().completion().is_none(), "and the list closed behind it");
}

#[test]
fn the_shapes_of_javascript_that_could_be_mis_sliced_survive_being_typed() {
    // Every one of these is a shape where a byte offset could land inside a character or past the end:
    // a template literal with expressions in it, a regular expression, an accented identifier, an
    // unclosed string, a comment naming a symbol, and spread syntax.
    let mut harness = javascript_harness();
    let shapes = [
        "import { getName } from './people.js';\n",
        "import * as people from './people.js';\n",
        "const greeting = `hello ${person.name} and ${getName(person)}`;\n",
        "const pattern = /get[A-Z]\\w+/g;\n",
        "class Person extends Employee {\n  static get kind() { return 'person'; }\n}\n",
        "const \u{00e9}quipe = { g\u{00e9}rant: getName };\n",
        "// a comment about getName\n",
        "const half = 'unclosed string\n",
        "const object = { get, getName, ...rest };\n",
        "async function* getEverything() { yield await getName(); }\n",
    ];
    for shape in shapes {
        harness.state_mut().command(Command::SelectAll);
        harness.run();
        type_and_paint(&mut harness, shape);
    }
}
