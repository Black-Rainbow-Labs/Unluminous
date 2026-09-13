//! The Base of Infinite Space: the canvas, its six kinds of node, and who holds the keyboard on it.
//!
//! Adding a node, dragging one, zooming the camera, wiring two together and driving one from the
//! other, and each kind of node in turn — terminal, browser, folder, file editor, chat and tasks.
//! Then the `task-1914` QA pass, four of whose seven reports were "I cannot type in X", every one of
//! them a question about which surface the keyboard is on.
//!
//! **Most of this file is assertions.** What a canvas is comes back through
//! `unluminous-cli space` and `status --section keyboard`, and a picture of a browser node never holds
//! the page anyway, because a rendered page is a native child the operating system composites on top
//! of the surface a screenshot reads back.
//!
//! **15 of the 84 tests here take a picture**, and the rest read the window's state back.

mod common;

use common::*;

use egui::{vec2, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::UnluminousApp;

// ------------------------------------------------------------- the Base of Infinite Space (`task-1904`)

/// A canvas with one node of each kind on it, wired, ready to be photographed.
///
/// **Every terminal node is detached**, which is what makes the picture the same on every run: a real
/// shell answers when it answers, and `new_detached_space_node` hands the emulator fixed bytes
/// instead. That is `new_detached_terminal_tab`'s own bargain, made for a node.
fn a_canvas() -> Harness<'static, UnluminousApp> {
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");
    let terminal = harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Terminal,
        egui::pos2(40.0, 30.0),
    );
    harness.state_mut().feed_a_space_terminal(
        terminal,
        b"$ cargo test -p unluminous-app\r\n   Compiling unluminous-app\r\n    Finished in 3.59s\r\n$ ",
    );
    let browser = harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Browser,
        egui::pos2(700.0, 30.0),
    );
    let explorer = harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Folder,
        egui::pos2(40.0, 440.0),
    );
    let editor = harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Editor,
        egui::pos2(400.0, 440.0),
    );
    harness
        .state_mut()
        .open_in_a_space_node(editor, &folder.join("readme.md"))
        .expect("the file opens in the node");
    did(&mut harness, &format!("space connect {terminal} {browser}"));
    did(&mut harness, &format!("space connect {terminal} {explorer}"));
    did(&mut harness, &format!("space connect {terminal} {editor}"));
    did(&mut harness, "space camera --fit");
    harness.run();
    harness
}

/// A canvas nobody has put anything on says what to do rather than looking broken.
#[test]
fn an_empty_canvas_says_how_to_put_something_on_it() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    harness.run();
    harness.snapshot(shot("space_empty").as_str());
}

/// One node of each of the four kinds, wired to the terminal that may drive them.
#[test]
fn a_canvas_with_one_node_of_each_kind() {
    let mut harness = a_canvas();
    harness.snapshot(shot("space_nodes").as_str());
}

/// One terminal node at its own size, which is what a canvas looks like while somebody is using it.
///
/// The overview above is the canvas fitted; this is the zoom a person actually works at, and it is
/// where the header, the two ports and the terminal's own grid are readable.
#[test]
fn a_terminal_node_at_the_size_a_person_works_at() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let terminal = harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Terminal,
        egui::pos2(60.0, 40.0),
    );
    // Carriage returns as well as line feeds, because a terminal is a grid: a line feed on its own
    // moves down without going back to the first column, and the screenshot showed exactly that as a
    // staircase. **A Rust string literal folds a real CRLF in the source down to one `\n`**, so the
    // escapes have to be written out rather than typed in.
    harness.state_mut().feed_a_space_terminal(
        terminal,
        b"$ claude\r\n\r\n  Welcome to Claude Code\r\n\r\n\
          > read crates/unluminous-app/src/app/space.rs\r\n",
    );
    let browser = harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Browser,
        egui::pos2(740.0, 40.0),
    );
    did(&mut harness, &format!("space connect {terminal} {browser} --pipe off"));
    did(&mut harness, &format!("space focus {terminal}"));
    harness.run();
    harness.snapshot(shot("space_working").as_str());
}

/// The same canvas zoomed out, which is what the camera is for: the nodes are smaller and the dot
/// grid has gone, because dots closer together than they are wide are a grey wash.
#[test]
fn the_canvas_zoomed_out() {
    let mut harness = a_canvas();
    did(&mut harness, "space camera --zoom 0.4");
    harness.run();
    harness.snapshot(shot("space_zoomed_out").as_str());
}

/// The modal a right click opens: a search field and the four kinds under it.
#[test]
fn the_add_node_modal() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    did(&mut harness, "action run space-add");
    harness.run();
    harness.snapshot(shot("space_add_modal").as_str());
}

/// With the decoration switched off the canvas draws flat, in the same frame.
///
/// The off switch is a control like any other and Unluminous's rule is that a control has a test - and
/// the flat form is a separate path through the wires and the ports, since each asks
/// `look.chrome.is_recording()` and draws the other shape when it is false.
#[test]
fn the_canvas_with_its_decoration_switched_off() {
    let mut harness = a_canvas();
    did(&mut harness, "settings set plugins.chrome false");
    harness.run();
    harness.snapshot(shot("space_flat").as_str());
}

/// Everything a person can do on the canvas, done from the command line instead.
///
/// Unluminous's first rule is that an agent reaches the same code by the same path, so this drives the
/// whole area and asserts on what the window really holds afterwards rather than on what the replies
/// said.
#[test]
fn the_canvas_can_be_read_and_changed_entirely_from_the_command_line() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    assert!(harness.state().space.visible, "`space show` really shows it");

    // Two nodes, placed and sized by hand.
    let first = did(&mut harness, "space add folder --x 40 --y 40 --width 300 --height 320");
    let node = first["node"].as_u64().expect("a node id");
    did(&mut harness, &format!("space move {node} --x 120 --y 60"));
    let sized = did(&mut harness, &format!("space size {node} --width 260 --height 200"));
    assert_eq!(sized["width"], 260.0);
    did(&mut harness, &format!("space title {node} the project"));
    let second = did(&mut harness, "space add browser --x 500 --y 60");
    let other = second["node"].as_u64().expect("a node id");

    // A node is never made smaller than its kind allows, and the reply says what it really became.
    let squashed = did(&mut harness, &format!("space size {node} --width 10 --height 10"));
    assert!(squashed["width"].as_f64().expect("a width") > 10.0, "clamped, and it says so");
    did(&mut harness, &format!("space size {node} --width 260 --height 200"));

    // Wiring, and the permission that comes with it.
    let wired = did(&mut harness, &format!("space connect {node} {other}"));
    let edge = wired["connection"].as_u64().expect("a connection id");
    let connections = did(&mut harness, &format!("space connections --from {node}"));
    assert_eq!(connections["connections"][0]["to"], other);
    // The other way round there is no wire, so a command acting as the browser is refused.
    let refusal = refused(&mut harness, &format!("space folder {node} rows --from {other}"));
    assert_eq!(refusal, "refused");
    // And with no `--from` at all it is the window's own agent, which may reach everything.
    let rows = did(&mut harness, &format!("space folder {node} rows"));
    assert!(rows["rows"].as_array().expect("rows").len() > 1, "the project's own files");

    // The camera.
    did(&mut harness, "space camera --x -40 --y 20 --zoom 0.75");
    let camera = harness.state().space.space.current().camera;
    assert_eq!(camera.zoom, 0.75);
    assert_eq!(camera.at, egui::pos2(-40.0, 20.0));
    // A zoom nobody could read is clamped rather than believed.
    did(&mut harness, "space camera --zoom 90");
    assert_eq!(harness.state().space.space.current().camera.zoom, 2.5);

    // The views.
    did(&mut harness, "space new-view Rendering");
    assert_eq!(harness.state().space.space.current().name, "Rendering");
    assert!(harness.state().space.space.current().nodes.is_empty(), "a fresh view is empty");
    did(&mut harness, "space open-view Main");
    assert_eq!(harness.state().space.space.current().nodes.len(), 2);
    let copied = did(&mut harness, "space duplicate-view Main");
    assert!(copied["view"].as_u64().is_some());
    assert_eq!(harness.state().space.space.current().name, "Main 2");
    assert_eq!(harness.state().space.space.current().nodes.len(), 2, "the nodes came with it");
    // By its id, because `Main 2` is two words on a command line and a name is one argument.
    let copy = harness.state().space.space.current_id();
    did(&mut harness, &format!("space rename-view {copy} Second"));
    assert_eq!(harness.state().space.space.current().name, "Second");
    did(&mut harness, "space delete-view Second");
    assert_eq!(harness.state().space.space.views().len(), 2);

    // And back on the first view, taking things away.
    did(&mut harness, "space open-view Main");
    did(&mut harness, &format!("space disconnect {edge}"));
    assert!(harness.state().space.space.current().edges.is_empty());
    did(&mut harness, &format!("space remove {other}"));
    assert_eq!(harness.state().space.space.current().nodes.len(), 1);

    // The whole canvas as data, which is what an agent reads first.
    let view = did(&mut harness, "space view");
    assert_eq!(view["views"][0]["nodes"][0]["title"], "the project");
    did(&mut harness, "space hide");
    assert!(!harness.state().space.visible);
}

/// A File Editor node is an ordinary tab whose home is that node.
///
/// Which is what makes it the editing area rather than a second editor: the same `Document`, the same
/// undo history, the same `editor` commands. The two invariants `OpenFiles` keeps are about the panes,
/// so a tab living on a node is in none of them.
#[test]
fn a_file_editor_node_is_a_tab_that_lives_on_the_node() {
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");
    let made = did(&mut harness, "space add editor --x 40 --y 40");
    let node = made["node"].as_u64().expect("a node id");
    did(&mut harness, &format!("space editor {node} readme.md"));
    harness.run();

    let index = harness.state().files.tab_in_node(node).expect("the tab lives on the node");
    assert_eq!(harness.state().files.at(index).path(), Some(folder.join("readme.md").as_path()));
    assert_eq!(harness.state().files.home_of(index).node(), Some(node));
    assert_eq!(harness.state().files.home_of(index).pane(), None, "it is in no pane");
    // Every pane still holds a tab, which is the invariant a node's tab must not break.
    for pane in 0..harness.state().files.pane_count() {
        assert!(!harness.state().files.tabs_in(pane).is_empty(), "pane {pane} is empty");
    }
    // And the editing area is unchanged: its own tab is still what `tab list` answers with.
    let tabs = did(&mut harness, "tab list");
    let on_a_node: Vec<&serde_json::Value> = tabs["tabs"]
        .as_array()
        .expect("tabs")
        .iter()
        .filter(|tab| tab["node"].as_u64() == Some(node))
        .collect();
    assert_eq!(on_a_node.len(), 1, "the node's tab is listed, and says which node it is on");

    // Taking the node away closes its tab, and the editing area still has one.
    did(&mut harness, &format!("space remove {node}"));
    harness.run();
    assert!(harness.state().files.tab_in_node(node).is_none());
    assert!(!harness.state().files.is_empty(), "the window always has a tab to type into");
}

/// Where a world point is drawn, for a test that has to press one.
fn on_the_canvas(harness: &Harness<'static, UnluminousApp>, world: egui::Pos2) -> egui::Pos2 {
    let body = harness.state().space.body;
    harness.state().space.space.current().camera.to_screen(body.min, world)
}

/// Dragging a node's header moves the node, and nothing else on the canvas moves with it.
#[test]
fn dragging_a_nodes_header_moves_the_node() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let made = did(&mut harness, "space add folder --x 60 --y 60 --width 300 --height 240");
    let node = made["node"].as_u64().expect("a node id");
    harness.run();

    let was = harness.state().space.space.current().camera;
    let from = on_the_canvas(&harness, egui::pos2(180.0, 72.0));
    let to = egui::pos2(from.x + 150.0, from.y + 90.0);
    drag(&mut harness, from, to);

    let now = harness.state().space.space.current().node(node).expect("it is there").at;
    assert!((now.x - 210.0).abs() < 2.0, "it moved across: {now:?}");
    assert!((now.y - 150.0).abs() < 2.0, "and down: {now:?}");
    assert_eq!(harness.state().space.space.current().camera, was, "the canvas itself did not move");
}

/// Dragging a node's edge resizes it and leaves the opposite edge exactly where it was.
///
/// The case that is wrong in every implementation that keeps a place and a size and forgets one of
/// them, which is why `geometry::resized` is a function with its own test - and this is that
/// arithmetic reached through a real pointer.
#[test]
fn dragging_a_nodes_left_edge_moves_that_edge_and_no_other() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let made = did(&mut harness, "space add folder --x 200 --y 60 --width 300 --height 240");
    let node = made["node"].as_u64().expect("a node id");
    harness.run();
    let was = harness.state().space.space.current().node(node).expect("it is there").rect();

    let from = on_the_canvas(&harness, egui::pos2(200.0, 180.0));
    drag(&mut harness, from, egui::pos2(from.x - 60.0, from.y));

    let now = harness.state().space.space.current().node(node).expect("it is there").rect();
    assert!((now.left() - 140.0).abs() < 2.0, "the left edge moved: {now:?}");
    assert!((now.right() - was.right()).abs() < 0.01, "the right edge did not");
    assert!((now.top() - was.top()).abs() < 0.01 && (now.bottom() - was.bottom()).abs() < 0.01);
}

/// Pulling a wire out of one node's output port and letting it go over another's input connects them.
#[test]
fn pulling_a_wire_from_one_port_to_another_connects_the_two_nodes() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let first = did(&mut harness, "space add folder --x 40 --y 60 --width 260 --height 200");
    let second = did(&mut harness, "space add folder --x 500 --y 60 --width 260 --height 200");
    let (from, to) =
        (first["node"].as_u64().expect("a node id"), second["node"].as_u64().expect("a node id"));
    harness.run();
    assert!(harness.state().space.space.current().edges.is_empty());

    // Out of the first node's output port, which is the middle of its right hand edge, and into the
    // second node's input port, which is the middle of its left one.
    let out = on_the_canvas(&harness, egui::pos2(300.0, 160.0));
    let into = on_the_canvas(&harness, egui::pos2(500.0, 160.0));
    drag(&mut harness, out, into);

    let edges = &harness.state().space.space.current().edges;
    assert_eq!(edges.len(), 1, "one wire, from the port that was pulled to the port it landed on");
    assert_eq!(edges[0].from, from);
    assert_eq!(edges[0].to, to);
    assert!(harness.state().space.space.may_reach(from, to), "and it grants what a wire grants");
}

/// A wire let go over empty canvas connects nothing, which is the promise every drag in Unluminous makes.
#[test]
fn a_wire_let_go_over_nothing_is_a_drag_that_was_thought_better_of() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    did(&mut harness, "space add folder --x 40 --y 60 --width 260 --height 200");
    did(&mut harness, "space add folder --x 500 --y 60 --width 260 --height 200");
    harness.run();

    let out = on_the_canvas(&harness, egui::pos2(300.0, 160.0));
    drag(&mut harness, out, egui::pos2(out.x + 40.0, out.y + 160.0));
    assert!(harness.state().space.space.current().edges.is_empty(), "nothing was connected");
}

/// Dragging the empty canvas pans it, and the point under the pointer comes with it.
#[test]
fn dragging_the_empty_canvas_pans_it() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    harness.run();
    let body = harness.state().space.body;
    let held = egui::pos2(body.center().x, body.center().y);
    let was = harness.state().space.space.current().camera.to_world(body.min, held);

    drag(&mut harness, held, egui::pos2(held.x - 80.0, held.y + 40.0));

    let camera = harness.state().space.space.current().camera;
    let now = camera.to_world(body.min, egui::pos2(held.x - 80.0, held.y + 40.0));
    assert!(
        (now - was).length() < 1.0,
        "the point under the pointer came with it: {was:?} to {now:?}"
    );
}

/// A terminal node is called after the command it runs, not after the program that started it.
///
/// Measured on a live window: npm's `codex` is a batch file, which has to be started through
/// `cmd.exe`, so the node came up called `cmd.exe`. The command is what a person typed and it does
/// not change under them while the program sets a title of its own, which `claude` does on every
/// prompt.
#[test]
fn a_terminal_node_is_called_after_the_command_it_runs() {
    use unluminous_app::services::space::{Kind, State};
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    harness.state_mut().space.space.change(node, |state| {
        if let State::Terminal(terminal) = state {
            // What `codex` really becomes on Windows: `cmd.exe /c C:/nvm4w/nodejs/codex.cmd`.
            terminal.command = "codex --search".to_owned();
        }
    });
    harness.run();
    let found = harness.state().space.space.current().node(node).expect("it is there").clone();
    assert_eq!(harness.state().name_of_a_node(&found), "codex");

    // A name somebody typed still wins, which is the rule a terminal tab's name already keeps.
    harness.state_mut().space.space.title_node(node, "the reviewer");
    let found = harness.state().space.space.current().node(node).expect("it is there").clone();
    assert_eq!(found.title, "the reviewer");

    // And a node with no command of its own is called after the shell that is running in it.
    let shell =
        harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(700.0, 40.0));
    harness.run();
    let found = harness.state().space.space.current().node(shell).expect("it is there").clone();
    assert!(!harness.state().name_of_a_node(&found).is_empty());
}

