//! The Database plugin: the tree, the grid, the console, the pending edits and the vectors.
//!
//! Every one of these drives a **real SQLite file** or a **real Inillucent file** built in a
//! temporary folder. A PostgreSQL server cannot be assumed on the machine running a test —
//! `crates/unluminous-db/tests/scripted_server.rs` is where the wire protocol is tested against a
//! server made of fixed bytes, and `cargo run -p unluminous-db --example connect` is how the real one
//! is. This is the layer above both: the window, driven the way a person drives it, with the answers
//! coming from a real engine rather than from a stub.
//!
//! **9 of the 24 tests here take a picture**, and the rest read the window's state back.

mod common;

use common::*;

use egui::Modifiers;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::UnluminousApp;
use unluminous_core::Command;

// ---------------------------------------------------------------------------------- the Database plugin
//
// Every one of these drives a **real SQLite file** built in a temporary folder. A PostgreSQL server
// cannot be assumed on the machine running a test — `crates/unluminous-db/tests/scripted_server.rs` is
// where the wire protocol is tested against a server made of fixed bytes, and
// `cargo run -p unluminous-db --example connect` is how the real one is. This is the layer above both:
// the window, driven the way a person drives it, with the answers coming from a real engine rather
// than from a stub.

/// A database file with something in it, in a folder named after the test that asked for it.
///
/// A fixture only one test uses may be written each time and the name is what keeps them apart, which
/// is `git_folder(name)`'s own rule.
///
/// **Not [`fixture`], because what it writes is a database**, made by running real statements
/// through the engine rather than by writing bytes.
fn a_database_file(name: &str) -> std::path::PathBuf {
    // **No process id in the name**, unlike the plugin's own tests: a data source draws where it
    // points, so a folder that changed between runs would put a different string in an accepted
    // image every time. The test's own name is what keeps two of these apart, which is the rule
    // `git_folder(name)` already follows.
    let folder = std::env::temp_dir().join(format!("unluminous-database-shot-{name}"));
    let _ = std::fs::create_dir_all(&folder);
    let file = folder.join("library.db");
    let _ = std::fs::remove_file(&file);
    let connection = rusqlite::Connection::open(&file).expect("a database");
    connection
        .execute_batch(
            "create table album (id integer primary key, title text not null, year integer, note text);
             insert into album (title, year, note) values
               ('Kind of Blue', 1959, null),
               ('A Love Supreme', 1965, ''),
               ('Bitches Brew', 1970, 'double'),
               ('Agharta', 1975, 'live');
             create table tag (album_id integer, tag text);
             insert into tag values (1, 'jazz'), (2, 'jazz'), (3, 'fusion');
             create view album_tags as select a.title, t.tag from album a join tag t on t.album_id = a.id;",
        )
        .expect("a schema");
    file
}

/// A window with the Database plugin pointed at one, writable, with the tree showing.
fn a_database(name: &str) -> Harness<'static, UnluminousApp> {
    let file = a_database_file(name);
    let mut harness = harness("");
    did(&mut harness, &format!("plugins run database add-source library {}", file.display()));
    did(&mut harness, "plugins pane database/explorer --show");
    harness.run();
    harness
}

/// Step until the plugin has nothing outstanding, or give up.
///
/// `Harness::run` gives the window four steps to go quiet and panics otherwise, which is right for a
/// settled window and wrong while a worker thread is still answering — the rule `task-1654` wrote
/// down for the loops that wait on git, wearing a different hat.
fn until_the_database_settles(harness: &mut Harness<'static, UnluminousApp>) {
    for _ in 0..400 {
        harness.step();
        let view = did_while_waiting(harness, "plugins view database");
        let busy = view["sources"]
            .as_array()
            .map(|sources| sources.iter().any(|source| source["busy"] == serde_json::json!(true)))
            .unwrap_or(false);
        if !busy {
            harness.step();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the database plugin is still busy after four hundred frames");
}

/// Press a field anywhere inside it, and the box inside it takes the keyboard.
///
/// `controls::field_text_rect` lays a `TextEdit` out as a strip one line tall, centred in the
/// control — 15 points inside a 24 point field, inset eight points from the left — and that strip
/// used to be the only part of the field a pointer could hit. `task-1795`.
#[test]
fn a_press_anywhere_in_a_field_hands_it_the_keyboard() {
    let mut harness = a_database("field-focus");
    did(&mut harness, "plugins pane database/explorer --show");
    harness.run();
    let box_rect = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Filter database objects")
        .rect();
    assert!(box_rect.height() < 20.0, "the box really is a strip: {box_rect:?}");
    // Six points left of where the box begins, which is still inside the drawn field.
    press_at(&mut harness, egui::Pos2::new(box_rect.left() - 6.0, box_rect.center().y));
    assert!(
        harness.ctx.text_edit_focused(),
        "a press in the field's own padding should hand the box the keyboard"
    );
}

/// A paste into one of a plugin's fields must never reach the file behind the pane.
///
/// With no text box holding the keyboard, `Focus::Editor` still stands and
/// `editor_view::handle_input` reads the frame's `Event::Paste` — so `Ctrl+V` aimed at the Database
/// pane's filter went into the open document instead. Reproduced before the fix as
/// `document = "helloPASTED"`.
#[test]
fn a_paste_into_a_plugins_field_never_reaches_the_document() {
    let mut harness = a_database("field-paste");
    did(&mut harness, "plugins pane database/explorer --show");
    harness.state_mut().document_mut().apply(Command::Insert("hello".to_owned()));
    harness.run();
    let box_rect = harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, "Filter database objects")
        .rect();
    press_at(&mut harness, egui::Pos2::new(box_rect.left() - 6.0, box_rect.center().y));
    harness.input_mut().events.push(egui::Event::Paste("PASTED".to_owned()));
    harness.run();
    assert_eq!(
        harness.state().document().text().to_string(),
        "hello",
        "the paste belonged to the filter, not to the file behind the pane"
    );
}

/// The same, for the field the ticket actually reported: the SQLite path on the New Data Source
/// dialog.
#[test]
fn a_path_can_be_pasted_into_the_sqlite_file_field() {
    let mut harness = a_database("field-paste-file");
    did(&mut harness, "plugins pane database/explorer --show");
    harness.run();
    harness.get_by_label("New data source").click();
    harness.run();
    harness.get_by_label("SQLite").click();
    harness.run();
    let box_rect = harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "File").rect();
    press_at(&mut harness, egui::Pos2::new(box_rect.left() - 4.0, box_rect.center().y));
    harness.input_mut().events.push(egui::Event::Paste("C:/tmp/pasted.db".to_owned()));
    harness.run();
    assert_eq!(
        harness.get_by_role_and_label(egui::accesskit::Role::TextInput, "File").value(),
        Some("C:/tmp/pasted.db".to_owned())
    );
}