/// A file opened while the canvas has the keyboard lands in the editing area, not on the node.
///
/// Measured on a live window: with a File Editor node chosen, `browser open` put the page **inside**
/// that node, where nothing drew it and nothing could reach it. A node shows one thing, put there
/// deliberately by `space editor`; every other way of opening a tab is somebody asking for the
/// editing area.
#[test]
fn a_tab_opened_while_a_node_has_the_keyboard_goes_to_the_editing_area() {
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");
    let made = did(&mut harness, "space add editor --x 40 --y 40");
    let node = made["node"].as_u64().expect("a node id");
    did(&mut harness, &format!("space editor {node} readme.md"));
    harness.run();
    assert_eq!(harness.state().files.focus().node(), Some(node), "the node has the keyboard");

    // A second file, opened the ordinary way.
    harness.state_mut().open_path_permanently(&folder.join("notes.txt")).expect("it opens");
    harness.run();
    let opened = harness.state().files.index_of(&folder.join("notes.txt")).expect("it is open");
    assert_eq!(harness.state().files.home_of(opened).pane(), Some(0), "in the editing area");
    // And the node is still showing the file it was given.
    let on_the_node = harness.state().files.tab_in_node(node).expect("the node kept its tab");
    assert_eq!(
        harness.state().files.at(on_the_node).path(),
        Some(folder.join("readme.md").as_path())
    );
}

/// Every panel shows with the editing area hidden, whichever edges they are on.
///
/// `task-1905`: *"The panels for database explorer, agent tasks, and agent chat, don't show when editing
/// area is toggled off."* `fill_the_depth` gave the two strips the whole height, so the band the left and
/// right columns live in came out with none — and the drawing then skipped them for being under a point
/// tall while their rail buttons stayed lit.
#[test]
fn every_panel_shows_with_no_editing_area() {
    let mut harness = harness("");
    // One panel on a strip and one on a column, which is the arrangement that broke. The canvas along the
    // bottom, the explorer down the left.
    did(&mut harness, "space show");
    // **A detached node rather than `space add terminal`.** `task-1922`: that command starts a real
    // shell, and this picture then holds whatever PowerShell had printed by the frame it was taken
    // on -- its version banner, or nothing at all, depending on the machine and the moment. Measured
    // here: the same commit produced both. It is the rule the terminal's own screenshot tests have
    // kept since `task-1654`, applied to a node.
    harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Terminal,
        egui::pos2(40.0, 30.0),
    );
    did(&mut harness, "action run toggle-editor");
    harness.run();

    // Both are on the screen, which is what the arithmetic used to make impossible.
    let explorer = harness.state().panel_area(unluminous_app::app::dock::Panel::Explorer);
    let canvas = harness.state().panel_area(unluminous_app::app::dock::Panel::Space);
    assert!(explorer.height() > 1.0, "the explorer is {explorer:?}");
    assert!(canvas.height() > 1.0, "the canvas is {canvas:?}");
    harness.snapshot(shot("space_with_every_panel_and_no_editing_area").as_str());
}

/// A browser node draws its toolbar before it has a page, with the three buttons dimmed.
#[test]
fn a_browser_node_draws_its_toolbar_before_it_has_a_page() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Browser, egui::pos2(40.0, 30.0));
    did(&mut harness, &format!("space size {node} --width 760 --height 420"));
    harness.run();
    harness.snapshot(shot("space_browser_empty").as_str());
}

/// A File Editor node with three tabs, and one of them chosen.
#[test]
fn an_editor_nodes_tabs() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space size {node} --width 800 --height 420"));
    did(&mut harness, &format!("space editor {node} readme.md"));
    did(&mut harness, &format!("space editor {node} notes.txt"));
    did(&mut harness, &format!("space editor {node} program.rs"));
    harness.run();
    harness.snapshot(shot("space_editor_node_tabs").as_str());
}

/// A folder node dragged so that it would cover the rail, with its rows stopping at the rail's edge.
///
/// `task-1905`: *"Folder view node is going over the top of the left bar with icons, but terminal node
/// isn't."* `Ui::set_clip_rect` replaces rather than intersects, so the explorer's row list threw away the
/// clip `clip_for_nodes` worked out — and the rows were the only part of it that escaped, because
/// everything else there paints through `painter_at`, which intersects.
#[test]
fn a_folder_node_stops_at_the_rail() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Folder, egui::pos2(0.0, 20.0));
    did(&mut harness, &format!("space size {node} --width 320 --height 380"));
    // Left of the canvas's own left edge, so part of the node is over the rail.
    did(&mut harness, &format!("space move {node} --x -90 --y 20"));
    harness.run();
    harness.snapshot(shot("space_folder_node_over_the_rail").as_str());
}

/// A File Editor node holds more than one tab, and closing the last leaves it asking for a file.
///
/// `task-1905`: *"This should be just like our editing area, where I can see and edit files in multiple
/// tabs."* It held one — `open_in_a_space_node` closed whatever was there before opening the next — so
/// this fails on the code as it was, where the second `space editor` left one tab rather than two.
#[test]
fn an_editor_node_holds_more_than_one_tab() {
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add editor --x 40 --y 40")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space editor {node} readme.md"));
    did(&mut harness, &format!("space editor {node} notes.txt"));
    harness.run();

    let tabs = harness.state().files.tabs_in_node(node);
    assert_eq!(tabs.len(), 2, "both files are open in the node");
    // The second one is the one showing, because opening a file shows it.
    let showing = harness.state().files.tab_in_node(node).expect("one is showing");
    assert_eq!(harness.state().files.at(showing).path(), Some(folder.join("notes.txt").as_path()));

    // And asking for one that is already there shows it rather than opening it twice.
    did(&mut harness, &format!("space editor {node} readme.md"));
    harness.run();
    assert_eq!(harness.state().files.tabs_in_node(node).len(), 2, "no third tab");
    let showing = harness.state().files.tab_in_node(node).expect("one is showing");
    assert_eq!(harness.state().files.at(showing).path(), Some(folder.join("readme.md").as_path()));
}

/// A node's font size reaches every tab shown in it, and a tab that leaves goes back to the window's.
///
/// Three faults in one, all found by the Codex Sol review of `task-1905`. The size was remembered against the
/// **node**, so the second tab shown in it was never restyled — the node's cache already held the wanted
/// size. A tab dragged back into a pane kept the node's size for ever. And putting a node back to the
/// window's own size skipped `set_base_style` altogether, leaving the document visibly zoomed while the state
/// reported the default. It is remembered against the **tab** now, because a document is what carries a base
/// style.
#[test]
fn a_nodes_font_reaches_every_tab_in_it_and_leaves_with_none_of_them() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space size {node} --width 700 --height 380"));
    did(&mut harness, &format!("space editor {node} readme.md"));
    did(&mut harness, &format!("space editor {node} notes.txt"));
    harness.run();

    // A size of the node's own, well clear of the window's.
    did(&mut harness, &format!("space zoom {node} --factor 30"));
    harness.run();
    harness.run();
    let showing = harness.state().files.tab_in_node(node).expect("one is showing");
    assert_eq!(
        harness.state().files.at(showing).sized_at,
        Some(30.0),
        "the tab showing was resized"
    );

    // **The other tab in the node**, shown by name: it has to be resized too.
    let tabs = harness.state().files.tabs_in_node(node);
    let other = tabs.iter().copied().find(|index| *index != showing).expect("a second tab");
    harness.state_mut().files.show(other);
    harness.run();
    harness.run();
    assert_eq!(
        harness.state().files.at(other).sized_at,
        Some(30.0),
        "the second tab shown in the node was never restyled",
    );

    // **A tab that leaves goes back to the window's own font.**
    assert!(harness.state_mut().files.drag_tab(other, 0, 0));
    harness.run();
    harness.run();
    let moved = harness
        .state()
        .files
        .tabs_in(0)
        .into_iter()
        .find(|index| harness.state().files.at(*index).sized_at.is_some());
    assert_eq!(moved, None, "a tab in a pane is set in the window's own font");

    // **And putting the node back to the window's size really restyles.**
    did(&mut harness, &format!("space zoom {node} --reset"));
    harness.run();
    harness.run();
    let showing = harness.state().files.tab_in_node(node).expect("one is showing");
    assert_eq!(harness.state().files.at(showing).sized_at, None, "reset put it back");
}

/// Closing a node closes every tab on it, and so does deleting the view it is on.
///
/// Found by the Codex Sol review of `task-1905`: both closed only the tab that was **showing**, so the rest
/// were left with a `Home::Node` naming a node that had gone — reachable from nothing, drawn by nothing, and
/// still holding whatever had been typed into them.
#[test]
fn closing_a_node_closes_every_tab_on_it() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space editor {node} readme.md"));
    did(&mut harness, &format!("space editor {node} notes.txt"));
    did(&mut harness, &format!("space editor {node} program.rs"));
    harness.run();
    assert_eq!(harness.state().files.tabs_in_node(node).len(), 3);

    did(&mut harness, &format!("space remove {node}"));
    harness.run();
    assert!(
        harness.state().files.tabs_on_nodes().is_empty(),
        "every tab on the node went with it, and none was orphaned",
    );
    // And the editing area still has one, which is `close`'s own promise.
    assert!(harness.state().files.iter().any(|file| file.home.pane().is_some()));

    // The same for deleting a whole view, which closes every node on it.
    did(&mut harness, "space new-view Second");
    let other = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space editor {other} readme.md"));
    did(&mut harness, &format!("space editor {other} notes.txt"));
    harness.run();
    assert_eq!(harness.state().files.tabs_in_node(other).len(), 2);
    did(&mut harness, "space delete-view Second");
    harness.run();
    assert!(
        harness.state().files.tabs_on_nodes().is_empty(),
        "and deleting the view took them too"
    );
}

/// A File Editor node can be typed into.
///
/// `task-1914`: *"Im unable to edit files in file editor. I should be able to type, etc."*
///
/// `show_editor` asked whether `Focus` was `Focus::Editor` before it read a key, and clicking in a node
/// leaves it at `Focus::Space` — so the click frame placed the caret and every frame after it dropped the
/// key. What it asks now is **where the tab being drawn lives**: a pane answers to `Focus::Editor` and a
/// node to `Focus::Space`, which is what `focused` already decided for it.
///
/// The keys are given to the window rather than to a synthesised press inside the node, because a node's
/// contents are drawn into a transformed sublayer — the reason `open_from_a_folder_node_for_a_test` exists.
/// What is asserted is the document, which is the thing the report is about.
#[test]
fn a_file_editor_node_takes_the_keyboard_and_the_letters_reach_its_file() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space size {node} --width 620 --height 320"));
    did(&mut harness, &format!("space editor {node} notes.txt"));
    // Which is what a click in the node does: it chooses the node and hands the keyboard to the canvas.
    did(&mut harness, &format!("space focus {node}"));
    harness.run();
    let index = harness.state().files.tab_in_node(node).expect("the node has the file");
    let was = harness.state().files.at(index).document.text().to_string();

    harness.input_mut().events.push(egui::Event::Text("Z".to_owned()));
    harness.step();
    harness.run();
    let index = harness.state().files.tab_in_node(node).expect("the node still has the file");
    let now = harness.state().files.at(index).document.text().to_string();
    assert_ne!(now, was, "the letter reached the node's own file");
    assert!(now.contains('Z'), "and it is the letter that was typed: {now:?}");

    // And a pane is left alone by the same press, because the keyboard is the canvas's.
    let in_a_pane = harness
        .state()
        .files
        .iter()
        .filter(|file| file.home.pane().is_some())
        .all(|file| !file.document.text().to_string().contains('Z'));
    assert!(in_a_pane, "the editing area behind the canvas took none of it");
}

/// A tab dropped on the empty canvas breaks out into a File Editor node of its own.
///
/// `task-1914`: *"I should be able to drag tabs onto the canvas and it break out into a new node."*
///
/// The drop is made through the one function the pointer reaches — `break_a_tab_out_onto_the_canvas`,
/// which `settle_the_tab_drag` calls — rather than through a synthesised pointer, because the canvas draws
/// its nodes into transformed sublayers and a press at the rectangle the accessibility tree reports lands
/// where the widget in the layer's own coordinates is not. That is `a_tab_is_dragged_between_a_node_and_a_pane`'s
/// own arrangement.
#[test]
fn a_tab_dropped_on_the_empty_canvas_becomes_a_node() {
    use unluminous_app::app::files::Home;
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");
    did(&mut harness, "tab open readme.md --permanent");
    harness.run();
    let carried = harness.state().files.index_of(&folder.join("readme.md")).expect("it is open");
    assert!(harness.state().files.at(carried).home.pane().is_some(), "it starts in a pane");
    let nodes_before = harness.state().space.space.current().nodes.len();

    let middle = harness.state().space.body.center();
    let node = harness.state_mut().break_a_tab_out_onto_the_canvas(carried, middle);
    harness.run();
    assert_eq!(
        harness.state().space.space.current().nodes.len(),
        nodes_before + 1,
        "a node was made where it was let go",
    );
    let moved =
        harness.state().files.index_of(&folder.join("readme.md")).expect("it is still open");
    assert_eq!(harness.state().files.at(moved).home, Home::Node(node), "and the tab lives on it");
    // The node is under the pointer rather than starting at it, so what is where the drop happened is the
    // node's own header - the part it is dragged by.
    let made = harness.state().space.space.current().node(node).cloned().expect("the node");
    let camera = harness.state().space.space.current().camera;
    let on_screen = camera.rect_to_screen(harness.state().space.body.min, made.rect());
    assert!(
        on_screen.contains(middle),
        "the node covers the point it was let go at: {on_screen:?}"
    );

    // And the editing area still has a tab, which is `move_to_node`'s own promise.
    assert!(harness.state().files.iter().any(|file| file.home.pane().is_some()));
}

/// A file dropped on the canvas opens as a node, and one dropped on a node opens as a tab there.
///
/// `task-1914`: *"Folder explorer - I should be able to drag a file onto the canvas to have it open into a
/// new file editor node. Or if I drag to existing file node, it should open the file in a new tab."*
///
/// Both go through `drop_a_file_onto_the_canvas`, which is what `settle_the_file_drag` calls when a row
/// carried out of the explorer or out of a Folder node is let go — the same split `task-1673` gave the tab
/// drag, because the list a row was picked up in cannot know about a node it has never heard of.
#[test]
fn a_file_dropped_on_the_canvas_opens_as_a_node_or_as_a_tab() {
    use unluminous_app::app::files::Home;
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");
    harness.run();
    let nodes_before = harness.state().space.space.current().nodes.len();

    // Nothing under the pointer: a node of its own, holding the file.
    let middle = harness.state().space.body.center();
    let made = harness
        .state_mut()
        .drop_a_file_onto_the_canvas(&folder.join("readme.md"), middle)
        .expect("a node was made");
    harness.run();
    assert_eq!(harness.state().space.space.current().nodes.len(), nodes_before + 1);
    let opened = harness.state().files.index_of(&folder.join("readme.md")).expect("it is open");
    assert_eq!(harness.state().files.at(opened).home, Home::Node(made));

    // **And every File Editor node records where it is whether or not it drew a tab strip**, which is what
    // makes the second half possible: a node showing one file draws no strip, and before `task-1914` it was
    // therefore not in the list a drop is settled against and could not be dropped on at all.
    let recorded = harness.state().node_tab_strips_were_recorded();
    let (_, over) = recorded
        .iter()
        .find(|(node, _)| *node == made)
        .copied()
        .expect("a node showing one file is still somewhere a drop can land");

    // A second file, let go over that node: a tab beside the first rather than a second node.
    let onto = harness
        .state_mut()
        .drop_a_file_onto_the_canvas(&folder.join("notes.txt"), over.center())
        .expect("it opened");
    harness.run();
    assert_eq!(onto, made, "it landed on the node it was let go over");
    assert_eq!(
        harness.state().space.space.current().nodes.len(),
        nodes_before + 1,
        "and no second node was made",
    );
    assert_eq!(harness.state().files.tabs_in_node(made).len(), 2, "two tabs on the one node");
}

/// `input` clicks, types and drags without the window being in front.
///
/// `task-1914`: *"we can't have the window take focus while testing ... right now im switched to a
/// different desktop, but get switched to another desktop with unluminous open."*
///
/// Synthetic operating system input goes to the **foreground** window, so a script that wanted to click
/// something in Unluminous had to bring Unluminous to the front — and on Windows activating a window that
/// is on another virtual desktop switches the desktop with it. What goes in instead is `egui::Event`,
/// down the control channel, fed to `RawInput` one step a frame.
///
/// **The harness has no foreground window at all**, which is what makes this the right place to test it:
/// there is nothing here that could have cheated by activating one. What is asserted is that the events
/// reach the same places a real device's do — a press in a text box, the letters after it, a key that is
/// not text, and where the pointer was left.
#[test]
fn input_clicks_and_types_without_the_window_being_in_front() {
    let mut harness = harness("");
    harness.run();
    // A click at a **place**, which is the half of the window that has no other way in: the explorer's
    // filter is an `egui::TextEdit` and it takes the keyboard from a press and from nothing else.
    let filter = harness.get_by_label("Filter files").rect();
    drove(&mut harness, &format!("input click {} {}", filter.center().x, filter.center().y));

    drove(&mut harness, "input text read");
    assert_eq!(harness.state().filter, "read", "the letters went into the box that was clicked");

    // A key, which is not text: the box takes it and loses a letter.
    drove(&mut harness, "input key Backspace");
    assert_eq!(harness.state().filter, "rea");

    // And the pointer is left where it was moved to, which is what makes something hover.
    drove(&mut harness, "input move 40 300");
    let where_it_is = harness.ctx.input(|input| input.pointer.latest_pos());
    assert_eq!(where_it_is, Some(egui::pos2(40.0, 300.0)));
}