/// Press and release the primary button at a point, and let the window settle.
///
/// `Node::click` presses the middle of a widget, and these tests are about the points that are
/// **not** any widget's middle.
fn press_at(harness: &mut Harness<'static, UnluminousApp>, at: egui::Pos2) {
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.step();
    harness.input_mut().events.push(egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.run();
}

/// The ticket's shape, as data: a tree in a pane, a workspace in a tab, a menu and a Settings page.
#[test]
fn the_database_plugin_contributes_a_pane_a_menu_and_a_page() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = harness("");
    let listed = did(&mut harness, "plugins list");
    let plugins = listed["plugins"].as_array().expect("the plugins");
    let database =
        plugins.iter().find(|plugin| plugin["id"] == "database").expect("the database plugin");
    assert_eq!(database["kind"], "ui");
    assert_eq!(database["provider"], "database");
    let contributes: Vec<&str> = database["contributes"]
        .as_array()
        .expect("what it adds")
        .iter()
        .filter_map(|it| it.as_str())
        .collect();
    // `task-1848` folded the workspace into the pane, so there is one surface rather than two.
    assert_eq!(contributes, ["pane", "menu", "settings page"]);

    // The rail has a button for the pane, and the pane docks where the manifest says.
    let slot = harness
        .state()
        .plugin_ui
        .slot_of("database/explorer")
        .expect("the tree's pane is in a slot");
    // `Database pane`, because the plugin's menu is called `Database` and no two controls in one window
    // may share a name.
    assert!(
        harness.get_all_by_label("Database pane").count() > 0,
        "the rail draws a button for it"
    );
    let shown = did(&mut harness, "plugins pane database/explorer --show");
    assert_eq!(shown["showing"], true);
    assert_eq!(shown["side"], "right", "where the reference editor docks its Database tool window");
    let moved = did(&mut harness, "plugins pane database/explorer --side left");
    assert_eq!(moved["side"], "left");
    assert_eq!(side_of(&harness, Panel::Plugin(slot as u8)), Side::Left);
    did(&mut harness, "plugins pane database/explorer --side right");

    // And the workspace is inside that pane rather than a tab of its own, which is `task-1848`:
    // "Database query view should be part of the database pane." Showing the pane is what puts the
    // consoles and the grids on the screen, and there is no second surface to open or close.
    let opened = did(&mut harness, "plugins pane database/explorer --show");
    assert_eq!(opened["showing"], true);

    // The SQL language plugin ships beside it, so a `.sql` file is coloured whether or not anybody
    // opens the pane.
    let sql = plugins.iter().find(|plugin| plugin["id"] == "sql").expect("the sql plugin");
    assert_eq!(sql["kind"], "language");
    assert!(sql["extensions"]
        .as_array()
        .expect("its extensions")
        .contains(&serde_json::json!("sql")));
}

/// The tree reads one level at a time, which is how a database with four thousand tables stays usable.
#[test]
fn the_database_tree_reads_a_source_one_level_at_a_time() {
    let mut harness = a_database("tree");
    // Nothing is connected until something asks, which is the laziness the plugin contract describes.
    let before = did(&mut harness, "plugins view database");
    assert_eq!(before["sources"][0]["connected"], false);

    // Opened the way a person opens it — by pressing the rows — one level per press.
    harness.get_by_label("library").click();
    until_the_database_settles(&mut harness);
    let connected = did(&mut harness, "plugins view database");
    assert_eq!(
        connected["sources"][0]["connected"], true,
        "pressing the row opened the connection"
    );
    assert_eq!(connected["tree"][0]["schemas"], serde_json::json!(["main"]));
    assert!(
        connected["tree"][0]["items"].as_array().is_some_and(Vec::is_empty),
        "nothing under a schema until the schema is opened: {connected}"
    );

    harness.get_by_label("main").click();
    until_the_database_settles(&mut harness);
    let opened = did(&mut harness, "plugins view database");
    let named: Vec<&str> = opened["tree"][0]["items"][0]["items"]
        .as_array()
        .expect("the items of the schema")
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(
        named.contains(&"album") && named.contains(&"tag") && named.contains(&"album_tags"),
        "{named:?}"
    );

    // The folders and then one table's columns, which is the fourth level and the last.
    harness.get_by_label("tables").click();
    until_the_database_settles(&mut harness);
    harness.get_by_label("album").click();
    until_the_database_settles(&mut harness);
    assert_eq!(did(&mut harness, "plugins view database")["chosen_row"]["name"], "album");
    harness.snapshot(shot("database_tree").as_str());
}

/// A table opens in the workspace with its rows, its row numbers and its key column marked.
#[test]
fn a_table_opens_in_the_workspace_with_its_rows() {
    let mut harness = a_database("grid");
    did(&mut harness, "plugins run database tables main");
    did(&mut harness, "plugins run database open main.album");
    until_the_database_settles(&mut harness);
    let page = did(&mut harness, "plugins run database result");
    assert_eq!(page["kind"], "grid");
    assert_eq!(page["table"], "album");
    assert_eq!(page["key"], serde_json::json!(["id"]));
    assert_eq!(page["editable"], true);
    assert_eq!(page["rows"]["count"], 4);
    // A NULL and an empty string are different values and the grid keeps them apart all the way from
    // the engine, which is the fault `unluminous_db::Value` exists to keep out of a grid.
    assert_eq!(page["rows"]["rows"][0][3], serde_json::Value::Null);
    assert_eq!(page["rows"]["rows"][1][3], serde_json::json!(""));
    harness.snapshot(shot("database_grid").as_str());
}

/// A view's rows belong to the tables underneath it, so the grid is read only and says why.
#[test]
fn a_view_is_drawn_read_only_with_the_reason_where_the_buttons_were() {
    let mut harness = a_database("view");
    did(&mut harness, "plugins run database tables main");
    did(&mut harness, "plugins run database open main.album_tags");
    until_the_database_settles(&mut harness);
    let page = did(&mut harness, "plugins run database result");
    assert_eq!(page["editable"], false);
    let why = page["why_not_editable"].as_str().unwrap_or_default();
    assert!(why.contains("view"), "{why}");
    // And an agent asking to change one is refused with the same sentence, rather than quietly
    // recording a change that could never be written.
    // `failed` rather than `usage`: the command was written correctly and the *state* refuses it,
    // which is the distinction `unluminous-cli`'s two codes already draw.
    assert_eq!(refused(&mut harness, "plugins run database set 1 tag rock"), "failed");
}

/// A console runs a statement and the result comes back under it.
#[test]
fn a_console_runs_a_statement_and_shows_what_came_back() {
    let mut harness = a_database("console");
    did_while_waiting(&mut harness, "plugins run database console library");
    did_while_waiting(
        &mut harness,
        "plugins run database query select title, year from album order by year",
    );
    until_the_database_settles(&mut harness);
    let state = did(&mut harness, "plugins run database state");
    assert_eq!(state["running"], false);
    let result = did(&mut harness, "plugins run database result");
    assert_eq!(result["kind"], "console");
    assert_eq!(result["result"]["count"], 4);
    assert_eq!(result["result"]["rows"][0][0], "Kind of Blue");
    harness.snapshot(shot("database_console").as_str());
}

/// A statement that returns no rows fills `Output` rather than a grid, with its own count.
#[test]
fn a_statement_that_returns_no_rows_fills_the_output_tab() {
    let mut harness = a_database("output");
    did_while_waiting(&mut harness, "plugins run database console library");
    // No quoted literal in the statement: the command line takes the quotes off, which is right for
    // every other command and is why a value with spaces in it belongs in the grid rather than here.
    did_while_waiting(
        &mut harness,
        "plugins run database query update album set year = 2000 where id = 1",
    );
    until_the_database_settles(&mut harness);
    let result = did(&mut harness, "plugins run database result");
    assert!(result["result"].is_null(), "no grid for a statement that returned no rows");
    let output: Vec<&str> = result["output"]
        .as_array()
        .expect("output")
        .iter()
        .filter_map(|line| line.as_str())
        .collect();
    assert!(output.iter().any(|line| line.contains("1 rows")), "{output:?}");
}

/// A statement that will not run comes back as the engine's own words rather than as a summary.
#[test]
fn a_failing_statement_is_reported_in_the_engines_own_words() {
    let mut harness = a_database("failing");
    did_while_waiting(&mut harness, "plugins run database console library");
    did_while_waiting(&mut harness, "plugins run database query select * from nothing_like_this");
    until_the_database_settles(&mut harness);
    let result = did(&mut harness, "plugins run database result");
    let failure = result["failure"].as_str().unwrap_or_else(|| panic!("a failure: {result}"));
    assert!(failure.contains("nothing_like_this"), "{failure}");
    // And the connection is still usable, which is the point of reading a statement through to the
    // end: one bad statement in a console must not cost the session.
    did_while_waiting(&mut harness, "plugins run database query select 1");
    until_the_database_settles(&mut harness);
    assert_eq!(did(&mut harness, "plugins run database result")["result"]["count"], 1);
}