/// A drag sent through `input` is a drag, not a click.
///
/// The two are told apart by *frames*: every drag in Unluminous is settled from `Response::drag_delta`,
/// which is the difference between two frames' pointer positions, so a press and a release in
/// consecutive frames is a click however far apart they are. `services::input::dragged` therefore moves
/// over frames of its own — and the thing that proves it arrived is a divider that really moved.
#[test]
fn a_drag_sent_through_input_moves_a_divider() {
    let mut harness = harness("");
    harness.run();
    let was = harness.state().panes.explorer_width;
    // The divider is the explorer's right hand edge — `show_the_panel_dividers` draws one there, and
    // `components::splitter` takes a drag over it.
    let panel = harness.state().panel_area(unluminous_app::app::dock::Panel::Explorer);
    let (edge, middle) = (panel.right(), panel.center().y);
    drove(
        &mut harness,
        &format!("input drag {edge} {middle} --to-x {} --to-y {middle} --steps 8", edge + 90.0),
    );
    let now = harness.state().panes.explorer_width;
    assert!(now > was + 40.0, "the drag should have widened the explorer: {was} -> {now}");
}

/// An Agent Chat node holds a chat of its own, and two of them hold two conversations.
///
/// `task-1914`: *"Agent Chat ... We want a node that is able to connect similar to our terminal with claude
/// etc so the agent knows how to control/read/etc the nodes it's connected to. Should be the exact same as
/// the agent chat pane (image uploads, etc)"*.
///
/// **The exact same pane, and its own conversation.** `components::agent_chat::pane` is the function the
/// panel draws with and it is what a node draws with, so everything that pane can do a node can do; what is
/// different is which `AgentChat` it is handed. Two views of one conversation would be one agent that cannot
/// say which node it is, which is the one thing the connection half of the ticket needs.
#[test]
fn a_chat_node_holds_its_own_conversation_and_comes_back_on_it() {
    use unluminous_app::services::space::{Kind, State};
    let mut harness = harness("");
    did(&mut harness, "space show");
    let one = did(&mut harness, "space add chat --x 20 --y 20")["node"].as_u64().expect("id");
    let two = did(&mut harness, "space add chat --x 520 --y 20")["node"].as_u64().expect("id");
    harness.run();

    assert_eq!(
        harness.state().space.space.current().node(one).expect("it is there").kind(),
        Kind::Chat
    );
    let first = harness.state().space.live.chat(one).map(|chat| chat.conversation_id().to_owned());
    let second = harness.state().space.live.chat(two).map(|chat| chat.conversation_id().to_owned());
    let first = first.expect("the node opened a chat of its own the first time it was drawn");
    let second = second.expect("and so did the second node");
    assert_ne!(first, second, "two chat nodes are two agents, not two views of one");

    // **Written down**, so a canvas comes back with each agent where it was left rather than every one of
    // them on the newest conversation, which is what the pane does because there is one of it.
    let recorded =
        match &harness.state().space.space.current().node(one).expect("it is there").state {
            State::Chat(chat) => chat.conversation.clone(),
            other => panic!("a chat node holds a chat state, not {other:?}"),
        };
    assert_eq!(recorded, first, "the node records which conversation it is on");

    // And `space list` reads it back, which is the half of Unluminous's rule that says an agent reaches
    // what a person sees.
    let listed = did(&mut harness, "space list").to_string();
    assert_eq!(
        listed.matches("\"kind\":\"chat\"").count(),
        2,
        "both nodes read back as chats: {listed}"
    );
}

/// A chat node's tool call is asked from that node, so its wires are what it may reach.
///
/// A terminal node carries `UNLUMINOUS_SPACE_NODE` in its environment and the client sends it, which is
/// what makes `space here` answer about that node and every `space` command it sends carry `--from`. A chat
/// node has no client and no environment, so the window fills the same two in before the call is run.
///
/// **Only where the command really names the key**, read from the catalogue: `task-1804`'s rule is that a
/// key a command does not name is a usage refusal, so filling one in blindly would turn `space list` into
/// an error. And only when the model did not say, so an agent that names a `--from` of its own is answered
/// or refused on its own terms.
#[test]
fn a_chat_nodes_tool_call_is_asked_from_its_own_node() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let chat = did(&mut harness, "space add chat --x 20 --y 20")["node"].as_u64().expect("id");
    harness.run();

    let asked =
        |harness: &Harness<'static, UnluminousApp>, command: &str, given: serde_json::Value| {
            let map = given.as_object().expect("an object").clone();
            harness.state().what_a_chat_node_is_asking_about(chat, command, map)
        };

    // `space here` is the one command that asks *which node is calling*.
    let here = asked(&harness, "space.here", serde_json::json!({}));
    assert_eq!(here["node"], serde_json::json!(chat), "space here is asked as this node");

    // Every other `space` command asks what the caller may reach.
    let send = asked(&harness, "space.send", serde_json::json!({ "node": 9, "text": "hello" }));
    assert_eq!(send["from"], serde_json::json!(chat), "and the rest are asked from it");
    assert_eq!(send["node"], serde_json::json!(9), "the target it named is left alone");

    // A `from` the model named is its own, and is answered or refused on its merits.
    let named = asked(&harness, "space.send", serde_json::json!({ "node": 9, "from": 3 }));
    assert_eq!(named["from"], serde_json::json!(3), "a from it named is not overwritten");

    // A command that names neither is left exactly as it is: a key a command does not have is a usage
    // refusal, so filling one in would turn a working call into an error.
    let listed = asked(&harness, "space.list", serde_json::json!({}));
    assert!(listed.is_empty(), "space list names no from and gets none: {listed:?}");
    let opened = asked(&harness, "tab.open", serde_json::json!({ "path": "readme.md" }));
    assert!(!opened.contains_key("from"), "and neither does a command outside the canvas");
}

/// A chat node is driven from the command line, through the same function the pane's commands go through.
///
/// Unluminous's rule is that everything a person can do in this window an agent can do too, through the same
/// code — so a chat node that could only be typed into would be the one surface in the window with no way
/// in. `space chat` forwards to `UiProvider::command`, which is what `plugins run agent-chat` already
/// calls, so the verbs are not a second list and the two cannot answer differently. What it adds is
/// *whose* conversation: the pane has one and each node has one of its own.
#[test]
fn a_chat_node_answers_the_command_line_about_its_own_conversation() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add chat --x 20 --y 20")["node"].as_u64().expect("id");
    let terminal =
        did(&mut harness, "space add terminal --x 700 --y 20")["node"].as_u64().expect("id");
    harness.run();

    let state = did(&mut harness, &format!("space chat {node} state"));
    assert_eq!(state["node"], serde_json::json!(node), "the answer names the node it is about");
    assert_eq!(
        state["busy"],
        serde_json::json!(false),
        "nothing has been sent, so nothing is running"
    );

    // A new conversation is a new conversation on **this** node, and the canvas records it.
    let was =
        harness.state().space.live.chat(node).expect("it opened").conversation_id().to_owned();
    let made = did(&mut harness, &format!("space chat {node} new"));
    harness.run();
    let now =
        harness.state().space.live.chat(node).expect("still there").conversation_id().to_owned();
    assert_ne!(now, was, "`new` moved it to another conversation");
    assert_eq!(made["id"], serde_json::json!(now), "and the reply named the one it moved to");
    let recorded =
        match &harness.state().space.space.current().node(node).expect("it is there").state {
            unluminous_app::services::space::State::Chat(chat) => chat.conversation.clone(),
            other => panic!("a chat node holds a chat state, not {other:?}"),
        };
    assert_eq!(recorded, now, "the canvas wrote down where the node ended up");

    // A node that is not a chat is refused by kind, which is `a_reachable_node`'s own answer.
    let refused = run(&mut harness, &format!("space chat {terminal} state"));
    assert!(!refused.ok, "a terminal node has no conversation");

    // And a verb the chat has not got is refused with the chat's own words rather than swallowed.
    let unknown = run(&mut harness, &format!("space chat {node} nonsense"));
    assert!(!unknown.ok, "an unknown verb is refused: {}", unknown.message);
}

/// An Agent Tasks node draws the window's one board.
///
/// `task-1914` asks for it beside the chat node — *"Agent Tasks - similar to agent chat."* It is
/// deliberately **not** per-node, which is written down on `Kind::Tasks`: the board is one SQLite file with
/// one watchdog behind it, so two instances would be two connections to the same tickets, each refreshing
/// without the other. Two Tasks nodes therefore show the same board, which they should, because there is one.
#[test]
fn a_tasks_node_draws_the_windows_own_board() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add tasks --x 20 --y 20")["node"].as_u64().expect("id");
    harness.run();
    assert_eq!(
        harness.state().space.space.current().node(node).expect("it is there").kind(),
        Kind::Tasks
    );
    // The node is what opened the provider: nothing has pressed the rail button and no pane is showing.
    assert!(
        harness.state().plugin_ui.view_of("agent-tasks").is_some(),
        "drawing the node opened the board, lazily, the way pressing its rail button would",
    );
}

/// A tab is dragged out of a File Editor node into a pane, and back.
///
/// `task-1905` gives a node a strip of tabs, and a strip that could not be dragged out of would be the one
/// strip in Unluminous that behaves differently from the others. `settle_the_tab_drag` is the one place a
/// tab drag lands, for the reason it exists: a tab picked up in one place is dropped in another as often as
/// not, so a node is a third kind of home in the same list rather than a second settling function.
#[test]
fn a_tab_is_dragged_between_a_node_and_a_pane() {
    use unluminous_app::app::files::Home;
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space size {node} --width 700 --height 380"));
    did(&mut harness, &format!("space editor {node} readme.md"));
    did(&mut harness, &format!("space editor {node} notes.txt"));
    harness.run();
    assert_eq!(harness.state().files.tabs_in_node(node).len(), 2);

    // **The drag is settled after every place a tab can be drawn**, which is what makes a node a real
    // target rather than a function nothing reaches. Asserted by the ordering rather than by a synthesised
    // pointer, because a node's strip is in a transformed sublayer: the Codex Sol review of `task-1905`
    // found that the settle ran between the panes and the canvas, so `node_tab_strips` was empty every
    // time it was read and neither direction of the drag could ever land.
    assert!(
        !harness.state().node_tab_strips_were_recorded().is_empty(),
        "a node with two tabs records its strip, which is what the drag is settled against",
    );

    // Out into pane zero, through the one function the drag settles with.
    let carried = harness.state().files.tabs_in_node(node)[0];
    assert!(harness.state_mut().files.drag_tab(carried, 0, 0));
    harness.run();
    assert_eq!(harness.state().files.tabs_in_node(node).len(), 1, "one left on the node");
    let moved =
        harness.state().files.index_of(&folder.join("readme.md")).expect("it is still open");
    assert_eq!(harness.state().files.at(moved).home, Home::Pane(0));

    // And back onto the node, which is the half that had no function at all before.
    assert!(harness.state_mut().files.drag_tab_to_node(moved, node, 0));
    harness.run();
    assert_eq!(harness.state().files.tabs_in_node(node).len(), 2, "both are on the node again");
    let back = harness.state().files.index_of(&folder.join("readme.md")).expect("still open");
    assert_eq!(harness.state().files.at(back).home, Home::Node(node));
    // It is the one showing, because a tab put down is a tab somebody means to look at.
    assert_eq!(harness.state().files.tab_in_node(node), Some(back));
    // And the editing area still has a tab, which is `move_to_node`'s own promise.
    assert!(harness.state().files.iter().any(|file| file.home.pane().is_some()));
}

/// A double click in a folder node opens the file in a wired File Editor node, making one if there is none.
///
/// `task-1905`: *"If I double click a file, it should open a file view node and connect it, if one isn't
/// already open, or open a new tab in the connected file view node."*
#[test]
fn a_double_click_in_a_folder_node_opens_a_wired_editor_node() {
    use unluminous_app::services::space::Kind;
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");
    let tree = did(&mut harness, "space add folder --x 40 --y 40")["node"].as_u64().expect("id");
    harness.run();

    // Nothing is wired, so `space folder open` — which is what the double click reaches — makes an editor
    // node beside it and wires it.
    let answer = did(&mut harness, &format!("space folder {tree} open --path readme.md"));
    harness.run();
    // With no editor node wired the file goes to the editing area, which is the single click's own answer;
    // the double click is what makes one. So drive the node's own path.
    assert!(answer["path"].as_str().is_some());
    harness.state_mut().open_from_a_folder_node_for_a_test(tree, &folder.join("readme.md"));
    harness.run();

    let made = harness
        .state()
        .space
        .space
        .current()
        .reaches(tree)
        .into_iter()
        .find(|node| {
            harness
                .state()
                .space
                .space
                .current()
                .node(*node)
                .is_some_and(|n| n.kind() == Kind::Editor)
        })
        .expect("an editor node was made and wired");
    let showing = harness.state().files.tab_in_node(made).expect("with the file in it");
    assert_eq!(harness.state().files.at(showing).path(), Some(folder.join("readme.md").as_path()));

    // A second double click joins that node's tabs rather than making another node.
    harness.state_mut().open_from_a_folder_node_for_a_test(tree, &folder.join("notes.txt"));
    harness.run();
    assert_eq!(harness.state().files.tabs_in_node(made).len(), 2);
    let editors = harness
        .state()
        .space
        .space
        .current()
        .nodes
        .iter()
        .filter(|node| node.kind() == Kind::Editor)
        .count();
    assert_eq!(editors, 1, "one editor node, with two tabs in it");
}

/// The modifier wheel over a node zooms that node and leaves the camera where it was.
///
/// `task-1905`: *"If I CMD/CTRL mouse wheel while hovering over a node, that node should zoom in/out,
/// rather than the entire canvas."* Both gestures reached the camera and nothing reached a node, so this
/// fails on the code as it was in both directions — the node's size did not move and the camera's did.
#[test]
fn the_modifier_wheel_over_a_node_zooms_the_node_and_not_the_camera() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    did(&mut harness, &format!("space size {node} --width 500 --height 320"));
    harness.run();
    let camera_was = harness.state().space.space.current().camera;
    let font_was =
        did(&mut harness, &format!("space font {node}"))["size"].as_f64().expect("a size");

    // The pinch, over the node. `zoom_delta` is what `Ctrl`/`Cmd` with the wheel becomes.
    let body = harness.state().space.body;
    let over_the_node = camera_was.to_screen(body.min, egui::pos2(200.0, 160.0));
    harness.input_mut().events.push(egui::Event::PointerMoved(over_the_node));
    harness.run();
    harness.input_mut().events.push(egui::Event::Zoom(1.4));
    pump(&mut harness);
    pump(&mut harness);

    let font_now =
        did(&mut harness, &format!("space font {node}"))["size"].as_f64().expect("a size");
    assert!(font_now > font_was, "the node's letters should be bigger: {font_was} -> {font_now}");
    let camera_now = harness.state().space.space.current().camera;
    assert_eq!(camera_now.zoom, camera_was.zoom, "and the canvas did not zoom with it");
    // And the window's own terminal setting is untouched, which is what makes it the node's own.
    assert_eq!(harness.state().settings.terminal_font_size, {
        let fresh = unluminous_app::settings::Settings::new();
        fresh.terminal_font_size
    });
}

/// The modifier wheel over the empty canvas still zooms the camera.
///
/// The other half of the rule above, so the change cannot quietly take the canvas's own zoom away.
#[test]
fn the_modifier_wheel_over_the_empty_canvas_still_zooms_the_camera() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    did(&mut harness, &format!("space size {node} --width 300 --height 200"));
    harness.run();
    let camera_was = harness.state().space.space.current().camera;

    // Well clear of the node, over the ground.
    let body = harness.state().space.body;
    let empty = egui::pos2(body.right() - 60.0, body.bottom() - 60.0);
    harness.input_mut().events.push(egui::Event::PointerMoved(empty));
    harness.run();
    harness.input_mut().events.push(egui::Event::Zoom(1.4));
    pump(&mut harness);
    pump(&mut harness);

    let camera_now = harness.state().space.space.current().camera;
    assert!(camera_now.zoom > camera_was.zoom, "the canvas is at {}", camera_now.zoom);
}

/// The zoom buttons step the camera and the reading puts it back to one.
///
/// `task-1905`: *"The icons for zoom in/out at the top right are not good. should be classic - + buttons
/// with cirlces around them."* There were no zoom controls at all — the `+` the report is looking at is
/// the bar's `New view` plus, which is why pressing it made a view.
#[test]
fn the_zoom_buttons_step_the_camera_and_the_reading_resets_it() {
    let mut harness = a_canvas();
    // `a_canvas` fits everything in view, so it opens at whatever zoom that took. Put it at one, which is
    // where the reading says 100%.
    did(&mut harness, "space camera --zoom 1");
    harness.run();
    let was = harness.state().space.space.current().camera.zoom;
    assert_eq!(was, 1.0);

    harness.get_by_label("Zoom in").click();
    harness.run();
    let bigger = harness.state().space.space.current().camera.zoom;
    assert!(bigger > was, "zoom in should have zoomed in, it is at {bigger}");

    harness.get_by_label("Zoom out").click();
    harness.run();
    let back = harness.state().space.space.current().camera.zoom;
    assert!((back - was).abs() < 0.001, "one notch each way is where it started, it is at {back}");

    // The reading is a button, and its name carries the number so a test reads the zoom out of the
    // accessibility tree rather than out of a picture.
    did(&mut harness, "space camera --zoom 2");
    harness.run();
    harness.get_by_label_contains("Reset zoom").click();
    harness.run();
    assert_eq!(harness.state().space.space.current().camera.zoom, 1.0);
}

/// A picture of the two buttons and the reading, at 100% and at the bottom of the ladder.
///
/// At `MIN_ZOOM` the `Zoom out` button is **dimmed** rather than absent, because it applies again the
/// moment the other one is pressed — the dimmed half of Unluminous's absent-control rule.
#[test]
fn the_canvas_zoom_controls() {
    let mut harness = a_canvas();
    harness.run();
    harness.snapshot(shot("space_zoom_controls").as_str());
    did(&mut harness, "space camera --zoom 0.25");
    harness.run();
    harness.snapshot(shot("space_zoom_controls_at_the_end_of_the_ladder").as_str());
}

/// `space here` names every node it is wired to and the command that drives each one.
///
/// `task-1905` §2: an agent in a terminal node *"doesn't seem to know that a web node is connected to
/// it"*, and the capture shows it spending nine tool calls and two shell commands working that out.
/// Everything it needed was reachable and none of it was reached, which is `CLAUDE.md`'s own distinction.
///
/// **The commands are the point rather than the node ids.** `task-1695` measured a model handed an id and
/// left to work out which of twenty-three `space` verbs applies to a browser: it reached for `bash`.
#[test]
fn space_here_names_every_node_it_is_wired_to_and_the_command_for_each() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let agent = did(&mut harness, "space add terminal --x 40 --y 40")["node"].as_u64().expect("id");
    let page = did(&mut harness, "space add browser --x 700 --y 40")["node"].as_u64().expect("id");
    let tree = did(&mut harness, "space add folder --x 40 --y 500")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space connect {agent} {page}"));
    did(&mut harness, &format!("space connect {agent} {tree}"));
    // Something wired *into* the agent as well, so the two directions are told apart.
    let other =
        did(&mut harness, "space add terminal --x 700 --y 500")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space connect {other} {agent}"));
    harness.run();

    let answer = did(&mut harness, &format!("space here --node {agent}"));
    assert_eq!(answer["node"], agent);
    assert_eq!(answer["inANode"], true);
    assert_eq!(answer["kind"], "terminal");

    let reaches = answer["reaches"].as_array().expect("what it reaches").clone();
    assert_eq!(reaches.len(), 2, "the browser and the folder");
    let for_the_page =
        reaches.iter().find(|one| one["node"] == page).expect("the browser it is wired to");
    // The command, written out with both ids in it and `--from` already there.
    assert_eq!(
        for_the_page["command"],
        format!("space browser {page} go --url <address> --from {agent}")
    );
    let for_the_tree = reaches.iter().find(|one| one["node"] == tree).expect("the folder");
    assert_eq!(for_the_tree["command"], format!("space folder {tree} rows --from {agent}"));

    // And what is wired *into* it is a separate list, because an edge is one way round.
    let reached_by = answer["reachedBy"].as_array().expect("what reaches it").clone();
    assert_eq!(reached_by.len(), 1);
    assert_eq!(reached_by[0]["node"], other);
}

/// Outside a node `space here` answers rather than refusing.
///
/// The window's own agent runs it too, and a refusal there would be a refusal about nothing — which is
/// `picture::from_the_clipboard`'s rule, where the absence is the ordinary case.
#[test]
fn space_here_outside_a_node_says_so_rather_than_refusing() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    // No `--node`, which is what a client with no `UNLUMINOUS_SPACE_NODE` sends.
    let answer = did(&mut harness, "space here");
    assert_eq!(answer["inANode"], false);
    assert!(answer["node"].is_null());
}

/// A terminal node's environment says which node it is **and** that there is a command to run.
///
/// `task-1905`: `UNLUMINOUS_SPACE_NODE` was already there and nothing suggested looking at it. An agent
/// that runs `env` — which `claude` does, and the report's capture shows it doing — reads values, and a
/// number tells it nothing it can act on.
#[test]
fn a_node_agents_environment_points_at_the_command_that_orients_it() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    harness.run();
    let settings =
        harness.state().space_terminal_settings(node, "a-fresh-id").expect("a terminal node");
    let named = |name: &str| {
        settings.env.iter().find(|(held, _)| held == name).map(|(_, value)| value.clone())
    };
    assert_eq!(named("UNLUMINOUS_SPACE_NODE").as_deref(), Some(node.to_string().as_str()));
    let hint = named("UNLUMINOUS_SPACE_HINT").expect("the sentence that points at the command");
    assert!(hint.contains("space here"), "{hint}");
    assert!(hint.contains(&node.to_string()), "{hint}");

    // **And where `unluminous-cli` is, and which window to drive.** Found by driving the real window:
    // `unluminous-cli` is on nobody's `PATH`, so `unluminous-cli space here` typed in a node answered
    // `zsh: command not found` — the very command §2 tells an agent to run first. The Agent-Tasks board
    // already carries both, and the hint names the variables rather than a bare command.
    let cli = named(unluminous_app::services::agent_tasks::agent::ENV_CLI)
        .expect("where unluminous-cli is");
    assert!(cli.ends_with("unluminous-cli") || cli.ends_with("unluminous-cli.exe"), "{cli}");
    let instance = named(unluminous_app::services::agent_tasks::agent::ENV_INSTANCE)
        .expect("which window to drive");
    assert_eq!(instance, std::process::id().to_string());
    assert!(hint.contains(unluminous_app::services::agent_tasks::agent::ENV_CLI), "{hint}");
    assert!(hint.contains(unluminous_app::services::agent_tasks::agent::ENV_INSTANCE), "{hint}");
}

/// With two folder nodes overlapping, the wheel goes to the one on top.
///
/// Found by the Codex Sol review of `task-1905`: each node asked "am I under the pointer" as it was drawn,
/// and the loop draws **back to front** — so the backmost of a stack took the wheel, and because it cleared
/// `smooth_scroll_delta` the node somebody was actually looking at got nothing. Which node owns the pointer
/// is decided once now, before any of them is drawn.
#[test]
fn the_wheel_over_two_overlapping_folder_nodes_goes_to_the_one_on_top() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let many = std::env::temp_dir().join("unluminous-folder-node-overlap");
    let _ = std::fs::create_dir_all(&many);
    for number in 0..60 {
        let _ = std::fs::write(many.join(format!("file-{number:02}.txt")), "x");
    }
    // Two nodes on the same spot. The second is added later, so it is later in the list and on top.
    let under = harness.state_mut().new_detached_space_node(Kind::Folder, egui::pos2(20.0, 20.0));
    let over = harness.state_mut().new_detached_space_node(Kind::Folder, egui::pos2(40.0, 40.0));
    for node in [under, over] {
        did(&mut harness, &format!("space size {node} --width 320 --height 300"));
        did(&mut harness, &format!("space folder {node} root --path {}", many.display()));
    }
    // Nothing chosen, so what decides is the drawing order alone.
    harness.state_mut().space.space.choose(None);
    harness.run();

    // A point inside both of them.
    let body = harness.state().space.body;
    let camera = harness.state().space.space.current().camera;
    let shared = camera.to_screen(body.min, egui::pos2(140.0, 160.0));
    harness.input_mut().events.push(egui::Event::PointerMoved(shared));
    harness.run();
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -240.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::default(),
    });
    pump(&mut harness);
    pump(&mut harness);

    assert!(
        harness.state().space.live.scroll_of(over) > 20.0,
        "the node on top should have scrolled, it is at {}",
        harness.state().space.live.scroll_of(over),
    );
    assert_eq!(
        harness.state().space.live.scroll_of(under),
        0.0,
        "and the one underneath should not have moved",
    );
    let _ = std::fs::remove_dir_all(&many);
}

/// The modifier wheel over a folder node zooms it and does **not** also scroll its rows.
///
/// One gesture must not do two things, and a folder node is where the two readings meet:
/// `wheel_over_a_folder_node` takes `smooth_scroll_delta` to scroll and `zoom_over_a_node` takes
/// `zoom_delta` to zoom. **egui is what keeps them apart** — `InputState::begin_pass` asks whether the
/// wheel's own modifiers match the zoom modifier and feeds *either* `zoom_factor_delta` *or*
/// `smooth_scroll_delta`, never both. This asserts that, because it is somebody else's invariant that this
/// code now depends on: an egui upgrade that changed it would make one notch scroll and zoom at once, and
/// nothing else here would notice.
#[test]
fn the_modifier_wheel_over_a_folder_node_zooms_it_without_also_scrolling_it() {
    use unluminous_app::services::space::{Kind, State};
    let mut harness = harness("");
    did(&mut harness, "space show");
    let many = std::env::temp_dir().join("unluminous-folder-node-modifier");
    let _ = std::fs::create_dir_all(&many);
    for number in 0..60 {
        let _ = std::fs::write(many.join(format!("file-{number:02}.txt")), "x");
    }
    let node = harness.state_mut().new_detached_space_node(Kind::Folder, egui::pos2(20.0, 20.0));
    did(&mut harness, &format!("space size {node} --width 320 --height 300"));
    did(&mut harness, &format!("space folder {node} root --path {}", many.display()));
    harness.run();
    assert_eq!(harness.state().space.live.scroll_of(node), 0.0);

    // The wheel **with the zoom modifier**, which arrives as both a `Zoom` and a `MouseWheel`.
    let body = harness.state().space.body;
    let camera = harness.state().space.space.current().camera;
    let over = camera.to_screen(body.min, egui::pos2(120.0, 160.0));
    harness.input_mut().events.push(egui::Event::PointerMoved(over));
    harness.run();
    // A wheel **with the zoom modifier**, which is the whole gesture: egui turns it into a zoom rather
    // than into a scroll. A positive delta is a zoom in.
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, 240.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::COMMAND,
    });
    pump(&mut harness);
    pump(&mut harness);

    let zoom = match &harness.state().space.space.current().node(node).expect("the node").state {
        State::Folder(folder) => folder.zoom,
        other => panic!("{other:?}"),
    };
    assert!(zoom > 1.0, "the node's rows should be bigger, its zoom is {zoom}");
    assert_eq!(
        harness.state().space.live.scroll_of(node),
        0.0,
        "and one gesture must not also scroll the rows"
    );
    let _ = std::fs::remove_dir_all(&many);
}

/// A node whose file has been deleted since comes back without it, and keeps the rest.
///
/// `project_state`'s rule for the panes — a path that is no longer a file is dropped rather than refused — and
/// a node has to keep it: the whole of that module is written so a project opens rather than complaining.
#[test]
fn a_node_whose_file_has_gone_comes_back_without_it() {
    use unluminous_app::services::space::State;
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-screenshot-gone-file");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space editor {node} readme.md"));
    did(&mut harness, &format!("space editor {node} notes.txt"));
    for _ in 0..4 {
        harness.run();
    }
    let space = harness.state().space.space.clone();
    unluminous_app::services::space::store::save(&folder, &space).expect("written");
    drop(harness);

    // One of the two files is gone, which is what a checkout, a rebase or somebody's own `rm` does.
    std::fs::remove_file(folder.join("notes.txt")).expect("the file goes");

    let mut second = harness_in(&folder);
    second.state_mut().restore_project();
    for _ in 0..8 {
        second.run();
    }
    let open: Vec<std::path::PathBuf> = second
        .state()
        .files
        .tabs_in_node(node)
        .into_iter()
        .filter_map(|index| second.state().files.at(index).path().map(std::path::Path::to_path_buf))
        .collect();
    assert!(
        open.iter().any(|path| path.ends_with("readme.md")),
        "the file that is still there should be open, and the node holds {open:?}",
    );
    assert!(
        !open.iter().any(|path| path.ends_with("notes.txt")),
        "the file that has gone should not be, and the node holds {open:?}",
    );
    // And the canvas still has its node, rather than the whole thing having been refused.
    match &second.state().space.space.current().node(node).expect("the node").state {
        State::Editor(_) => {}
        other => panic!("{other:?}"),
    }
    std::fs::remove_dir_all(&folder).ok();
}

/// Switching views and back does not take a tab off a node.
///
/// Found by driving the installed build: a canvas whose two views each had an editor node naming the same
/// file lost that tab from one of them, because `OpenFiles::open`'s rule is that a file already open is
/// *shown* rather than opened twice — so `open_in_a_space_node` **moves** the tab, and bringing a view to life
/// stole the file from the node on the view being left. Measured: three paths on one node became two after
/// switching away and back. `task-1906`.
#[test]
fn switching_views_does_not_take_a_tab_off_a_node() {
    use unluminous_app::services::space::State;
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-screenshot-two-views");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    did(&mut harness, "space show");
    let here = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space editor {here} readme.md"));
    did(&mut harness, &format!("space editor {here} notes.txt"));
    // A second view whose own editor node names one of the same files, which is what somebody working on one
    // file across two canvases really does.
    did(&mut harness, "space new-view Second");
    let there = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space editor {there} notes.txt"));
    for _ in 0..4 {
        harness.run();
    }

    // Back to the first, which is where the tab used to disappear.
    did(&mut harness, "space open-view Main");
    for _ in 0..6 {
        harness.run();
    }
    let node = harness
        .state()
        .space
        .space
        .current()
        .node(here)
        .expect("the node is on the view that is showing")
        .clone();
    match &node.state {
        State::Editor(state) => {
            assert_eq!(
                state.paths.len(),
                2,
                "the node was left holding two files and came back holding {:?}",
                state.paths,
            );
        }
        other => panic!("{other:?}"),
    }
    std::fs::remove_dir_all(&folder).ok();
}

/// The space manager lists every canvas in the project, and opening one shows it.
///
/// `task-1906`: *"i need a space/view manager modal so i can open other saved spaces/tabs."* `view_bar` has
/// always broken out of its loop when a chip would not fit, so past about six views the rest were not merely
/// hard to reach — they were not drawn and nothing said so.
#[test]
fn the_space_manager_lists_every_canvas_and_opens_one() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    did(&mut harness, "space new-view Rendering");
    did(&mut harness, "space new-view Notes");
    did(&mut harness, "space open-view Main");
    harness.run();

    did(&mut harness, "space manage");
    harness.run();
    // Every view is a row, found by name, and the one showing says so.
    for name in ["Main", "Rendering", "Notes"] {
        harness.get_by_label(&format!("Space: {name}"));
    }

    // Opening one shows it, which is what the modal is for. `Enter` on the highlighted row rather than a
    // double click, because the harness cannot synthesise one — and the arrow keys are the path a person
    // reaches for in a list they are searching anyway.
    harness.key_press(egui::Key::ArrowDown);
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().space.space.current().name, "Rendering");
    // And the modal is closed, because opening a space is finishing with the list.
    assert!(harness.state().space.managing.is_none());
}

/// A picture of the manager, and of it with a name typed into its search box.
#[test]
fn the_space_manager() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    for name in ["Rendering", "Notes", "Scratch"] {
        did(&mut harness, &format!("space new-view {name}"));
    }
    did(&mut harness, "space open-view Main");
    // Detached, so the node draws nothing a real shell decided. See
    // `every_panel_shows_with_no_editing_area` for the measurement.
    harness.state_mut().new_detached_space_node(
        unluminous_app::services::space::Kind::Terminal,
        egui::pos2(40.0, 30.0),
    );
    did(&mut harness, "space manage");
    harness.run();
    harness.snapshot(shot("space_manager").as_str());

    harness.get_by_label("Find a space").type_text("no");
    harness.run();
    harness.snapshot(shot("space_manager_filtered").as_str());
}

/// An agent node is given a conversation id, and comes back resumed onto it.
///
/// `task-1906`: *"terminal session should still have claude-code open with same session."* `Terminal::session`
/// round tripped through `space.conf` since `task-1904` and nothing ever put a value in one, so the whole
/// resume path was reachable only from a hand edited file.
///
/// **The id is one Unluminous gives, not one it reads back**, which is `services::agent_tasks`' own answer to
/// the same problem: a first run is `claude --session-id <uuid>` and a later one `claude --resume <uuid>`, so
/// the id is one Claude answers to rather than one parsed out of somebody else's stream.
///
/// **Nothing here asks the node what it recorded**, and that is `task-1922`'s correction. A node writes its
/// conversation down inside the `Ok` arm of `Session::spawn`, so that a node whose program would not start
/// is not left claiming a conversation nothing is on. Whether there is anything to read is therefore the
/// question *is `claude` installed on this machine*. It is on the machine this was written on and it is not
/// on a CI runner, so the assertion that read it passed here and failed there, on both platforms. What is
/// asserted instead is the command line, which is built with no process behind it and is where the id is
/// decided. That the recorded value agrees with what was sent is `launch::session_for`'s own unit test,
/// which pins the two halves against each other in every combination.
#[test]
fn an_agent_node_is_started_on_the_session_it_was_left_on() {
    use unluminous_app::services::space::State;
    let mut harness = harness("");
    did(&mut harness, "space show");
    // A node naming an agent that takes an id. Whether it really starts is a fact about the machine, and
    // nothing below depends on it.
    let node = did(&mut harness, "space add terminal --x 40 --y 30 --command claude")["node"]
        .as_u64()
        .expect("id");
    harness.run();

    let line = harness
        .state()
        .space_terminal_settings(node, "a-fresh-id")
        .expect("a terminal node builds a command line");
    let said = line.args.join(" ");
    // **A run that is not a resume asks for a fresh id**, which is what `Restart` means and what the real path
    // does: `new_session_id()` on every start, and only a resume reuses what was recorded. The id it is handed
    // is the id it asks for, which is the half the node then writes down.
    assert!(
        said.contains("--session-id") && said.contains("a-fresh-id"),
        "a run that is not a resume should ask for the id it was handed, and asks {said:?}",
    );

    // **Coming back, the same node resumes that conversation** rather than beginning another. The id is put
    // on the node by hand for the reason above: a node that never started has none, and what is under test
    // here is the command line built from one rather than where the one came from.
    let was = "the-conversation-it-was-left-on";
    harness.state_mut().space.space.change(node, |state| {
        if let State::Terminal(terminal) = state {
            terminal.session = was.to_owned();
        }
    });
    let resumed = harness
        .state()
        .space_terminal_settings_resuming(node)
        .expect("a terminal node builds a command line");
    let said = resumed.args.join(" ");
    assert!(
        said.contains("--resume") && said.contains(was),
        "a restored node should resume the conversation it was on, and asks {said:?}",
    );
    assert!(!said.contains("--session-id"), "and not both at once: {said:?}");
}