/// An edit is pending until it is submitted, and then the file on disk really changes.
#[test]
fn an_edit_is_pending_until_it_is_submitted_and_the_file_changes() {
    let file = a_database_file("submit");
    let mut harness = harness("");
    did(&mut harness, &format!("plugins run database add-source library {}", file.display()));
    did(&mut harness, "plugins run database read-only library off");
    did(&mut harness, "plugins pane database/explorer --show");
    did(&mut harness, "plugins run database tables main");
    did(&mut harness, "plugins run database open main.album");
    until_the_database_settles(&mut harness);

    did(&mut harness, "plugins run database set 1 title Kind of Green");
    let pending = did(&mut harness, "plugins run database pending");
    let statements = pending.as_array().expect("the statements");
    assert_eq!(statements.len(), 1);
    let sql = statements[0]["sql"].as_str().expect("the sql");
    assert!(sql.starts_with("UPDATE \"album\" SET \"title\" = ?1 WHERE \"id\" = ?2"), "{sql}");
    // The value is **bound**, not pasted into the statement, which is what makes an awkward value a
    // non-event rather than a fault.
    assert!(!sql.contains("Kind of Green"));
    harness.snapshot(shot("database_pending").as_str());

    did(&mut harness, "plugins run database submit");
    until_the_database_settles(&mut harness);
    // Read back through a connection of its own, so this is the file rather than a cache.
    let connection = rusqlite::Connection::open(&file).expect("opened");
    let title: String = connection
        .query_row("select title from album where id = 1", [], |row| row.get(0))
        .expect("a row");
    assert_eq!(title, "Kind of Green");
}

/// A data source is writable, and a read-only one refuses a write before anything is sent.
///
/// **A new data source can be written to.** `task-1795` asks for no safety check at all — *"We don't
/// want a safety check at all. Should be full access."* — so the tick box, the write confirmation
/// and the read-only default are all gone. What is left is the guarantee itself, which is the
/// *server's* rather than a parser in Unluminous: `SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY`
/// and `SQLITE_OPEN_READONLY`. Nothing in the window offers it and `read-only` is the one way in, so
/// an agent that wants a session which cannot write can still ask for one.
#[test]
fn a_read_only_data_source_refuses_a_write_before_anything_is_sent() {
    let file = a_database_file("guard");
    let mut harness = harness("");
    did(&mut harness, &format!("plugins run database add-source library {}", file.display()));
    assert_eq!(
        did(&mut harness, "plugins view database")["sources"][0]["read_only"],
        false,
        "a new data source is writable",
    );
    did(&mut harness, "plugins run database read-only library on");
    did_while_waiting(&mut harness, "plugins run database console library");
    assert_eq!(refused(&mut harness, "plugins run database query delete from album"), "failed");
    let view = did(&mut harness, "plugins view database");
    assert_eq!(view["sources"][0]["read_only"], true);
    // Nothing was sent, so the rows are all still there.
    let connection = rusqlite::Connection::open(&file).expect("opened");
    let count: i64 =
        connection.query_row("select count(*) from album", [], |row| row.get(0)).expect("a row");
    assert_eq!(count, 4);
}

/// The New Data Source dialog, which is the reference editor's General tab cut to what applies.
#[test]
fn the_new_data_source_dialog() {
    let mut harness = harness("");
    // The menu entry and the command are one path: `add-source` with nothing said opens the dialog,
    // and `add-source <name> <url>` adds one without it.
    did(&mut harness, "plugins run database add-source");
    harness.run();
    harness.snapshot(shot("database_source_dialog").as_str());
}

/// The Settings page: where each data source points, and where its password is — never what it is.
///
/// **A PostgreSQL source rather than the SQLite one every other test here uses**, and that is about
/// the picture rather than about the page: a SQLite data source draws the path of its file, and the
/// only paths available to a test are a temporary folder that differs between machines and between
/// runs — which is a screenshot that can never match twice. `postgres://…@localhost:5432/library`
/// says the same thing on every machine, and it is the case this page is really about anyway, since
/// it is the one that has a password to name a place for.
#[test]
fn the_database_settings_page_says_where_a_password_is_and_never_what_it_is() {
    let mut harness = harness("");
    did(
        &mut harness,
        "plugins run database add-source library postgres://postgres@localhost:5432/library UNLUMINOUS_DB_LIBRARY",
    );
    did(&mut harness, "action run settings");
    harness.run();
    harness.get_all_by_label("Database").last().expect("the Settings row").click();
    harness.run();
    // Nothing on this page is the password, and the page says where it is instead.
    let view = did(&mut harness, "plugins view database");
    assert_eq!(view["sources"][0]["password"], "environment UNLUMINOUS_DB_LIBRARY");
    harness.snapshot(shot("database_settings").as_str());
}

/// The grid the workspace is showing, as data, so a test can read what the pane has on it.
fn the_open_page(harness: &mut Harness<'static, UnluminousApp>) -> serde_json::Value {
    let view = did(harness, "plugins view database");
    let current = view["current"].clone();
    view["pages"]
        .as_array()
        .expect("the pages")
        .iter()
        .find(|page| page["id"] == current)
        .cloned()
        .unwrap_or_else(|| panic!("no page is open: {view}"))
}

/// `Ctrl/Cmd+Enter` in the console runs the statement, and `Enter` alone is still a new line.
///
/// The chord was written when the console was and it never once fired: it is guarded on the SQL box
/// having the keyboard, and a press anywhere but the middle fifteen points of the box did not give
/// it — which is `task-1795`'s root cause, the same one that sent a paste into the file behind the
/// pane. Both halves are asserted, because a console in which `Enter` executed would be a console
/// nobody could write two lines of SQL in.
#[test]
fn ctrl_enter_runs_the_statement_under_the_caret() {
    let mut harness = a_database("ctrl-enter");
    did_while_waiting(&mut harness, "plugins run database console library");
    until_the_database_settles(&mut harness);

    // Pressed in the box's own padding rather than its middle, which is the press that used to
    // leave the console without the keyboard.
    let box_rect = harness.get_by_label("SQL").rect();
    press_at(&mut harness, egui::Pos2::new(box_rect.left() + 2.0, box_rect.top() + 2.0));
    assert!(harness.ctx.text_edit_focused(), "the press handed the SQL box the keyboard");

    harness.get_by_label("SQL").type_text("select title from album order by id");
    harness.run();

    // Enter alone is a new line. Nothing runs.
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert!(
        the_open_page(&mut harness)["result"].is_null(),
        "Enter on its own must not execute — a console has to be able to hold two lines",
    );

    harness.key_press_modifiers(Modifiers::COMMAND, egui::Key::Enter);
    until_the_database_settles(&mut harness);
    let result = did(&mut harness, "plugins run database result");
    assert_eq!(result["kind"], "console");
    assert_eq!(result["result"]["count"], 4, "the chord ran the statement: {result}");
    assert_eq!(result["result"]["rows"][0][0], "Kind of Blue");
}

/// A double click opens a cell for typing, and Save writes what was typed to the file.
///
/// `task-1795`: *"I should be able to double click an entry in the table, type to update, and see a
/// save button that writes to the table for that value."* Each of the three is asserted separately,
/// because two of them half existed before: the model had an `editing` field that nothing ever set,
/// and the button was there under another name.
#[test]
fn double_clicking_a_cell_edits_it_and_save_writes_it() {
    let file = a_database_file("cell-edit");
    let mut harness = harness("");
    did(&mut harness, &format!("plugins run database add-source library {}", file.display()));
    did(&mut harness, "plugins pane database/explorer --show");
    did(&mut harness, "plugins run database tables main");
    did(&mut harness, "plugins run database open main.album");
    until_the_database_settles(&mut harness);

    // A single click chooses the cell and opens nothing — the two gestures must not fight, because
    // choosing is what `Delete row` and `Set NULL` act on.
    harness.get_by_label("title row 1").click();
    harness.run();
    assert!(
        the_open_page(&mut harness)["editing"].is_null(),
        "one click chooses a cell rather than opening it",
    );

    let at = harness.get_by_label("title row 1").rect().center();
    double_click_at(&mut harness, at);
    let editing = the_open_page(&mut harness)["editing"].clone();
    assert_eq!(editing["row"], 1, "the double click opened the cell: {editing}");
    assert_eq!(editing["column"], "title");
    assert_eq!(editing["text"], "Kind of Blue", "filled with what the cell shows");

    // Everything in the box is selected the frame it opens, so typing replaces rather than joins.
    harness.get_by_label("title row 1").type_text("Kind of Green");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();

    // Committing records a pending change rather than sending a statement, which is the arrangement
    // that lets Preview show what is about to happen.
    let pending = did(&mut harness, "plugins run database pending");
    assert_eq!(pending.as_array().expect("the statements").len(), 1, "{pending}");
    assert!(harness.get_all_by_label("Save 1").count() > 0, "the button says Save, and how many");

    harness.get_by_label("Save 1").click();
    until_the_database_settles(&mut harness);
    let connection = rusqlite::Connection::open(&file).expect("opened");
    let title: String = connection
        .query_row("select title from album where id = 1", [], |row| row.get(0))
        .expect("a row");
    assert_eq!(title, "Kind of Green", "Save wrote the typed value to the file");
}

/// `Escape` throws the typing away and leaves the row exactly as it was.
#[test]
fn escape_puts_a_cell_back_the_way_it_was() {
    let mut harness = a_database("cell-escape");
    did(&mut harness, "plugins pane database/explorer --show");
    did(&mut harness, "plugins run database tables main");
    did(&mut harness, "plugins run database open main.album");
    until_the_database_settles(&mut harness);

    let at = harness.get_by_label("title row 1").rect().center();
    double_click_at(&mut harness, at);
    harness.get_by_label("title row 1").type_text("Thrown away");
    harness.run();
    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(
        did(&mut harness, "plugins run database pending").as_array().expect("none").is_empty(),
        "Escape left nothing pending",
    );
    assert!(the_open_page(&mut harness)["editing"].is_null(), "and closed the box");
}