/// A shell node is given no conversation, because a shell has none and would refuse the argument.
#[test]
fn a_shell_node_is_given_no_session() {
    use unluminous_app::services::space::State;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add terminal --x 40 --y 30")["node"].as_u64().expect("id");
    harness.run();
    match &harness.state().space.space.current().node(node).expect("the node").state {
        State::Terminal(terminal) => {
            assert!(terminal.session.is_empty(), "a shell was handed a conversation id");
        }
        other => panic!("{other:?}"),
    }
    let line = harness.state().space_terminal_settings(node, "a-fresh-id").expect("a command line");
    let said = line.args.join(" ");
    assert!(!said.contains("--session-id"), "a shell would refuse to start: {said:?}");
}

/// Everything a node was left holding comes back: its tabs, which was showing, and where it was read.
///
/// `task-1906`: *"If I close unluminous and open back up, my spaces should be in the same state."* Three
/// fields were written to `space.conf` and read back from it since `task-1904`, and nothing ever put a value
/// in one — so a canvas came back with its nodes in the right places and its file at the top with the caret
/// at byte zero. And `Editor::path` was one path, where a node holds a strip of tabs since `task-1905`.
#[test]
fn everything_a_node_was_left_holding_comes_back() {
    use unluminous_app::services::space::{Kind, State};
    // **A folder of its own, because this test makes the window write into it.** `restore_project` is what
    // turns writing on — `remembers_this_project` — and `sample_folder` is shared behind a `OnceLock` by
    // every test that wants a project. Writing a `space.conf` into it left a canvas of nodes in a fixture
    // other tests copy, and `a_split_project_opens_split_again` then restored three editor node tabs it had
    // never opened. That is `task-1654`'s rule about a shared fixture, and this is what breaking it looks
    // like.
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-screenshot-node-state");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    did(&mut harness, "space show");
    let editor = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space editor {editor} readme.md"));
    did(&mut harness, &format!("space editor {editor} notes.txt"));
    did(&mut harness, &format!("space editor {editor} program.rs"));
    // The one in the middle is what is being read, part way down.
    let tabs = harness.state().files.tabs_in_node(editor);
    let middle = tabs[1];
    harness.state_mut().files.show(middle);
    harness
        .state_mut()
        .files
        .at_mut(middle)
        .document
        .apply(unluminous_core::Command::PlaceCaret { offset: 3, extend: false });
    let folder_node =
        harness.state_mut().new_detached_space_node(Kind::Folder, egui::pos2(700.0, 30.0));
    for _ in 0..4 {
        harness.run();
    }
    // The folder node's scroll is read back off its own `ScrollArea` every frame, so it is set after the
    // frames for the same reason the editor's is: the sample folder has seven rows and nothing to scroll.
    harness.state_mut().space.live.scroll_to(folder_node, 120.0);
    // **The scroll is set after the frames**, because the editing area clamps a tab's scroll to what its
    // document is tall enough to need — and the sample files are two lines, so drawing puts a made up scroll
    // straight back to zero. What is asserted below is that the number reaches `space.conf`, which is the
    // half `task-1906` adds; that the editor keeps a scroll a document can hold is `task-1672`'s.
    harness.state_mut().files.at_mut(middle).scroll = 42.0;
    harness.state_mut().note_where_the_nodes_are_reading();

    // Written down, which is what closing the window does.
    let space = harness.state().space.space.clone();
    unluminous_app::services::space::store::save(&folder, &space).expect("written");

    // And read back, which is what opening it again does.
    let back = unluminous_app::services::space::store::load(&folder);
    let node = back.current().node(editor).expect("the editor node came back");
    match &node.state {
        State::Editor(state) => {
            assert_eq!(state.paths.len(), 3, "every tab came back, not one of them");
            assert_eq!(state.showing, 1, "and the one that was showing is the one showing");
            assert_eq!(state.caret, 3, "at the caret it was left at");
            assert!(
                (state.scroll - 42.0).abs() < 1.0,
                "and scrolled where it was: {}",
                state.scroll
            );
            assert!(state.showing().is_some_and(|path| path.ends_with("notes.txt")), "{state:?}");
        }
        other => panic!("{other:?}"),
    }
    let node = back.current().node(folder_node).expect("the folder node came back");
    match &node.state {
        State::Folder(state) => {
            assert!(
                (state.scroll - 120.0).abs() < 1.0,
                "its rows came back scrolled: {}",
                state.scroll
            );
        }
        other => panic!("{other:?}"),
    }

    // **And a second window really opens it**, which is the half reading the file back cannot check.
    // Measured on the installed build: `space.conf` held three paths before a restart and none after it,
    // because `note_where_the_nodes_are_reading` derives what it writes from the live state and ran on the
    // frames before the canvas had been brought to life — so it wrote the empty list over the saved one.
    drop(harness);
    let mut second = harness_in(&folder);
    second.state_mut().restore_project();
    // **And opening a project changes nothing about the canvas, so nothing is written.** Bringing a view to
    // life opens each of a node's tabs in turn and every one of those calls `remember_a_nodes_tabs`, which
    // compares the tabs open *so far* against the whole saved list — so the first path made that comparison
    // say the list had changed, and `Space::change` marks the canvas dirty whatever the closure did. A window
    // that opened this project and touched nothing therefore rewrote `space.conf` with byte-identical
    // content, which is the rule `Space::is_dirty` exists to keep. The Codex Sol review found it.
    second.state_mut().bring_the_current_view_to_life();
    assert!(
        !second.state().space.space.is_dirty(),
        "opening a project asked for space.conf to be written again, having changed nothing in it",
    );
    // **And a folder node's rows come back where they were scrolled to.** This is the assertion that was
    // missing, and the fault it was hiding is the sort only a restart shows: `Folder::scroll` was written to
    // `space.conf` and read back out of it while **nothing put the number anywhere the drawing reads**, so
    // the rows came back at the top — and then the first idle frame compared the saved 120 against the live
    // 0, decided they had moved, and wrote the zero over the file. One restart lost the number and every
    // later one had nothing left to lose.
    //
    // It is asked before the frames for the reason the number is set after them above: the sample folder has
    // seven rows, so drawing clamps a scroll to what there is to scroll and would put any value back to zero.
    // What is being checked is that bringing a view to life hands the saved number over at all.
    second.state_mut().bring_the_current_view_to_life();
    assert!(
        (second.state().space.live.scroll_of(folder_node) - 120.0).abs() < 1.0,
        "the folder node's rows came back at {} rather than where they were scrolled to",
        second.state().space.live.scroll_of(folder_node),
    );
    for _ in 0..8 {
        second.run();
    }
    // **And the file it wrote still names them.** This is the assertion the fault was hiding behind: the
    // window came up, wrote an empty tab list over the saved one on its first frames, and only *then* opened
    // the tabs — so reading the canvas in memory looked right while the file on disk had been emptied. A
    // third window would then have opened nothing at all.
    let after = unluminous_app::services::space::store::load(&folder);
    match &after.current().node(editor).expect("the node is in the file").state {
        State::Editor(state) => {
            assert_eq!(
                state.paths.len(),
                3,
                "the window wrote an empty tab list over the three it had been given",
            );
        }
        other => panic!("{other:?}"),
    }
    let node = second
        .state()
        .space
        .space
        .current()
        .node(editor)
        .expect("the editor node is on the restored canvas")
        .clone();
    match &node.state {
        State::Editor(state) => {
            assert_eq!(state.paths.len(), 3, "a second window opened the node's three tabs");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        second.state().files.tabs_in_node(editor).len(),
        3,
        "and they are really open on the node rather than only named in the file",
    );
    std::fs::remove_dir_all(&folder).ok();
}

/// The last thing done on a view is recorded even when the same frame switched away from it.
///
/// What `note_where_the_nodes_are_reading` writes down is derived from the live state, and it only ever walks
/// the view that is **showing**. So a frame that both moved something and switched view — a wheel and a chip in
/// one input frame — left that movement unrecorded, because by the next frame the old view was no longer the
/// one being walked. `task-1906`, found by the Codex Sol review.
#[test]
fn what_was_done_on_a_view_is_kept_when_the_same_frame_switches_away() {
    use unluminous_app::services::space::State;
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-screenshot-view-switch");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    did(&mut harness, "space show");
    let first = harness.state().space.space.current_id();
    let editor = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space editor {editor} readme.md"));
    did(&mut harness, &format!("space editor {editor} notes.txt"));
    // A second view to switch to.
    did(&mut harness, "space new-view");
    let second = harness.state().space.space.current_id();
    assert_ne!(first, second);
    did(&mut harness, &format!("space open-view {first}"));
    for _ in 0..6 {
        harness.run();
    }

    // The caret is moved and the view is switched **with no frame in between**, which is what one input frame
    // holding both looks like from the model's side.
    let tabs = harness.state().files.tabs_in_node(editor);
    let showing = tabs[1];
    harness.state_mut().files.show(showing);
    harness
        .state_mut()
        .files
        .at_mut(showing)
        .document
        .apply(unluminous_core::Command::PlaceCaret { offset: 5, extend: false });
    harness.state_mut().space.space.show_view(second);
    for _ in 0..4 {
        harness.run();
    }

    // And the caret it was left at is what the view it was left on records.
    let kept = harness
        .state()
        .space
        .space
        .view(first)
        .expect("the view is still there")
        .nodes
        .iter()
        .find(|node| node.id == editor)
        .map(|node| node.state.clone())
        .expect("the node is still on it");
    match kept {
        State::Editor(held) => {
            assert_eq!(
                held.caret, 5,
                "the last thing done on a view was lost because the same frame switched away from it",
            );
        }
        other => panic!("{other:?}"),
    }
    std::fs::remove_dir_all(&folder).ok();
}

/// A canvas that nothing changed is not written again, however many frames go by.
///
/// `task-1906` fills in three fields from the live state every frame — a node's caret, its two scrolls and its
/// list of tabs — and `Space::change` marks the canvas dirty **whatever the closure did**. So asking inside
/// the closure would write `space.conf` sixty times a second, which is the one thing `Space::is_dirty` exists
/// to prevent. The comparison happens before `change` is called, and this is what says so.
#[test]
fn a_canvas_nothing_changed_is_not_written_again() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let editor = did(&mut harness, "space add editor --x 40 --y 30")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space editor {editor} readme.md"));
    harness.state_mut().new_detached_space_node(Kind::Folder, egui::pos2(700.0, 30.0));
    for _ in 0..4 {
        harness.run();
    }

    // Written down, so the canvas is clean.
    harness.state_mut().space.space.written();
    assert!(!harness.state().space.space.is_dirty());
    // And a run of frames with nobody touching anything leaves it clean.
    for _ in 0..6 {
        harness.run();
        assert!(
            !harness.state().space.space.is_dirty(),
            "an idle canvas asked to be written again, which is a file written sixty times a second",
        );
    }
}

/// A folder node's rows carry the same icons the panel draws.
///
/// `task-1906`: *"In folder view, I don't see the same icons i see next to the files i do in the main folder
/// pane. e.g. rust icon for rust files isn't showing to the left."* The node was handed a placeholder that
/// answered `Decoration::default()` for every row, so no plugin icon and no git colour ever reached one —
/// `task-1904` asked for the node to have the panel's own style and functionality, and this was the one
/// place it did not.
#[test]
fn a_folder_nodes_rows_carry_the_same_icons_the_panel_draws() {
    use unluminous_app::services::space::Kind;
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Folder, egui::pos2(20.0, 20.0));
    did(&mut harness, &format!("space size {node} --width 320 --height 380"));
    harness.run();

    // The Rust plugin claims `.rs` and ships an icon, so `program.rs` has one — which is the row the report
    // names. The map is what the component is handed, so that is what is asserted: a texture cannot be read
    // back out of a picture.
    let decorations = harness.state_mut().decorations_for_a_folder_node_for_a_test(node);
    let rust = folder.join("program.rs");
    let for_rust = decorations.get(&rust).expect("program.rs is a row in the node");
    assert!(for_rust.icon.is_some(), "a .rs row should carry the Rust plugin's own icon");

    // And a file no plugin claims has none rather than a wrong one, which is what makes the line above mean
    // something.
    let plain = folder.join("notes.txt");
    if let Some(for_plain) = decorations.get(&plain) {
        assert!(for_plain.icon.is_none(), "no plugin claims .txt, so there is no icon to draw");
    }
    // Beside the panel showing the same folder, which is the comparison the report makes.
    harness.run();
    harness.snapshot(shot("space_folder_node_icons").as_str());
}

/// A folder node scrolls with the wheel, and the canvas behind it does not move.
///
/// `task-1905`: *"I can't scroll the node."* No `ScrollArea` inside a node can take the wheel, because a
/// node's contents are drawn into a layer registered with `set_sublayer`, which puts the layer in the
/// order list and registers no `AreaState` — and `Context::rect_contains_pointer`, which is what a
/// `ScrollArea` asks, reads exactly that map. So the window reads the wheel and hands the offset over.
///
/// It fails on the code as it was, where the offset is zero however far the wheel is turned.
#[test]
fn a_folder_node_scrolls_with_the_wheel() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    // A folder with far more rows in it than the node is tall, so there is something to scroll. The
    // sample folder has seven, which fits.
    let many = std::env::temp_dir().join("unluminous-folder-node-scroll");
    let _ = std::fs::create_dir_all(&many);
    for number in 0..60 {
        let _ = std::fs::write(many.join(format!("file-{number:02}.txt")), "x");
    }
    let node = harness.state_mut().new_detached_space_node(Kind::Folder, egui::pos2(20.0, 20.0));
    did(&mut harness, &format!("space size {node} --width 320 --height 300"));
    did(&mut harness, &format!("space folder {node} root --path {}", many.display()));
    harness.run();
    assert_eq!(harness.state().space.live.scroll_of(node), 0.0);
    let camera_was = harness.state().space.space.current().camera;

    // The wheel over the node's own rows, which is what a person does.
    let body = harness.state().space.body;
    let over_the_rows = camera_was.to_screen(body.min, egui::pos2(120.0, 160.0));
    harness.input_mut().events.push(egui::Event::PointerMoved(over_the_rows));
    harness.run();
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -240.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::default(),
    });
    // **`pump`, not `run`.** `run` gives the window four steps to go quiet and panics otherwise, and
    // egui's own tooltip layer asks for a repaint on every frame while the pointer is scrolling — which is
    // `task-1654`'s rule about a loop that waits.
    pump(&mut harness);
    pump(&mut harness);

    let scrolled = harness.state().space.live.scroll_of(node);
    assert!(scrolled > 20.0, "the node's rows should have scrolled, they are at {scrolled}");
    // **And the canvas did not move with it.** A wheel the node took is taken out of the frame, which is
    // what `egui::ScrollArea` does when it takes one: without that, one gesture would scroll the rows and
    // pan the canvas at the same time.
    let camera_now = harness.state().space.space.current().camera;
    assert_eq!(camera_now.at, camera_was.at, "the canvas stayed where it was");
    assert_eq!(camera_now.zoom, camera_was.zoom);
    let _ = std::fs::remove_dir_all(&many);
}

/// An address typed into a browser node's bar and entered opens it.
///
/// `task-1905`, reported against the installed build: *"the web browser address bar allows me to type a
/// url and hit enter, but the url disappears and no page is loaded."* Two faults in one line, both in the
/// reading of Enter. A **singleline** `egui::TextEdit` handles `return_key` itself — it calls
/// `surrender_focus` and breaks out of its event loop, consuming the press — so on the frame Enter
/// arrives the box no longer has the focus and the key is not in the frame's input either. Asking
/// `has_focus()` was asking a condition that cannot be true, so nothing was ever sent; and the branch
/// that keeps the field showing where the page is then ran on that same frame and wiped what was typed.
///
/// It fails on the code as it was: `space browser url` answered with nothing.
#[test]
fn an_address_typed_into_a_browser_node_is_opened() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Browser, egui::pos2(40.0, 40.0));
    harness.run();

    // The control a person uses, found by its name, given the keyboard and typed into.
    //
    // **`focus()` rather than `click()`**, and the reason is the harness rather than the field: a node's
    // contents are drawn into a transformed sublayer, and `Node::click` synthesises a press at the
    // rectangle the accessibility tree reports, which is in **global** points — so the press lands
    // somewhere the widget in the layer's own coordinates is not. `focus()` asks through AccessKit and
    // needs no position. A person clicking the real field works, which the live window is where that is
    // checked.
    harness.get_by_label("Address").focus();
    harness.run();
    harness.get_by_label("Address").type_text("https://example.com/typed");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();

    // The node is pointed at it, and what is in the bar is what the page is.
    let held = harness.state().space.live.browser(node);
    match held {
        // On a platform with a browser engine the tab is made and the address is its own.
        Some(tab) => assert!(
            tab.current_url().contains("example.com/typed"),
            "the node went to {}",
            tab.current_url()
        ),
        // On one without, the refusal is about the platform rather than about the address — and the
        // typed address is still in the bar rather than having been silently thrown away.
        None => {
            // `SUPPORTED` is a compile-time constant, so clippy would rather this were a static
            // assertion — but whether this branch runs at all depends on what the harness actually
            // did, which is not known until the test runs. A supported platform reaching here is a
            // real failure and has to panic, so this stays a runtime check written as a plain `if`.
            if unluminous_app::services::browser::SUPPORTED {
                panic!("a supported platform made no tab");
            }
            let state =
                harness.state().space.space.current().node(node).cloned().expect("the node");
            match &state.state {
                unluminous_app::services::space::State::Browser(browser) => {
                    assert!(browser.typed.contains("example.com/typed"), "the address was kept");
                }
                other => panic!("{other:?}"),
            }
        }
    }
}