/// A right click on a table offers `New Table…`, and the rows are the ones that apply to a table.
///
/// `task-1795`: *"I should be able to right click the db or tables and see a popup to create a new
/// table."* The menu is the plugin's own rather than `components::context_menu`, which speaks the
/// window's vocabulary of `actions::Action` — so this checks the rows really are drawn.
#[test]
fn a_right_click_on_a_table_offers_a_new_table() {
    let mut harness = a_database("tree-menu");
    // Opened the way a person opens it, one level a press, because the tree draws a row only once
    // the thing above it is open — `the_database_tree_reads_a_source_one_level_at_a_time`. Asking
    // for the tables down the command line instead would open the source as a side effect, and the
    // press meant to open it would then shut it again.
    for level in ["library", "main", "tables"] {
        harness.get_by_label(level).click();
        until_the_database_settles(&mut harness);
    }
    let row = harness.get_by_label("album").rect();
    right_click_at(&mut harness, row.center());
    for named in ["New Table…", "Open Data", "Show DDL", "Copy Name", "Drop Table"] {
        assert!(harness.get_all_by_label(named).count() > 0, "the menu has no {named} on it");
    }
    // And nothing that belongs to a data source rather than to a table.
    assert_eq!(harness.query_all_by_label("Remove Data Source").count(), 0);

    harness.get_by_label("New Table…").click();
    harness.run();
    assert_eq!(
        did(&mut harness, "plugins view database")["modal"],
        "new-table",
        "choosing the row opened the dialog",
    );
}

/// The dialog draws the statement it is going to send, and redraws it as the form is typed into.
///
/// It comes from `unluminous_db::sql::create_table`, which is the same call `Create` makes — the rule the
/// pending-changes preview already keeps, so what is read and what happens cannot drift apart.
#[test]
fn the_new_table_modal_shows_the_statement_it_will_send() {
    let mut harness = a_database("new-table-modal");
    did(&mut harness, "plugins run database tables main");
    until_the_database_settles(&mut harness);
    // A bare word that names a schema means *make a table there*, which is what the tree's own menu
    // means by a right click on a schema — and is what `new-table main` used to read as a table
    // called `main`, composing `CREATE TABLE "main"."main"`.
    did(&mut harness, "plugins run database new-table main");
    harness.run();
    assert!(harness.get_all_by_label("Table name").count() > 0, "the dialog is up");
    assert_eq!(
        harness.get_by_label("Table name").value(),
        Some(String::new()),
        "the schema is where it goes, not what it is called",
    );

    harness.get_by_label("Table name").click();
    harness.run();
    harness.get_by_label("Table name").type_text("shelf");
    harness.run();
    let sql = did(&mut harness, "plugins view database")["new_table_sql"].clone();
    let sql = sql.as_str().unwrap_or_else(|| panic!("no statement: {sql}"));
    assert!(sql.starts_with("CREATE TABLE \"main\".\"shelf\""), "{sql}");
    assert!(sql.contains("PRIMARY KEY"), "the first column is a key, which a table wants: {sql}");

    // Every identifier is quoted, so a name that is a keyword or holds a space is a non-event, and
    // the statement follows the typing rather than being composed once when the dialog opened.
    harness.get_by_label("Table name").type_text(" of order");
    harness.run();
    let sql = did(&mut harness, "plugins view database")["new_table_sql"].clone();
    assert!(
        sql.as_str().expect("a statement").contains("\"shelf of order\""),
        "the name is quoted and the statement followed the typing: {sql}",
    );

    // Empty, it refuses rather than composing a statement with a hole in it, and says why.
    did(&mut harness, "plugins run database new-table main");
    harness.run();
    assert_eq!(
        did(&mut harness, "plugins view database")["new_table_sql"]["problem"],
        "a table needs a name.",
    );
    harness.get_by_label("Table name").click();
    harness.run();
    harness.get_by_label("Table name").type_text("shelf");
    harness.run();
    harness.snapshot(shot("database_new_table").as_str());
}

/// The dialog's `Create` really makes the table, and the tree has it without anything being pressed.
#[test]
fn creating_a_table_makes_it_and_the_tree_shows_it() {
    let file = a_database_file("new-table-create");
    let mut harness = harness("");
    did(&mut harness, &format!("plugins run database add-source library {}", file.display()));
    did(&mut harness, "plugins pane database/explorer --show");
    did(&mut harness, "plugins run database tables main");
    until_the_database_settles(&mut harness);

    did(&mut harness, "plugins run database new-table main");
    harness.run();
    harness.get_by_label("Table name").click();
    harness.run();
    harness.get_by_label("Table name").type_text("shelf");
    harness.run();
    harness.get_by_label("Create").click();
    until_the_database_settles(&mut harness);

    let connection = rusqlite::Connection::open(&file).expect("opened");
    let made: String = connection
        .query_row("select name from sqlite_master where name = 'shelf'", [], |row| row.get(0))
        .expect("the table was made");
    assert_eq!(made, "shelf");
    let tables = did(&mut harness, "plugins run database tables main");
    let names: Vec<&str> = tables["items"]
        .as_array()
        .expect("the tables")
        .iter()
        .filter_map(|table| table["name"].as_str())
        .collect();
    assert!(names.contains(&"shelf"), "the tree refreshed itself: {names:?}");
}

/// `Add row` puts a row on the screen that can be typed into.
///
/// The whole of the report was that it did not: `Pending::add` recorded a `Change::Add` and the grid
/// drew `grid.rows.rows`, so the pending count went up and the screen did not change. The added rows
/// are drawn after the read ones and every cell in one is opened by the same double click as any
/// other.
#[test]
fn add_row_puts_a_row_on_the_screen_that_can_be_typed_into() {
    let file = a_database_file("add-row");
    let mut harness = harness("");
    did(&mut harness, &format!("plugins run database add-source library {}", file.display()));
    did(&mut harness, "plugins pane database/explorer --show");
    did(&mut harness, "plugins run database tables main");
    did(&mut harness, "plugins run database open main.album");
    until_the_database_settles(&mut harness);
    assert_eq!(harness.query_all_by_label("title row 5").count(), 0, "four rows were read");

    harness.get_by_label("Add row").click();
    harness.run();
    assert!(harness.get_all_by_label("title row 5").count() > 0, "the added row is on the screen");

    let at = harness.get_by_label("title row 5").rect().center();
    double_click_at(&mut harness, at);
    harness.get_by_label("title row 5").type_text("Sketches of Spain");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();

    // One statement, and it is an INSERT carrying what was typed — not an UPDATE of a row that is
    // not there yet, which is what `Pending::set` folding a cell into its own `Change::Add` buys.
    let pending = did(&mut harness, "plugins run database pending");
    let statements = pending.as_array().expect("the statements");
    assert_eq!(statements.len(), 1, "{pending}");
    let sql = statements[0]["sql"].as_str().expect("the sql");
    assert!(sql.starts_with("INSERT INTO \"album\""), "{sql}");
    // And it carries only the cell that was typed in. `Add row` lands on the first cell, so the key
    // was being committed as an empty string when the keyboard moved on — which SQLite refuses on an
    // `integer primary key` with `datatype mismatch`, so no added row could ever be saved.
    assert!(!sql.contains("\"id\""), "an untouched cell must not be written down: {sql}");

    harness.get_by_label("Save 1").click();
    until_the_database_settles(&mut harness);

    let after = the_open_page(&mut harness);
    assert!(after["failure"].is_null(), "the save was refused: {after}");
    assert_eq!(after["pending"], 0, "and nothing is left pending: {after}");
    let connection = rusqlite::Connection::open(&file).expect("opened");
    let id: i64 = connection
        .query_row("select id from album where title = 'Sketches of Spain'", [], |row| row.get(0))
        .expect("the row that was typed into is the row that was inserted");
    assert_eq!(id, 5, "and the engine gave it a key, because nothing pretended to supply one");
}

/// An Inillucent database, written by the engine itself, for the two pictures below.
///
/// **No process id in the name**, for the reason `a_database_file` gives: a data source draws where
/// it points, so a folder that changed between runs would put a different string in an accepted
/// image every time.
///
/// **Not [`fixture`], because what it writes is a database**, made through `inillucent-driver`
/// rather than by writing bytes.
fn an_inillucent_file(name: &str) -> std::path::PathBuf {
    let folder = std::env::temp_dir().join(format!("unluminous-inillucent-shot-{name}"));
    let _ = std::fs::create_dir_all(&folder);
    let file = folder.join("notes.rdb");
    let _ = std::fs::remove_file(&file);
    let database = inillucent_driver::Database::open(&file).expect("a database");
    let connection = database.connect();
    connection
        .execute_batch(
            "create table member (id integer primary key, name text not null, joined text);
             insert into member (id, name, joined) values
               (1, 'Jason', '2026-01-04'), (2, 'Ada', '2026-02-11'), (3, 'Grace', null);
             create virtual table docs using inillucent_search(title, body, dims = 8, mode = 'exact', metric = 'cosine');",
        )
        .expect("a schema");
    for (nth, (title, body)) in [
        ("the release process", "how a release is cut, tagged and published"),
        ("the buffer pool", "frames, version latches and the cooling FIFO"),
        ("drawing a vector", "an embedding is a blob until something reads it as numbers"),
    ]
    .iter()
    .enumerate()
    {
        // A unit vector, so every norm reads 1.000 — which is what makes an unnormalised corpus
        // visible at a glance in the cell.
        let mut values = [0.0f32; 8];
        for (at, slot) in values.iter_mut().enumerate() {
            *slot = ((nth * 8 + at) as f32 * 0.37).sin();
        }
        let length: f32 = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        let mut bytes = Vec::new();
        for value in values.iter() {
            bytes.extend_from_slice(&(value / length).to_le_bytes());
        }
        connection
            .execute(
                "insert into docs(title, body, vector) values(?1, ?2, ?3)",
                &[
                    inillucent_driver::Value::Text((*title).to_owned()),
                    inillucent_driver::Value::Text((*body).to_owned()),
                    inillucent_driver::Value::Blob(bytes),
                ],
            )
            .expect("a row with a vector");
    }
    database.checkpoint().expect("checkpointed");
    // `connection` has no destructor of its own, so nothing is gained by dropping it before
    // `database`, which does close the file.
    drop(database);
    file
}