/// A half-typed address survives losing the focus, and using a node's toolbar selects that node.
///
/// Two findings from the Codex Sol review of `task-1905`. The branch that keeps the bar showing where the
/// page is was written as "nobody has the focus", so clicking Reload, another node or a pane threw away a
/// half-typed address — and Escape is the key that is *for* putting it back. And only clicking the page body
/// took the focus, so typing into a node that does not own the one native view left it unselected and
/// `BrowserHost` refused the navigation as "not the one showing".
#[test]
fn a_half_typed_address_survives_losing_the_focus() {
    use unluminous_app::services::space::{Kind, State};
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Browser, egui::pos2(40.0, 30.0));
    harness.state_mut().new_detached_space_page(node, "https://example.com/first").expect("a tab");
    harness.run();

    harness.get_by_label("Address").focus();
    harness.run();
    harness.get_by_label("Address").type_text("https://example.com/half");
    harness.run();
    // The focus taken away, without Enter and without Escape — which is what pressing Reload, clicking
    // another node or clicking into a pane does.
    harness.ctx.memory_mut(|memory| {
        if let Some(had) = memory.focused() {
            memory.surrender_focus(had);
        }
    });
    for _ in 0..3 {
        pump(&mut harness);
    }
    let typed = match &harness.state().space.space.current().node(node).expect("the node").state {
        State::Browser(browser) => browser.typed.clone(),
        other => panic!("{other:?}"),
    };
    assert!(
        typed.contains("/half"),
        "the half-typed address was thrown away, the bar holds {typed:?}"
    );

    // And Escape is what puts the page's own address back.
    harness.get_by_label("Address").focus();
    harness.run();
    harness.key_press(egui::Key::Escape);
    for _ in 0..3 {
        pump(&mut harness);
    }
    let typed = match &harness.state().space.space.current().node(node).expect("the node").state {
        State::Browser(browser) => browser.typed.clone(),
        other => panic!("{other:?}"),
    };
    assert!(
        typed.contains("/first"),
        "Escape should put the page's address back, the bar holds {typed:?}"
    );
}

/// A browser node's page is placed inside the node, at the size the node is drawn.
///
/// `task-1905`, reported against the installed build: *"the page itself is up and to the left of the node.
/// it should be fully contained to the node and resize/zoom/etc."* A native child view is a real window, so
/// its placement is in the **window's** own points; everything else about a node is drawn in world points
/// into a layer carrying the camera. Handed over as it came back from the component, the page was placed at
/// the node's world position.
///
/// **No screenshot could have caught this**, which is why the test is on the placement: a rendered page is a
/// native child the operating system composites on top of the surface `window screenshot` captures, so no
/// picture Unluminous takes holds one.
#[test]
fn a_browser_nodes_page_is_placed_inside_the_node() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Browser, egui::pos2(120.0, 90.0));
    did(&mut harness, &format!("space size {node} --width 520 --height 360"));
    harness
        .state_mut()
        .new_detached_space_page(node, "https://example.com/")
        .expect("a tab with no view behind it");
    harness.run();

    let body = harness.state().space.body;
    let placements = harness.state().browser_placements();
    let page = placements.first().copied().expect("the node's page was placed").area;
    // Inside the canvas, which is what "contained to the node" means at the outer edge.
    assert!(body.contains_rect(page), "the page is at {page:?} and the canvas is {body:?}");
    // And inside the node itself, under its own toolbar.
    let camera = harness.state().space.space.current().camera;
    let on_screen = camera.rect_to_screen(
        body.min,
        harness.state().space.space.current().node(node).expect("the node").rect(),
    );
    assert!(
        on_screen.contains_rect(page),
        "the page is at {page:?} and the node is at {on_screen:?}"
    );
    assert!(page.width() > 100.0 && page.height() > 100.0, "and it is a page rather than a sliver");

    let was = page;
    // **And it follows a pan**, which is the other half of the report: *"it also doesn't move around with
    // the canvas. the page just stays fixed in a single spot."*
    //
    // A pan small enough to keep the whole page on the canvas, because `task-1907` stopped drawing a page
    // that has been cut into by more than `PAGE_CROP` — `wry` narrows a native child's viewport rather than
    // cropping it, so a page laid out against that viewport reflows, and what was drawn was not a picture of
    // the page. The pan this test used to make took 80 of the page's 520 points off the left edge, which is
    // exactly the case that is now put away.
    did(&mut harness, "space camera --x 40 --y 150");
    harness.run();
    let panned = harness.state().browser_placements().first().copied().expect("still placed").area;
    assert_ne!(panned.min, was.min, "the page should have moved with the canvas");
    let node_now = camera_of(&harness).rect_to_screen(
        harness.state().space.body.min,
        harness.state().space.space.current().node(node).expect("the node").rect(),
    );
    assert!(
        node_now.contains_rect(panned),
        "after a pan the page is {panned:?} and the node {node_now:?}"
    );
    did(&mut harness, "space camera --x 0 --y 0");
    harness.run();

    // **And it follows the zoom**: a canvas at half the size draws a node half as wide, and a page that
    // kept its world size would hang out of it.
    did(&mut harness, "space camera --zoom 0.5");
    harness.run();
    let smaller = harness.state().browser_placements().first().copied().expect("still placed").area;
    assert!(smaller.width() < was.width() * 0.75, "the page was {was:?} and is now {smaller:?}");
    let on_screen = camera_of(&harness).rect_to_screen(
        harness.state().space.body.min,
        harness.state().space.space.current().node(node).expect("the node").rect(),
    );
    assert!(on_screen.contains_rect(smaller), "at half the zoom the page is {smaller:?}");
}

/// The camera the canvas is being looked at from, for the test above.
fn camera_of(harness: &Harness<'static, UnluminousApp>) -> unluminous_app::services::space::Camera {
    harness.state().space.space.current().camera
}

/// Two browser nodes: one renders and the other says so, and only the rendering one is placed.
///
/// A window has **one** native child view, which is `task-1904`'s measured rule — creating a second
/// WebView2 controller while another lives on the thread blocks in a nested message pump that never
/// returns. So a second browser node draws its toolbar and says the page is showing elsewhere, and it must
/// not hand over a placement: two placements for one view would ask the host to point it at both.
#[test]
fn only_one_browser_node_is_placed_however_many_there_are() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let first = harness.state_mut().new_detached_space_node(Kind::Browser, egui::pos2(40.0, 30.0));
    let second =
        harness.state_mut().new_detached_space_node(Kind::Browser, egui::pos2(40.0, 420.0));
    did(&mut harness, &format!("space size {first} --width 400 --height 300"));
    did(&mut harness, &format!("space size {second} --width 400 --height 300"));
    let one = harness
        .state_mut()
        .new_detached_space_page(first, "https://example.com/one")
        .expect("a tab");
    let two = harness
        .state_mut()
        .new_detached_space_page(second, "https://example.com/two")
        .expect("a tab");
    assert_ne!(one, two, "two nodes, two tabs");
    harness.run();

    // Both nodes drew, and each placed its own page — because with no view created yet
    // `BrowserHost::showing` answers `None` and every tab believes it is the one showing. What decides is
    // `reconcile`, which points the one view at the last placement it was given; the rule this asserts is
    // the weaker and true one: a placement names a tab that really exists on a node.
    let placements = harness.state().browser_placements();
    for placement in &placements {
        let (id, rect) = (placement.id, placement.visible);
        assert!(id == one || id == two, "a placement named tab {id}, which is neither node's");
        assert!(rect.width() > 1.0 && rect.height() > 1.0, "tab {id} was placed at {rect:?}");
        // **The part that may be painted, not the whole page.** Since `task-1914` a placement carries
        // both: `area` is the node wherever it is, because that is what the page lays itself out against,
        // and `visible` is what the platform crops it to. The canvas is the ceiling for the second.
        assert!(harness.state().space.body.contains_rect(rect), "tab {id} is outside the canvas");
    }
}

/// A page that finished loading says so on a **node**, and its history steps.
///
/// `task-1905` is the report, and this is the fault behind two halves of it: `browser_tab` and
/// `change_browser_tab` both walked `self.files`, and a node's tab lives in `space::live::Live::browsers`,
/// so every `BrowserEvent` was dropped for a node. The title never arrived, `loading` was set once and
/// never cleared, and `Back` answered "there is nowhere for this tab to go that way" however many pages
/// had been visited, because the history had one entry in it.
///
/// It fails on the code as it was: the assertions below are all about a tab nothing could find.
#[test]
fn a_page_that_finished_loading_says_so_on_a_node() {
    use unluminous_app::services::browser::BrowserEvent;
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Browser, egui::pos2(40.0, 40.0));
    let tab = harness
        .state_mut()
        .new_detached_space_page(node, "https://example.com/one")
        .expect("a tab with no view behind it");

    // The three the engine really sends, fed by hand because there is no engine in a test.
    harness.state_mut().act_on_browser_events(vec![
        BrowserEvent::LoadStarted { id: tab, url: "https://example.com/one".to_owned() },
        BrowserEvent::LoadFinished { id: tab, url: "https://example.com/one".to_owned() },
        BrowserEvent::Title { id: tab, title: "The First Page".to_owned() },
    ]);
    let held = harness.state().space.live.browser(node).cloned().expect("the node's tab");
    assert_eq!(held.title, "The First Page", "the title reached the node's own tab");
    assert!(!held.loading, "and it is no longer loading");

    // A second page, and the history is two deep — which is what makes `Back` mean anything.
    harness.state_mut().act_on_browser_events(vec![BrowserEvent::LoadFinished {
        id: tab,
        url: "https://example.com/two".to_owned(),
    }]);
    let held = harness.state().space.live.browser(node).cloned().expect("the node's tab");
    assert_eq!(held.current_url(), "https://example.com/two");
    assert!(held.can_go_back(), "two pages is a history");
}

/// A view chosen from the strip has everything behind its nodes running.
///
/// The `task-1904` review's second finding: choosing a view changed the model and nothing else, so a
/// terminal on it had no session, a browser no page and an editor no file. It is asked rather than
/// told — `catch_the_space_up` notices that the view showing is not the one it last brought to life —
/// which is `follow_the_open_file`'s rule, so the next way of changing a view cannot forget.
#[test]
fn choosing_a_view_starts_what_is_on_it() {
    use unluminous_app::services::space::Kind;
    let folder = sample_folder();
    let mut harness = harness("");
    did(&mut harness, "space show");

    // A second view with an editor node on it, pointed at a file.
    did(&mut harness, "space new-view Reading");
    let made = did(&mut harness, "space add editor --x 40 --y 40");
    let node = made["node"].as_u64().expect("a node id");
    did(&mut harness, &format!("space editor {node} readme.md"));
    harness.run();
    assert!(harness.state().files.tab_in_node(node).is_some());

    // Away to the first view, which closes the node's tab because the node is not on it.
    did(&mut harness, "space open-view Main");
    harness.run();

    // And back. Without the fix the node came back empty and said so.
    did(&mut harness, "space open-view Reading");
    harness.run();
    let index = harness.state().files.tab_in_node(node).expect("the node has its file again");
    assert_eq!(harness.state().files.at(index).path(), Some(folder.join("readme.md").as_path()));
    assert_eq!(harness.state().space.space.current().nodes[0].kind(), Kind::Editor);
}

/// The canvas is written down when it changes and read back when the project opens.
#[test]
fn a_canvas_comes_back_when_the_project_is_opened_again() {
    // **A folder of its own, not one inside `sample_folder()`.** This test writes a project folder and a
    // canvas file into it, and `sample_folder()` is the fixture every other test's explorer is a picture
    // of — so a subfolder created here appeared in all of them, and measured on a clean `a7902ed` it
    // failed about a hundred and thirty screenshot tests with an extra `space-round-trip` row in the
    // tree. It is also a race: the row is there or not depending on whether this test has run yet.
    //
    // `sample_folder()` is written once behind a `OnceLock` for exactly this reason, and adding to it
    // afterwards is the same fault from the other side. `git_folder(name)`'s rule — a fixture a test
    // writes to is named after that test — is what this follows.
    let folder = std::env::temp_dir().join("unluminous-space-round-trip");
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("make the folder");
    let mut space = unluminous_app::services::space::Space::new();
    let node = space.add_node(
        unluminous_app::services::space::Kind::Terminal,
        egui::pos2(120.0, 40.0),
        Some(&folder),
    );
    space.title_node(node, "the agent");
    let second = space.add_node(
        unluminous_app::services::space::Kind::Browser,
        egui::pos2(800.0, 40.0),
        Some(&folder),
    );
    space.connect(node, second, unluminous_app::services::space::Pipe::Off).expect("wired");
    unluminous_app::services::space::store::save(&folder, &space).expect("saved");

    let back = unluminous_app::services::space::store::load(&folder);
    assert_eq!(back.current().nodes.len(), 2);
    assert_eq!(back.current().nodes[0].title, "the agent");
    assert_eq!(back.current().edges.len(), 1);
    assert_eq!(back.current().nodes[0].at, egui::pos2(120.0, 40.0));
}

/// A bare host sent to a node that already has a page is given a scheme.
///
/// `task-1907`: *"the browser node is on example.com and if i type google.com and enter, nothing happens."*
/// `BrowserLocation::parse` turns `google.com` into `https://google.com/` and `send_a_space_browser_to` then
/// handed the **typed** text to the view, where wry passes an unknown scheme to `Navigate` and it is refused in
/// silence. Measured on the installed 0.39.1: the reply said the node had gone and `space browser url` said it
/// was still on the page before.
///
/// The first address a node is given always worked, because a node with no tab yet falls through to
/// `open_a_space_browser`, which passes the parsed location to `open_tab`. So this opens a page first.
#[test]
fn a_bare_host_sent_to_a_node_that_already_has_a_page_is_given_a_scheme() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add browser --x 40 --y 40")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space browser {node} go --url https://example.com/"));
    harness.run();

    did(&mut harness, &format!("space browser {node} go --url google.com"));
    harness.run();
    let answer = did(&mut harness, &format!("space browser {node} url"));
    assert_eq!(
        answer["url"], "https://google.com/",
        "the bare host was given a scheme and the tab really moved"
    );
}

/// A browser node that is not the one rendering still records where it was sent.
///
/// A window has one native view, so `BrowserHost::navigate` refuses a tab that is not the one showing — an
/// honest refusal that `task-1907` found had been throwing the address away while the node's own record was
/// changed anyway. Measured on the installed build with two browser nodes: `space list` said the node was on
/// `https://example.org/` and `space browser url` said `https://google.com/`, for ever.
#[test]
fn a_browser_node_that_is_not_rendering_still_records_where_it_was_sent() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let first = did(&mut harness, "space add browser --x 40 --y 40")["node"].as_u64().expect("id");
    let second =
        did(&mut harness, "space add browser --x 700 --y 40")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space browser {first} go --url https://example.com/"));
    did(&mut harness, &format!("space browser {second} go --url https://example.org/"));
    harness.run();

    // The first node is the one being read now, so the second is the one that cannot be driven.
    did(&mut harness, &format!("space focus {first}"));
    harness.run();
    did(&mut harness, &format!("space browser {second} go --url https://example.net/"));
    harness.run();

    let answer = did(&mut harness, &format!("space browser {second} url"));
    assert_eq!(
        answer["url"], "https://example.net/",
        "the tab knows where it should be even though the view could not be driven there"
    );
}

/// A node's own record and its page agree about where it is.
///
/// The two disagreeing is what made this hard to attribute: `space list` reads the node's state and
/// `space browser url` reads the live tab, and `send_a_space_browser_to` recorded the address before it
/// navigated — so a navigation that did not happen left the two saying different things, and `space.conf` was
/// written from the one that was wrong.
#[test]
fn a_nodes_own_record_and_its_page_agree_about_where_it_is() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add browser --x 40 --y 40")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space browser {node} go --url https://example.com/"));
    did(&mut harness, &format!("space browser {node} go --url example.org"));
    harness.run();

    let page = did(&mut harness, &format!("space browser {node} url"))["url"].clone();
    let listed = did(&mut harness, "space list")["views"][0]["nodes"]
        .as_array()
        .expect("the nodes")
        .iter()
        .find(|one| one["id"] == node)
        .expect("the browser node")["url"]
        .clone();
    assert_eq!(page, listed, "the state and the page name one address");
    assert_eq!(page, "https://example.org/");
}