/// A window with the Database plugin pointed at an Inillucent database, with the tree showing.
fn an_inillucent_database(name: &str) -> Harness<'static, UnluminousApp> {
    let file = an_inillucent_file(name);
    let mut harness = harness("");
    did(&mut harness, &format!("plugins run database add-source notes {}", file.display()));
    did(&mut harness, "plugins pane database/explorer --show");
    harness.run();
    harness
}

/// A search index says what it declared, and the five tables it keeps its state in are marked.
///
/// The row is where the four things a person needs before trusting a result live — how wide the
/// vectors are, whether the answers are exact, which distance they are exact about, and which
/// analysis produced the terms — because they are what the index *is* rather than something to open.
#[test]
fn the_database_tree_says_what_a_search_index_declared() {
    let mut harness = an_inillucent_database("tree");
    harness.get_by_label("notes").click();
    until_the_database_settles(&mut harness);
    harness.get_by_label("main").click();
    until_the_database_settles(&mut harness);
    // The search index is in a folder of its own, second, because on this engine it is what somebody
    // came to look at.
    harness.get_by_label("search").click();
    until_the_database_settles(&mut harness);

    let view = did(&mut harness, "plugins view database");
    let kinds: Vec<(&str, &str)> = view["tree"][0]["items"][0]["items"]
        .as_array()
        .expect("the items of the schema")
        .iter()
        .filter_map(|item| Some((item["name"].as_str()?, item["kind"].as_str()?)))
        .collect();
    assert!(kinds.contains(&("docs", "search index")), "{kinds:?}");
    assert!(kinds.contains(&("docs_content", "shadow table")), "{kinds:?}");
    // And the engine reports what it cannot do, which is what the absent Stop button is drawn from.
    let capabilities = did(&mut harness, "plugins run database capabilities notes");
    let cancel = capabilities["capabilities"]
        .as_array()
        .expect("the table")
        .iter()
        .find(|row| row["name"] == serde_json::json!("cancel"))
        .expect("the cancel row");
    assert_eq!(cancel["support"], "no");
    harness.snapshot(shot("database_inillucent_tree").as_str());
}

/// A vector is drawn as a vector: its width, the first of its components, and its length.
///
/// The cell used to be `32 bytes: 3f 00 00 00…`, which tells somebody the row has *something*. The
/// norm is in the summary because this engine's distance is cosine, so a corpus stored unnormalised
/// is visible at a glance rather than after a query nobody thought to run.
#[test]
fn a_vector_is_drawn_as_a_vector_rather_than_as_its_bytes() {
    let mut harness = an_inillucent_database("grid");
    did(&mut harness, "plugins run database tables main");
    did(&mut harness, "plugins run database open docs");
    until_the_database_settles(&mut harness);

    let page = did(&mut harness, "plugins run database result");
    assert_eq!(page["kind"], "grid");
    // A search table has no key of its own, so its rows are read rather than changed - the ordinary
    // addressability rule rather than a special case.
    assert_eq!(page["editable"], serde_json::json!(false));
    // **The vector column is in the result at all**, which is the line that decides whether any of
    // this is true of the grid: it arrives through a hidden column, so `select *` would have left it
    // out and drawn the title and the body of a row whose whole point is the embedding beside them.
    let columns: Vec<&str> = page["rows"]["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .filter_map(|column| column["name"].as_str())
        .collect();
    assert!(columns.contains(&"vector"), "{columns:?}");

    let read = did(&mut harness, "plugins run database vector 1 vector");
    assert_eq!(read["dimensions"], serde_json::json!(8));
    assert!((read["norm"].as_f64().unwrap_or_default() - 1.0).abs() < 1.0e-5, "{read}");
    harness.snapshot(shot("database_inillucent_vector").as_str());
}

/// **Switching back to a tab that has already been laid out lays nothing out.**
///
/// A hidden tab keeps its whole layout so that coming back to it is instant, which is the reason
/// `tasks/task-1813-performance-review-tdd.md` section 4 refuses to evict one: the 538 KB reference
/// file costs 64 ms to lay out from scratch, so a switch that rebuilt it would be a visible stall.
/// Compacting the displaced tab's capacity must not quietly cost that, and a counter is the only
/// thing that can tell the two apart - nothing else in the window would look any different.
#[test]
fn showing_a_tab_that_was_already_laid_out_does_not_lay_it_out_again() {
    let folder = sample_folder();
    let mut harness = harness("");
    harness.state_mut().open_path_permanently(&folder.join("readme.md")).expect("the file opens");
    harness.run();
    let first = harness.state().files.active_index();
    harness.state_mut().open_path_permanently(&folder.join("notes.txt")).expect("the file opens");
    harness.run();
    let laid_out = harness.state().layouts_built();
    assert!(laid_out > 0, "opening two files lays them out");

    // Back to the first, which is hidden and was compacted when the second displaced it.
    harness.state_mut().files.show(first);
    harness.run();
    assert_eq!(
        harness.state().layouts_built(),
        laid_out,
        "showing a cached tab reused its layout instead of building another"
    );
    // And it is really the file that is showing, so nothing was skipped by showing the wrong tab.
    assert_eq!(harness.state().document().text().to_string(), "# Unluminous\n");
    assert!(!harness.state().layout().lines.is_empty(), "with its lines still in place");
}