/// An editor node set at its own size has a gutter at that size.
///
/// `task-1907`: *"the line numbers on the file view don't shrink the same as the text to the right of them."*
/// `UnluminousApp::gutter` read `self.settings.font_size` while every other field in it read the file, and an
/// editor node gives its own tab a size through `set_base_style` — so the letters changed and the numbers
/// beside them did not. Asserted on the gutter's **width**, because that is the observable the window computes
/// from the type size and hands to the layout.
#[test]
fn an_editor_node_at_its_own_size_has_a_gutter_at_that_size() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Editor, egui::pos2(40.0, 40.0));
    harness.run();
    did(&mut harness, &format!("space editor {node} program.rs"));
    harness.run();

    let narrow = harness.state().editor_area().left();
    did(&mut harness, &format!("space zoom {node} --factor 40"));
    harness.run();
    let wide = harness.state().editor_area().left();
    assert!(wide > narrow, "the gutter grew with the node's own font: {narrow} to {wide}");
}

/// The gutter never takes more than its share of the pane, at any type size.
///
/// This is what replaced the ceiling on the gutter's type — see `gutter::fitted_size`. `task-1693` asked that
/// *"a hundred and forty-four point text must not have a gutter wider than the editing area beside it"*, and
/// capping the type was an approximation of it that also stopped the numbers following the text at ordinary
/// sizes. Measured on the column itself, the requirement holds and the numbers are free.
#[test]
fn a_gutter_never_takes_more_than_its_share_of_the_pane() {
    // **The hardest case the harness can build**: the largest type the settings allow, five digits of line
    // number so the column is as wide as it can be, and the editing pane narrowed by a wide explorer. Every one
    // of those is what made a single proportional pass land short — the change bar, the gap and the margins are
    // fixed points that do not shrink with the letters, which the Codex Sol review of `task-1907` found.
    let many = (1..=12_000).map(|line| format!("line {line}\n")).collect::<String>();
    let mut harness = harness(&many);
    did(&mut harness, "settings set appearance.font.size 144");
    did(&mut harness, "panel size explorer --width 600");
    harness.run();

    // The editing pane is what is left of the panes area once the explorer has taken its width, and the gutter
    // is the part of it in front of the text.
    let explorer = harness.state().panel_rect_for_tests(unluminous_app::app::dock::Panel::Explorer);
    let pane_left = explorer.right();
    let pane_width = harness.state().panes_area().right() - pane_left;
    let gutter = harness.state().editor_area().left() - pane_left;
    assert!(gutter > 0.0, "there is a gutter to measure: {gutter}");
    assert!(
        gutter < pane_width * 0.45,
        "a 144 point file with five digit line numbers still leaves the text most of a narrow pane: \
         gutter {gutter} of {pane_width}"
    );
}

/// Zooming the canvas does not relayout a node, which is the promise the crispness change could have broken.
///
/// `task-1904` promises that *"a zoom costs a matrix, not a relayout"*, and `task-1907` makes a node's glyphs
/// rasterise at the size they are composited at. The one way that could go wrong is by scaling the **layout**
/// rather than the raster size — the terminal derives its rows and columns from its cell metrics, so a scaled
/// metric would change the cell count and send a resize to the program on the far side.
#[test]
fn a_zoom_does_not_relayout_a_node() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let terminal =
        harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    harness.state_mut().feed_a_space_terminal(terminal, b"$ cargo test\r\n");
    harness.run();

    let grid = |harness: &Harness<'static, UnluminousApp>| {
        let session = harness.state().space.live.terminal(terminal).expect("its session");
        (session.size().rows, session.size().columns)
    };
    let at_one = grid(&harness);

    for zoom in ["1.5", "2.0", "0.5"] {
        did(&mut harness, &format!("space camera --zoom {zoom}"));
        harness.run();
        assert_eq!(
            grid(&harness),
            at_one,
            "the terminal kept its cell count at a camera of {zoom}"
        );
    }
}

/// The canvas at 200%, which is the picture `task-1907` is about.
///
/// A node's terminal and editor text is rasterised at the size it is composited at, so the glyphs are drawn
/// rather than magnified. The furniture — the header, the gutter numbers, a folder node's rows — is `egui`
/// galleys and is still scaled, which §3.2 of the design says plainly and which this picture is the record of.
#[test]
fn space_zoomed_in() {
    let mut harness = a_canvas();
    did(&mut harness, "space camera --zoom 2.0");
    harness.run();
    harness.snapshot(shot("space_zoomed_in").as_str());
}

/// A terminal node records the program running in it, and an agent can start it again.
///
/// `task-1907`: *"if i just have a view with a terminal with claude-code open, then quit, re-open, the terminal
/// is there but no claude code."* A node's `command` is what it was **given**, and a person adds a plain
/// terminal node and types `claude` into the shell — so a canvas that recorded only the command came back as a
/// shell whatever had been running in it. Measured on the installed 0.39.1, the node read back
/// `{"command": "", "session": ""}`, which is byte for byte the shape of the reporter's own `space.conf`.
///
/// **Through a detached session**, which is what makes a terminal testable with no shell: it has no
/// pseudoterminal, so `Session::foreground` answers `None` and the node is recorded as being at a prompt. That
/// is the honest answer for a test and is asserted here rather than worked around, because a detached session
/// claiming a program would put one in every canvas a test wrote down. What a real shell reports is measured by
/// `cargo run -p unluminous-terminal --example foreground_check`.
#[test]
fn a_terminal_node_records_the_program_running_in_it() {
    use unluminous_app::services::space::{Kind, State};
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    harness.run();

    let running = |harness: &Harness<'static, UnluminousApp>| match harness
        .state()
        .space
        .space
        .current()
        .node(node)
        .map(|found| &found.state)
    {
        Some(State::Terminal(terminal)) => terminal.running.clone(),
        _ => panic!("a terminal node"),
    };
    assert_eq!(running(&harness), "", "a session with no pseudoterminal answers nothing");

    // And what a restore reads out of `space.conf` is what an agent can act on, which is the rule that a thing
    // done by hand and the same thing done by an agent are the same thing.
    harness.state_mut().space.space.change(node, |state| {
        if let State::Terminal(terminal) = state {
            terminal.running = "echo".to_owned();
        }
    });
    harness.run();
    let reply = run(&mut harness, &format!("space restart {node} --running"));
    assert!(reply.ok, "it started: {}", reply.message);
    assert!(
        reply.message.contains("echo"),
        "the reply names the program it started: {}",
        reply.message
    );
}

/// And a node that was left at a prompt is refused rather than starting something.
#[test]
fn a_node_that_was_not_left_running_anything_is_refused_with_a_sentence() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    harness.run();
    let reply = run(&mut harness, &format!("space restart {node} --running"));
    assert!(!reply.ok, "a node at a prompt has nothing to start again");
    assert!(reply.message.contains("not left running"), "{}", reply.message);
}

/// A page cut by the edge of the canvas keeps its whole width and is cropped to what is showing.
///
/// `task-1907`: *"there's an issue with the browser node. it resizes the content when it's pushed against the
/// edge of the main window. e.g. if the node itself is 50% off the page/view, the full browser page is shown but
/// resized to 50% width."* And `task-1914` again: *"if the node is halfway off the screen on the right, then the
/// page content width is 50%, rather than just have half the page not shown."*
///
/// `set_bounds` is the page's **viewport** as well as its position, so a placement cut to the pane makes a
/// responsive page relay out into what is left. Two rectangles are sent instead: `area`, which is the whole
/// node and is what the page lays itself out against, and `visible`, which is the part inside the pane and is
/// all a platform lets it paint. `services::browser`'s `clip_to_the_visible_part` is where the crop happens.
///
/// **The measurement is the placement rather than a picture**, because a native child is composited by the
/// operating system over the surface `ViewportCommand::Screenshot` captures — no screenshot Unluminous takes has
/// ever held a page. What is asserted is the two rectangles the host is handed.
#[test]
fn a_browser_page_cut_by_the_edge_keeps_its_whole_width() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Browser, egui::pos2(20.0, 20.0));
    // Small enough to sit wholly inside the canvas at the camera's home, so the first reading is a page
    // nothing has cut and the ones after it are the same page with the edge taken off.
    did(&mut harness, &format!("space size {node} --width 360 --height 220"));
    harness
        .state_mut()
        .new_detached_space_page(node, "https://example.com/")
        .expect("a tab with no view behind it");
    harness.run();
    let whole = harness.state().browser_placements().first().copied().expect("the page is drawn");
    assert_eq!(whole.visible, whole.area, "nothing is cut while the node is inside the pane");
    let width = whole.area.width();

    // Half the node off the right hand edge of the canvas. The page still lays itself out at the node's
    // own width, and only the part of it inside the pane may be painted. The **node** is moved rather than
    // the camera, so the arithmetic is one anybody reading this can check: the camera is at the origin, so
    // a node at world x is drawn x points in from the canvas's left edge.
    let body = harness.state().space.body;
    did(&mut harness, &format!("space move {node} --x {} --y 20", body.width() - 180.0));
    harness.run();
    let cut =
        harness.state().browser_placements().first().copied().expect("the page is still drawn");
    assert!(
        (cut.area.width() - width).abs() < 0.5,
        "the page lays itself out at the node's whole width, not at what is left: {} against {width}",
        cut.area.width()
    );
    assert!(
        cut.visible.width() < cut.area.width() - 1.0,
        "and what may be painted is cut by the pane: {} against {}",
        cut.visible.width(),
        cut.area.width()
    );
    assert!(cut.visible.right() <= body.right() + 0.5, "cut to the pane");

    // Cut to a strip, and it is still a strip of the same page rather than a page put away - which is
    // what cropping means, and what `task-1908`'s report about a page vanishing near the edge asks for.
    did(&mut harness, &format!("space move {node} --x {} --y 20", body.width() - 40.0));
    harness.run();
    let strip =
        harness.state().browser_placements().first().copied().expect("a strip is still a page");
    assert!((strip.area.width() - width).abs() < 0.5, "still the whole page");
    assert!(strip.visible.width() < 100.0, "and a strip of it showing");

    // Right off the canvas there is nothing on the screen to place.
    did(&mut harness, &format!("space move {node} --x {} --y 20", body.width() + 200.0));
    harness.run();
    assert!(
        harness.state().browser_placements().is_empty(),
        "a node with nothing of it on the canvas places no page"
    );

    did(&mut harness, &format!("space move {node} --x 20 --y 20"));
    harness.run();
    let back = harness.state().browser_placements().first().copied().expect("the page comes back");
    assert_eq!(back.visible, back.area, "and it is whole again");
}

/// A page followed to a new address is what the node comes back on.
///
/// `task-1907`, against the released build: *"i clicked and navigated to a url from hacker news, but when it
/// reopened it was back at hacker news."* A click inside a page navigates the view and
/// `BrowserTab::arrived_at` records that on the **tab** — but `Browser::url` is what `store::write` puts in
/// `space.conf`, and nothing bridged the two. So the node was written down at the address it was *sent* to and
/// came back there, while the toolbar and `space browser url` both showed the right page for as long as the
/// window was open, which is why it read as a save fault rather than a navigation one.
///
/// `arrived_at` is what a click looks like from here: nothing asked for the page, so there is no `awaiting`.
#[test]
fn a_page_followed_to_a_new_address_is_what_the_node_comes_back_on() {
    use unluminous_app::services::space::{Kind, State};
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Browser, egui::pos2(60.0, 60.0));
    harness
        .state_mut()
        .new_detached_space_page(node, "https://news.ycombinator.com/")
        .expect("a tab with no view behind it");
    // **The view has to be alive before anything derived from it is written down**, which is `task-1906` §4.5's
    // guard: before the nodes are running the live state is empty, and writing it would put nothing over the
    // file. A real window does this when it opens a project.
    harness.state_mut().bring_the_current_view_to_life();
    harness.run();

    let recorded = |harness: &Harness<'static, UnluminousApp>| match harness
        .state()
        .space
        .space
        .current()
        .node(node)
        .map(|found| &found.state)
    {
        Some(State::Browser(browser)) => browser.url.clone(),
        _ => panic!("a browser node"),
    };
    assert_eq!(recorded(&harness), "https://news.ycombinator.com/");

    // A click inside the page, which is an arrival nothing asked for.
    let tab = harness.state().space.live.browser(node).expect("its tab").id;
    harness.state_mut().arrived_at_for_tests(tab, "https://example.com/an-article".to_owned());
    harness.run();

    assert_eq!(
        recorded(&harness),
        "https://example.com/an-article",
        "the node records where the page really went, which is what a reopen sends it to"
    );
}

/// A terminal node comes back showing what was on it.
///
/// `task-1908`: *"I have 2 terminals, one with claude code, and one with `ls` command executed. When I quit and
/// reopen, I want both exactly restored so I see claude code and the contents of `ls`."* The process cannot come
/// back — `tasks/task-1908-restoring-what-was-open-tdd.md` §1.1 has tmux's and iTerm2's own documentation on why
/// — and the screen can. This is the screen.
///
/// **On a folder of its own**, which is `task-1906` §4.8's rule: a test that calls `restore_project` writes into
/// the project it is given, and `sample_folder` is shared by every test that wants one.
#[test]
fn a_terminal_node_comes_back_showing_what_was_on_it() {
    use unluminous_app::services::space::Kind;
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-screen-replay");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    harness.run();

    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    harness
        .state_mut()
        .feed_a_space_terminal(node, b"$ ls\r\ntotal 48\r\nsrc  tests  Cargo.toml\r\n$ ");
    harness.run();

    // What the window would write on its way out.
    harness.state_mut().write_the_screens_down();
    let saved = unluminous_app::services::space::store::screen_path(&folder, node);
    assert!(saved.is_file(), "the screen was written down at {}", saved.display());

    // And what the node is started with when it opens again. `task-1912`: what is replayed is not written into
    // the terminal from outside — the console host erases that on Windows — but printed by a program inside the
    // node's own console, so what a test can hold is the command line that program is given and the bytes it
    // will print.
    let printing = unluminous_app::services::space::store::a_screen_to_print(&folder, node)
        .expect("there is a screen to print");
    assert_eq!(printing, saved, "and it is the file that was written down");
    let restore = unluminous_cli::restore::Restore {
        file: printing.clone(),
        shim: std::path::PathBuf::from("/apps/unluminous-cli"),
    };
    let (program, args) = unluminous_cli::restore::command_line(&restore, "zsh", &[]);
    assert_eq!(
        program, restore.shim,
        "the node starts the program that prints and then becomes the shell"
    );
    assert_eq!(
        args.last().map(String::as_str),
        Some("zsh"),
        "and the shell is the last word of it"
    );

    // What that program will print, read by a terminal, is the screen the node was left showing.
    let bytes = std::fs::read(&printing).expect("the screen comes back");
    let mut fresh = unluminous_terminal::Session::detached(unluminous_terminal::Size::new(12, 40));
    fresh.feed(&bytes);
    let screen = fresh.snapshot();
    assert!(screen.contains("total 48"), "the `ls` output came back: {:?}", screen.text());
    assert!(screen.contains("Cargo.toml"), "all of it, not just the first line");

    std::fs::remove_dir_all(&folder).ok();
}

/// A node starting with nothing to restore takes away whatever was lying there.
///
/// `task-1908`'s rule, kept now that the reading has moved out of this process: a screen is printed by the
/// shim and deleted by it, and this is the other way a file stops existing. Without both, a canvas that failed
/// to come back would replay a week-old screen for ever.
#[test]
fn a_screen_nobody_printed_is_not_kept_for_ever() {
    use unluminous_app::services::space::Kind;
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-screen-forgotten");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    harness.run();

    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 40.0));
    harness.state_mut().feed_a_space_terminal(node, b"$ ls\r\ntotal 48\r\n");
    harness.run();
    harness.state_mut().write_the_screens_down();

    let saved = unluminous_app::services::space::store::screen_path(&folder, node);
    assert!(saved.is_file(), "a screen was written down");
    unluminous_app::services::space::store::forget_a_screen(&folder, node);
    assert!(!saved.exists(), "and a node starting without it takes it away");
    assert!(
        unluminous_app::services::space::store::a_screen_to_print(&folder, node).is_none(),
        "so there is nothing to print"
    );

    std::fs::remove_dir_all(&folder).ok();
}

// ---------------------------------------------------------------------------------------------
// `task-1914`, the QA pass. Four of its seven reports are "I cannot type in X", and each one is a
// question about **who holds the keyboard** — which until this ticket was the one thing about this
// window nothing could be asked. `tasks/task-1914-nodes-that-can-be-typed-into-tdd.md` is the design.
// ---------------------------------------------------------------------------------------------

/// Where a node's body really is on the screen, which is what a click has to be aimed at.
fn where_a_node_is(harness: &Harness<'static, UnluminousApp>, node: u64) -> egui::Rect {
    let space = &harness.state().space;
    let camera = space.space.current().camera;
    let found = space.space.current().node(node).expect("the node is on the canvas").clone();
    let parts = unluminous_app::components::space::parts_of(&found);
    camera.rect_to_screen(space.body.min, parts.body)
}

/// A rectangle the accessibility tree reported for a control **inside a node**, as screen points.
///
/// A node is drawn into a layer of its own carrying the camera, and `egui` hands out a widget's
/// rectangle in that layer's own coordinates — so a control at `[[50 581] …]` in a node at world 20,20
/// is nowhere near the point 50,581 of the window. `input` positions are the window's own points, which
/// is what `window screenshot` writes out, so the two have to be put back together here.
fn on_the_screen(harness: &Harness<'static, UnluminousApp>, rect: egui::Rect) -> egui::Rect {
    let space = &harness.state().space;
    space.space.current().camera.rect_to_screen(space.body.min, rect)
}

/// Who holds the keyboard, as `unluminous-cli status --section keyboard` answers it.
fn who_holds_the_keyboard(harness: &mut Harness<'static, UnluminousApp>) -> serde_json::Value {
    did(harness, "status --section keyboard")["keyboard"].clone()
}

/// A terminal node clicked in takes the keyboard, and the letters reach its own session.
///
/// `task-1914`: *"In base of infinite space, i cant type in a terminal."* Nothing was wrong with the
/// terminal — see `a_project_that_comes_back_with_a_canvas_gives_it_the_keyboard` for what really was —
/// but until this there was no test that a key press reached a node's program at all, because a detached
/// session dropped whatever was sent to it. `Session::sent_to_a_detached_session` is that gap closed.
#[test]
fn a_terminal_node_takes_the_keyboard_when_it_is_clicked() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = harness.state_mut().new_detached_space_node(Kind::Terminal, egui::pos2(40.0, 30.0));
    // Nothing chosen, which is the state a canvas that has just been read off disk is in.
    harness.state_mut().space.space.choose(None);
    harness.run();

    let body = where_a_node_is(&harness, node);
    let (x, y) = (body.center().x, body.center().y);
    drove(&mut harness, &format!("input click {x} {y}"));

    let keyboard = who_holds_the_keyboard(&mut harness);
    assert_eq!(keyboard["holder"], "space", "a click in a node hands the keyboard to the canvas");
    assert_eq!(keyboard["node"], serde_json::json!(node), "and chooses that node");
    assert_eq!(keyboard["textBox"], serde_json::json!(false), "no field is holding it");

    drove(&mut harness, "input text ls");
    let sent = harness
        .state()
        .space
        .live
        .terminal(node)
        .expect("the node has a session")
        .sent_to_a_detached_session();
    assert_eq!(sent, "ls", "the letters were encoded and sent to the node's own program");
}

/// A project whose canvas is showing and whose editing area is not gives the canvas the keyboard.
///
/// **This is what "i cant type in a terminal" really was.** A window starts on `Focus::Editor`, and a
/// project restored with the canvas filling the window has no editing area for a key press to reach — so
/// every letter went to a pane nobody could see. The rule is the narrow one: where **both** are showing
/// the editing area keeps the keyboard, which is what a text editor should do.
///
/// It is also the second half of *"When I reopen after closing, my cursor focus is stuck in the web
/// browser url"*: nothing holds the focus after a reopen, which `textBox` says here. The address bar only
/// looked like the place the keys went because it was the one caret drawn on the canvas.
#[test]
fn a_project_that_comes_back_with_a_canvas_gives_it_the_keyboard() {
    use unluminous_app::app::Focus;
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-canvas-keyboard");
    // A project left with the canvas showing, the editing area put away, and a node chosen.
    {
        let mut harness = harness_in(&folder);
        harness.state_mut().restore_project();
        did(&mut harness, "space show");
        let node =
            did(&mut harness, "space add terminal --x 40 --y 30")["node"].as_u64().expect("id");
        harness.state_mut().editor_visible = false;
        harness.state_mut().space.space.choose(Some(node));
        harness.run();
    }

    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    harness.run();
    assert!(harness.state().space.visible, "the canvas came back");
    assert!(!harness.state().editor_visible, "and the editing area is still away");
    assert_eq!(harness.state().focus, Focus::Space, "so the canvas holds the keyboard");
    assert!(
        harness.state().space.space.chosen().is_some(),
        "and the node it was left on is chosen, so the first key press has somewhere to go",
    );
    let keyboard = who_holds_the_keyboard(&mut harness);
    assert_eq!(keyboard["textBox"], serde_json::json!(false), "nothing is holding a text box");

    std::fs::remove_dir_all(&folder).ok();
}

/// And where both are showing, the editing area keeps the keyboard.
///
/// The other half of the rule above, and it is a test because it is the half every other test rests on:
/// widening it to "the canvas takes the keyboard whenever it is showing" would take the keys out of the
/// editor in every project that has a canvas open beside it.
#[test]
fn a_project_showing_both_leaves_the_keyboard_in_the_editing_area() {
    use unluminous_app::app::Focus;
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-canvas-and-editor");
    {
        let mut harness = harness_in(&folder);
        harness.state_mut().restore_project();
        did(&mut harness, "space show");
        harness.run();
    }

    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    harness.run();
    assert!(harness.state().space.visible, "the canvas came back");
    assert!(harness.state().editor_visible, "and so did the editing area");
    assert_eq!(harness.state().focus, Focus::Editor, "which keeps the keyboard");

    std::fs::remove_dir_all(&folder).ok();
}

/// Which node was chosen is written down, and the restore does not choose a different one.
///
/// Opening a File Editor node's tabs chooses that node, because opening a file into a node is using it —
/// so a canvas with one came back with the keyboard there whatever it was left on, and a canvas with none
/// came back with the keyboard nowhere at all. The saved choice wins.
#[test]
fn which_node_was_chosen_comes_back_with_the_canvas() {
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-canvas-chosen");
    let terminal = {
        let mut harness = harness_in(&folder);
        harness.state_mut().restore_project();
        did(&mut harness, "space show");
        let terminal =
            did(&mut harness, "space add terminal --x 40 --y 30")["node"].as_u64().expect("id");
        let editor =
            did(&mut harness, "space add editor --x 700 --y 30")["node"].as_u64().expect("id");
        did(&mut harness, &format!("space editor {editor} readme.md"));
        // Left on the terminal, which is not the node the restore will open a file into.
        did(&mut harness, &format!("space focus {terminal}"));
        harness.run();
        terminal
    };

    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    harness.run();
    assert_eq!(
        harness.state().space.space.chosen(),
        Some(terminal),
        "the canvas came back on the node it was left on, not on the one whose file was opened",
    );

    std::fs::remove_dir_all(&folder).ok();
}

/// Enter in a chat node's composer sends, and Shift+Enter puts a new line in the draft.
///
/// `task-1914`: *"after i type an agent message in agent chat node, then stop, I can't type any more into
/// it."* Typing works; **Enter** did not, and a composer that keeps swallowing Enter is one somebody reads
/// as having stopped taking text.
///
/// The fault was in the input mechanism rather than in the composer, which is the kind that makes a test
/// lie. `composer::show` asks `input.modifiers.is_none()`, and `InputState::modifiers` is built from
/// `Event::ModifiersChanged` and from nothing else — so every event `services::input` queued carried its
/// modifiers on the event and left the frame's own state alone, and anything asking the frame was asking
/// about a keyboard nobody was touching. So this is driven through `input` rather than through
/// `Harness::key_press`, which sets the real modifiers and could never have found it.
///
/// The node's provider is pointed at a program nobody has, so the send is refused before anything is
/// started and **nothing here runs an agent or reaches a network** — which is
/// `enter_in_the_composer_sends_and_shift_enter_does_not`'s own arrangement for the pane.
#[test]
fn a_chat_node_sends_on_enter_rather_than_putting_a_new_line_in_the_draft() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add chat --x 10 --y 10")["node"].as_u64().expect("id");
    // Small enough to be **inside the pane**, because a node's contents are clipped to it: a composer
    // hanging below the canvas is drawn nowhere and can be clicked nowhere.
    did(&mut harness, &format!("space size {node} --width 520 --height 430"));
    harness.run();
    if let Some(chat) = harness.state_mut().space.live.chat_mut(node) {
        let _ = chat.configuration_mut().choose("claude");
        chat.configuration_mut().providers[0].command = NO_SUCH_AGENT.to_owned();
    }
    harness.run();

    // A press in the composer, which is a `TextEdit` inside a transformed sublayer.
    let composer = on_the_screen(&harness, harness.get_by_label("Message").rect());
    drove(&mut harness, &format!("input click {} {}", composer.center().x, composer.center().y));
    let keyboard = who_holds_the_keyboard(&mut harness);
    assert_eq!(keyboard["textBox"], serde_json::json!(true), "the composer took the keyboard");

    drove(&mut harness, "input text hello");
    let draft = |harness: &Harness<'static, UnluminousApp>| {
        harness.state().space.live.chat(node).expect("the node has a chat").draft.clone()
    };
    assert_eq!(draft(&harness), "hello", "the letters reached this node's own draft");

    // Shift+Enter is a new line: nothing is sent and nothing is refused.
    drove(&mut harness, "input key Enter --shift");
    let after = did(&mut harness, &format!("space chat {node} state"));
    assert!(after["problem"].is_null(), "shift+enter did not try to send: {after}");
    assert_eq!(
        draft(&harness),
        "hello
",
        "it put a new line in the draft"
    );

    // Enter sends, which here is refused before anything is started — and the refusal names the program
    // it would have run, which is the only way this test can tell "it sent" from "it did nothing".
    drove(&mut harness, "input key Enter");
    let after = did(&mut harness, &format!("space chat {node} state"));
    let problem = after["problem"].as_str().expect("Enter asked for a send, and it was refused");
    assert!(problem.contains(NO_SUCH_AGENT), "the refusal names the program: {problem}");
}

/// The search box of an Agent Tasks node takes the keyboard and what is typed into it.
///
/// `task-1914`: *"I cant type in the search of agent tasks node."* A field holding egui's focus makes
/// **every** other surface stand aside, deliberately, which is why one field that will not take a press
/// reads as the whole window having stopped answering — so what this asserts is both halves: the box has
/// the keyboard, and the letters are in the board's own query rather than anywhere else.
#[test]
fn the_tasks_node_search_takes_what_is_typed_into_it() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add tasks --x 10 --y 10")["node"].as_u64().expect("id");
    // Inside the pane, because a node's contents are clipped to it. See `on_the_screen`.
    did(&mut harness, &format!("space size {node} --width 860 --height 440"));
    harness.run();

    // Found by its name rather than by arithmetic over the board's own measurements, which is what
    // every screenshot test here does — the node draws the window's one board, so the box is the same
    // control the pane's is and answers to the same name.
    let search = on_the_screen(&harness, harness.get_by_label("Search tasks").rect());
    drove(&mut harness, &format!("input click {} {}", search.center().x, search.center().y));
    let keyboard = who_holds_the_keyboard(&mut harness);
    assert_eq!(keyboard["textBox"], serde_json::json!(true), "the search box took the keyboard");

    drove(&mut harness, "input text batt");
    let provider =
        harness.state_mut().plugin_ui.provider("agent-tasks").expect("the board is open");
    let board = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
        .expect("the Agent-Tasks provider");
    assert_eq!(board.query(), "batt", "what was typed is the board's own query");
}

/// The title bar keeps its drag area while a pane is maximised.
///
/// `task-1914`: *"If base of infinite space is maximized, i can't move the main window around, only
/// resize it."* `ViewportCommand::StartDrag` hands the drag to the operating system's own modal loop, so
/// no synthetic event can drive it and no test can assert that a window moved. What a test can assert is
/// that the **control is there and is the size it should be** — and until this ticket it could not even
/// do that, because the drag area had no `widget_info` and nothing could find it. Every control in
/// Unluminous has a plain name; this one is `Move window`.
#[test]
fn the_title_bar_can_still_be_dragged_while_a_pane_is_maximised() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    harness.run();
    let before = harness.get_by_label("Move window").rect();
    assert!(before.width() > 100.0, "the bar has a drag area to begin with: {before:?}");

    did(&mut harness, "action run toggle-maximised-pane");
    harness.run();
    let after = harness.get_by_label("Move window").rect();
    assert!(
        after.width() >= before.width() - 1.0,
        "maximising a pane did not take the title bar's drag away: {before:?} -> {after:?}",
    );
    assert_eq!(after.top(), before.top(), "and it is still along the top of the window");
}

/// The pixels of one band of the window, so two frames can be compared without a baseline image.
///
/// **A comparison rather than an accepted picture**, because what these ask is not *what does this look
/// like* but *did anything get drawn where nothing should be* — and that has an answer on any machine,
/// in any font, without a snapshot for each platform.
fn pixels_in(harness: &mut Harness<'static, UnluminousApp>, band: egui::Rect) -> Vec<u8> {
    let scale = harness.ctx.pixels_per_point();
    let image = harness.render().expect("the window renders");
    let (left, top) = ((band.left() * scale) as u32, (band.top() * scale) as u32);
    let (width, height) = ((band.width() * scale) as u32, (band.height() * scale) as u32);
    assert!(width > 0 && height > 0, "an empty band says nothing: {band:?}");
    image::imageops::crop_imm(&image, left, top, width, height).to_image().into_raw()
}

/// A node scrolled off the edge of the canvas draws nothing outside the pane.
///
/// `task-1914`'s sweep found an Agent Tasks node's card drawn **over the editing area** when the node was
/// scrolled off the edge of the canvas. `Ui::set_clip_rect` is an assignment, so a lane writing its own
/// rectangle over the one it was given threw the pane's edge away — which is the rule
/// `components::explorer` and `components::markdown_text` each already record, broken again a lane at a
/// time.
///
/// What is compared is a band of the window above the canvas, with the node inside the pane and then
/// hanging off the top of it. Nothing about that band may change.
#[test]
fn a_node_scrolled_off_the_canvas_draws_nothing_outside_the_pane() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add tasks --x 10 --y 10")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space size {node} --width 860 --height 440"));
    harness.run();

    // The editing area above the canvas, which nothing on the canvas may reach.
    let body = harness.state().space.body;
    let above = egui::Rect::from_min_max(
        egui::pos2(body.left() + 20.0, body.top() - 120.0),
        egui::pos2(body.right() - 20.0, body.top() - 8.0),
    );
    let quiet = pixels_in(&mut harness, above);

    // The same canvas with the node dragged most of the way off the top edge.
    did(&mut harness, "space camera --y 300");
    harness.run();
    let now = pixels_in(&mut harness, above);
    assert_eq!(quiet.len(), now.len(), "the same band both times");
    assert!(quiet == now, "a node hanging off the canvas drew something in the window above it");
}

/// A Folder node draws no file count, so nothing is cut in half by its own bottom edge.
///
/// The strip that counts a project's files belongs to the **panel** — `footer_top` has said so since the
/// node was built — but the count was drawn anyway, centred on a rectangle of no height sitting on the
/// node's bottom edge. Half of it was inside the node and half below it, and at
/// `appearance.ui.font.size = 24` the halves are plain to see. `task-1914`'s sweep found it.
#[test]
fn a_folder_node_draws_no_file_count_over_its_own_edge() {
    let mut harness = harness("");
    did(&mut harness, "space show");
    harness.run();
    assert_eq!(
        harness.query_all_by_label("File count").count(),
        1,
        "the explorer panel counts the project's files",
    );

    did(&mut harness, "space add folder --x 10 --y 10");
    harness.run();
    assert_eq!(
        harness.query_all_by_label("File count").count(),
        1,
        "a folder node added no second count, so there is none to be halved by the node's own edge",
    );
}

/// A browser node's address is set in a strip its own size, inside the field, at any interface size.
///
/// `task-1914`: *"the web browser node url text is not vertically centered in the bar, so its up too high
/// and partially clipped."* The arithmetic is
/// `controls::tests::a_fields_text_row_is_measured_at_the_size_the_text_will_be_set_in`; this is the
/// address bar really asking for it, at the interface size the report came from.
#[test]
fn an_address_bar_draws_its_text_inside_its_own_box() {
    let mut harness = harness("");
    did(&mut harness, "settings set appearance.ui.font.size 24");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add browser --x 10 --y 10")["node"].as_u64().expect("id");
    did(&mut harness, &format!("space size {node} --width 620 --height 300"));
    harness.run();

    let address = harness.get_by_label("Address").rect();
    assert!(
        address.height() <= 20.0,
        "the box is a row of the size the address is set in, not of the interface's: {address:?}",
    );
    // And inside the node, which is the half the report could see: the strip used to reach past the
    // field's own border and the top of the words was cut off by it.
    let found = harness.state().space.space.current().node(node).expect("it is there").clone();
    let parts = unluminous_app::components::space::parts_of(&found);
    assert!(
        parts.body.contains_rect(address),
        "the address box is inside the node: {address:?} in {:?}",
        parts.body,
    );
}

/// A chat node at its smallest keeps the composer's words inside the node.
///
/// The composer is measured for one line and grows to hold what is in it, so a hint too wide for a narrow
/// field wrapped to three lines and the last of them was drawn below the node's own bottom edge — which is
/// `composer::hint`'s reason for existing. `Kind::Chat::smallest` is the other half: under it the chat is a
/// header and a composer with no room between them.
#[test]
fn a_chat_node_at_its_smallest_keeps_the_composer_inside_it() {
    use unluminous_app::services::space::Kind;
    let mut harness = harness("");
    did(&mut harness, "settings set appearance.ui.font.size 24");
    did(&mut harness, "space show");
    let node = did(&mut harness, "space add chat --x 10 --y 10")["node"].as_u64().expect("id");
    let smallest = Kind::Chat.smallest();
    did(&mut harness, &format!("space size {node} --width {} --height {}", smallest.x, smallest.y));
    harness.run();

    let found = harness.state().space.space.current().node(node).expect("it is there").clone();
    assert_eq!(found.size, smallest, "a node cannot be dragged below its kind's own floor");
    let parts = unluminous_app::components::space::parts_of(&found);
    let composer = harness.get_by_label("Message").rect();
    assert!(
        parts.body.contains_rect(composer),
        "the composer is inside the node: {composer:?} in {:?}",
        parts.body,
    );
}
