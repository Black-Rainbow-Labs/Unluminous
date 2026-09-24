//! The two plugins that draw their own pane: the Agent-Tasks board and the Agent-Chat pane.
//!
//! The board's lanes, cards, three listings and ticket modal, drawn through `vello_cpu` because
//! `epaint` cannot draw an inset shadow, a diagonal gradient, a glow round a circle or a rounded
//! clip. Then the chat pane: the transcript, the composer, the streaming, the tool blocks, the
//! pictures and the provider list.
//!
//! **No test here makes a network request or starts an agent.** A conversation is built out of the
//! same `Reply` values the transport would have produced, which is the terminal's own rule. The two
//! tests that really call `send` point the chosen row at a program that does not exist first, because
//! the rows that ship run `claude` and `codex` and both are installed on the machine this is
//! developed on.
//!
//! **19 of the 66 tests here take a picture**, and the rest read the window's state back —
//! through `unluminous-cli` rather than out of the window's own fields, which is the rule the
//! debugger's tests already keep.

mod common;

use common::*;

use egui::{vec2, Modifiers};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::components::title_bar::MenuPlacement;
use unluminous_app::UnluminousApp;

/// What the contributed pane called `key` is called, from its own manifest.
fn pane_label(harness: &Harness<'static, UnluminousApp>, key: &str) -> String {
    let slot = harness
        .state()
        .plugin_ui
        .slot_of(key)
        .unwrap_or_else(|| panic!("there is no contributed pane called {key}"));
    harness.state().plugin_ui.pane(slot).expect("the pane").label.clone()
}

// ---------------------------------------------------------------------------- the plugins that draw
//
// `tasks/ui-plugin-architecture.md` is the design and `tasks/agent-tasks-plugin-tdd.md` is the plugin.
// Every test below drives the real window: the rail button is pressed rather than a field being set, and
// the answer is read back through `unluminous-cli` rather than out of the window's own state, which is the
// rule the debugger's tests already keep.

/// `task-28`: "Let\'s just have agent-tasks be opened in a tab, rather than that other pane view."
///
/// The board used to contribute a pane docked to the right as well as a tab, and the same board drew in
/// both. A 420 point column shows one lane at a time and needs scrolling sideways to reach the second, so
/// the pane is gone and the tab is the only place the board is drawn.
///
/// Unluminous\'s pane machinery is untouched, and this is the test that says so: the board contributes no pane,
/// and `the_pane_is_moved_and_put_away_from_the_command_line` drives one from a manifest written for it.
#[test]
fn the_board_contributes_a_pane_and_no_tab() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    let listed = did(&mut harness, "plugins list");
    let plugins = listed["plugins"].as_array().expect("the plugins");
    let board = plugins
        .iter()
        .find(|plugin| plugin["id"] == "agent-tasks")
        .expect("the agent-tasks plugin");
    assert_eq!(board["kind"], "ui");
    assert_eq!(board["provider"], "agent-tasks");
    let contributes: Vec<&str> = board["contributes"]
        .as_array()
        .expect("what it adds")
        .iter()
        .filter_map(|it| it.as_str())
        .collect();
    // `task-1848`: "Agent tasks should be its own pane, rather than a tab." Turned round rather than
    // deleted, so the shape stays pinned whichever way it is.
    assert_eq!(
        contributes,
        ["pane", "menu", "settings page"],
        "a pane, a menu and a page, and no tab"
    );

    // The board takes no dock slot. Agent-Chat's pane does — `task-1767` — so this is checked by name
    // rather than by slot number: which number a pane is in comes from the manifests and moves when a
    // plugin is switched on or off.
    assert!(
        harness.state().plugin_ui.slot_of("agent-tasks/board").is_some(),
        "the board contributes a pane, so it is in a slot and can be dragged to any edge"
    );
    let _ = Panel::Plugin(0);
    let panels = did(&mut harness, "panel list");
    let names: Vec<&str> = panels["panels"]
        .as_array()
        .expect("the panels")
        .iter()
        .filter_map(|it| it["panel"].as_str())
        .collect();
    assert_eq!(
        names,
        ["explorer", "terminal", "run", "debug", "space"],
        "`panel list` is Unluminous\'s own five; a contributed pane is moved with `plugins pane`: {names:?}"
    );

    // The board is reached from its menu entry instead, which is the control a person uses. Since
    // `task-1848` that entry is one level deeper: `Plugins` in the bar, then `Show Board` under the
    // `Agent-Tasks` heading — a submenu is drawn inline here, so the entry is on the screen as soon as
    // the `Plugins` menu is open and there is nothing to click on the heading itself.
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    steady(&mut harness);
    harness.get_by_label(unluminous_app::app::actions::PLUGINS_MENU).click();
    steady(&mut harness);
    harness.get_by_label("Show Board").click();
    steady(&mut harness);
    let slot = harness.state().plugin_ui.slot_of("agent-tasks/board").expect("its slot");
    assert!(
        harness.state().plugin_ui.is_visible(slot),
        "the menu entry showed the board's pane, which is the control a person uses"
    );
}

/// A contributed pane is shown, moved and put away from the command line.
///
/// **From a manifest written for this test**, since `task-28` took the pane out of Agent-Tasks\'s own
/// manifest. That is a better test than the one it replaces: Unluminous\'s pane machinery used to be verified
/// only through one plugin\'s incidental use of it, so removing that plugin\'s pane would have taken the
/// coverage with it. `a_manifest_changed_by_hand_takes_effect_on_a_reload_with_no_restart` is where this
/// pattern comes from — a plugin folder on disk shadows the bundled one of the same id.
#[test]
fn the_pane_is_moved_and_put_away_from_the_command_line() {
    use unluminous_app::app::dock::{Panel, Side};
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-plugin-pane");
    let settings = folder.join(".unluminous-settings");
    let plugin = settings.join("plugins").join("agent-tasks");
    std::fs::create_dir_all(&plugin).expect("a plugin folder");
    std::fs::write(
        plugin.join("plugin.conf"),
        "plugin.id = agent-tasks\nplugin.name = Agent-Tasks\nplugin.kind = ui\n\
         ui.provider = agent-tasks\npane.id = board\npane.label = Agent-Tasks\npane.side = right\n\
         pane.width = 420\n",
    )
    .expect("a manifest that contributes a pane");
    let mut harness = harness_in(&folder);
    harness.state_mut().use_store(unluminous_app::services::store::Store::at(&settings));
    steady(&mut harness);

    // By name rather than by slot number: Agent-Chat contributes a pane too since `task-1767`, and
    // which slot each is in comes from the manifests.
    let slot = harness
        .state()
        .plugin_ui
        .slot_of("agent-tasks/board")
        .expect("the manifest written for this test contributes a pane") as u8;
    let shown = did(&mut harness, "plugins pane agent-tasks/board --show");
    assert_eq!(shown["showing"], true);
    assert_eq!(shown["side"], "right");
    // The same drag the header takes, asked for by name.
    let moved = did(&mut harness, "plugins pane agent-tasks/board --side bottom");
    assert_eq!(moved["side"], "bottom");
    assert_eq!(side_of(&harness, Panel::Plugin(slot)), Side::Bottom);
    let away = did(&mut harness, "plugins pane agent-tasks/board --hide");
    assert_eq!(away["showing"], false);
    steady(&mut harness);
    // A pane that is put away takes no room, which is what `Rect::ZERO` in `regions` means. Read off the
    // rectangles the frame actually used rather than off `panel_area`, which answers where a panel
    // *would* be so that a menu can offer to move one that is hidden.
    assert_eq!(harness.state().panel_rect_for_tests(Panel::Plugin(slot)).width(), 0.0);
    let _ = Side::Right;
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn a_pane_or_a_command_nobody_has_is_refused_with_what_there_is() {
    let mut harness = harness("");
    assert_eq!(refused(&mut harness, "plugins pane agent-tasks/nothing --show"), "not-found");
    // The board has a pane since `task-1848`, so asking for it by name is not a refusal any more — what
    // is still refused is a pane the plugin does not have, which the line above covers.
    assert_eq!(refused(&mut harness, "plugins tab agent-tasks/board --open"), "not-found");
    assert_eq!(refused(&mut harness, "plugins pane chat/thread --show"), "not-found");
    assert_eq!(refused(&mut harness, "plugins tab agent-tasks/nothing --open"), "not-found");
    assert_eq!(refused(&mut harness, "plugins view rust"), "not-found");
    assert_eq!(refused(&mut harness, "plugins show nothing-like-this"), "not-found");
    let reply = run(&mut harness, "plugins run agent-tasks fly");
    assert!(!reply.ok);
    assert!(reply.message.contains("no `fly` command"), "{}", reply.message);
    assert!(reply.message.contains("board"), "the refusal lists what there is: {}", reply.message);
}

/// A contributed tab opens beside the file tabs, and its own command toggles it.
///
/// **From a manifest written for this test**, since `task-1848` moved the board and the database workspace
/// to panes and no plugin that ships contributes a tab. That is the pattern
/// `the_pane_is_moved_and_put_away_from_the_command_line` already uses, for the reason it gives: Unluminous's
/// tab machinery is part of the plugin contract, and testing it only through a plugin's incidental use of
/// it means the coverage leaves when that plugin changes shape.
#[test]
fn a_contributed_tab_opens_in_the_editing_area_beside_the_file_tabs() {
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-plugin-tab");
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
    did(&mut harness, "plugins tab agent-tasks/board --open");
    steady(&mut harness);
    // **Asserted on the tab being there rather than on how many tabs there are.** `harness_in` opens the
    // sample project's own files, so a count is a count of those plus this, and it is the board's tab that
    // this test is about.
    assert!(
        harness.state().files.index_of_plugin_tab("agent-tasks/board").is_some(),
        "the board's tab is open beside the file tabs"
    );
    // The plugin's own tab, by name. `some tab is unmodified` was true of the file beside it and said nothing
    // about the board.
    let tabs = did(&mut harness, "status --section tabs");
    let rows = tabs["tabs"].as_array().expect("the tabs");
    let board = rows.iter().find(|tab| tab["name"] == "Agent-Tasks").unwrap_or_else(|| {
        panic!("the board's tab should be called what its manifest calls it: {rows:?}")
    });
    assert_eq!(board["modified"], false, "a plugin tab is never modified");
    // And saving it writes nothing, rather than writing an empty `untitled.md` into the project.
    let before: Vec<std::path::PathBuf> = std::fs::read_dir(sample_folder())
        .expect("the sample folder")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    did(&mut harness, "action run save");
    steady(&mut harness);
    let after: Vec<std::path::PathBuf> = std::fs::read_dir(sample_folder())
        .expect("the sample folder")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    assert_eq!(before.len(), after.len(), "saving a plugin tab wrote a file");

    // **`--open` opens and never closes**, whatever is showing: a command named for what it does has to do
    // it. Asking again shows the one that is open rather than opening a second.
    let tabs_showing = harness.state().files.len();
    did(&mut harness, "plugins tab agent-tasks/board --open");
    steady(&mut harness);
    assert_eq!(
        harness.state().files.len(),
        tabs_showing,
        "asking twice does not open a second one"
    );

    // **The rail button toggles, and that is `task-1848`**: "I can't untoggle it to hide it. It should
    // always open/close." `open_the_plugin_tab` only ever opened, so the button could be pressed once and
    // then did nothing anybody could see. The action below is what the button and the menu entry run.
    did(&mut harness, "action run plugin-tab:agent-tasks/board");
    steady(&mut harness);
    assert!(
        harness.state().files.index_of_plugin_tab("agent-tasks/board").is_none(),
        "pressing it while the tab is showing closes it, which is what every other rail button does"
    );
    did(&mut harness, "action run plugin-tab:agent-tasks/board");
    steady(&mut harness);
    assert!(
        harness.state().files.index_of_plugin_tab("agent-tasks/board").is_some(),
        "and pressing it again brings it back"
    );

    // And `--close` closes it, which is the other half of the named pair.
    let closed = did(&mut harness, "plugins tab agent-tasks/board --close");
    assert_eq!(closed["open"], false);
    steady(&mut harness);
    assert!(harness.state().files.index_of_plugin_tab("agent-tasks/board").is_none());
}

#[test]
fn the_plugins_menu_is_after_unluminouss_own_six_and_its_entries_run_through_one_path() {
    let mut harness = harness("");
    let listed = did(&mut harness, "action list");
    let actions = listed["actions"].as_array().expect("the actions").clone();
    // The menus in the order they are drawn, each named once.
    let mut menus: Vec<String> = Vec::new();
    for entry in &actions {
        let menu = entry["menu"].as_str().unwrap_or_default().to_owned();
        if !menu.is_empty() && !menus.contains(&menu) {
            menus.push(menu);
        }
    }
    // The list carries a submenu's own name as well as a menu's, so the six built in menus are checked
    // by where they are rather than by the list being exactly seven long.
    let at = |name: &str| menus.iter().position(|menu| menu == name);
    let built_in: Vec<usize> = ["Unluminous", "File", "Edit", "View", "Run", "Git"]
        .iter()
        .map(|name| at(name).unwrap_or_else(|| panic!("{name} is missing: {menus:?}")))
        .collect();
    assert!(
        built_in.windows(2).all(|pair| pair[0] < pair[1]),
        "Unluminous's own six are still in their own order: {menus:?}"
    );
    let plugin = at("Agent-Tasks").unwrap_or_else(|| panic!("the plugin's menu: {menus:?}"));
    assert!(
        plugin > built_in[5],
        "the plugin's menu comes after Git, so no entry a hand already knows has moved: {menus:?}"
    );
    // Its entries are reachable by name, which is what `action_names.rs` gives them for nothing.
    let entries: Vec<&serde_json::Value> =
        actions.iter().filter(|entry| entry["menu"] == "Agent-Tasks").collect();
    let labels: Vec<&str> = entries.iter().filter_map(|entry| entry["label"].as_str()).collect();
    // `Show Board` since `task-1848` made the board a pane; it was `Open Board` when it was a tab.
    assert!(labels.contains(&"Show Board"), "{labels:?}");
    assert!(labels.contains(&"Reload Board"), "{labels:?}");
    // There is no `Sync JIRA` entry, because this board does not sync and a control that cannot apply is absent.
    assert!(!labels.contains(&"Sync JIRA"), "{labels:?}");
    // A submenu's own entries are listed under the submenu's name, which is what `Recent Projects`
    // already does, so `New` is a menu of its own in the list and `Task` is in it.
    let nested: Vec<&str> = actions
        .iter()
        .filter(|entry| entry["menu"] == "New")
        .filter_map(|entry| entry["label"].as_str())
        .collect();
    assert_eq!(nested, ["Task", "Epic", "Sprint"], "the nested submenu is read recursively");
    let names: Vec<&str> = entries.iter().filter_map(|entry| entry["name"].as_str()).collect();
    assert!(
        names.iter().any(|name| name.starts_with("plugin-run:agent-tasks:")),
        "a contributed entry names itself, so `action run` reaches it: {names:?}"
    );
    // No plugin entry claims a chord, because two menu items claiming one key equivalent is a real
    // fault on macOS and there is a test for it.
    assert!(
        entries.iter().all(|entry| entry["shortcut"].as_str().unwrap_or_default().is_empty()),
        "a contributed entry must not claim a shortcut: {entries:?}"
    );
    // And running one by that name goes down the same path the entry does.
    let name = names
        .iter()
        // `open-pane` since `task-1848` made the board a pane; it was `open-tab` when it was a tab.
        .find(|name| name.ends_with(":open-pane"))
        .expect("the Show Board entry names itself");
    did(&mut harness, &format!("action run {name}"));
    steady(&mut harness);
    assert!(harness.state().plugin_ui.is_open("agent-tasks"), "the menu entry showed the board");
}

/// No two controls in the rail answer to one name.
///
/// `design/style-guide.md` states the rule and `task-1848` is what broke it: with three plugins each
/// contributing a pane, and each naming its menu after itself, the rail had a `Database` button beside a
/// `Database` menu — a distinction the report calls out as doing nothing that can be told apart. So a
/// contributed pane's button is `<label> pane`, which is `Terminal tile`, `Run tile` and `Version Control`
/// applied to the plugins' own.
///
/// Asserted by asking for each name the rail should have and insisting there is exactly one node with it,
/// which is what a test looking for a control actually does — `get_by_label` panics when two match, and
/// that panic is the fault this pins.
#[test]
fn no_two_controls_in_the_rail_share_a_name() {
    let mut harness = harness("");
    steady(&mut harness);
    for name in ["Agent-Chat pane", "Agent-Tasks pane", "Database pane"] {
        let found = harness.get_all_by_label(name).count();
        assert_eq!(found, 1, "`{name}` should name exactly one control and names {found}");
    }
    // And with the menu bar drawn in the window, each plugin's plain name is the heading of its submenu
    // under `Plugins` — which is the name the rail buttons had to give way to, and the collision that
    // made a test find two. `task-1848` moved the three menus into one, so the name is on a heading
    // rather than on a menu in the bar, and it is drawn only while that menu is open.
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    steady(&mut harness);
    for name in ["Agent-Chat", "Agent-Tasks", "Database"] {
        // `query_all_by_label` rather than `get_all_by_label`: the `get_` family panics when nothing
        // matches, so it cannot express "there is none of this".
        let found = harness.query_all_by_label(name).count();
        assert_eq!(found, 0, "`{name}` is inside the Plugins menu, which is shut: found {found}");
    }
    harness
        .get_all_by_label(unluminous_app::app::actions::PLUGINS_MENU)
        .next()
        .expect("the Plugins menu")
        .click();
    steady(&mut harness);
    for name in ["Agent-Chat", "Agent-Tasks", "Database"] {
        let found = harness.query_all_by_label(name).count();
        assert_eq!(found, 1, "`{name}` is one heading under Plugins and names {found} controls");
    }
}

#[test]
fn nothing_the_plugin_owns_is_built_until_its_button_is_pressed() {
    // The reference editor's own rule, and its documented reason: a tool window nobody clicks loads and runs no
    // plugin code. Here it is the difference between opening a database when Unluminous starts and opening it
    // when somebody first looks at the board.
    let mut harness = harness("");
    assert!(
        !harness.state().plugin_ui.is_open("agent-tasks"),
        "not opened by loading the manifest"
    );
    did(&mut harness, "plugins pane agent-tasks/board --show");
    assert!(harness.state().plugin_ui.is_open("agent-tasks"), "opened by being shown");
    // And switching it off closes it, so it drops the board file it held.
    did(&mut harness, "plugins disable agent-tasks");
    steady(&mut harness);
    assert!(!harness.state().plugin_ui.is_open("agent-tasks"));
    let listed = did(&mut harness, "plugins list");
    let board = listed["plugins"]
        .as_array()
        .expect("the plugins")
        .iter()
        .find(|plugin| plugin["id"] == "agent-tasks")
        .expect("still installed")
        .clone();
    assert_eq!(board["enabled"], false);
}

#[test]
fn switching_the_plugin_off_withdraws_every_contribution_in_the_same_frame() {
    // Agent-Tasks contributes a tab, a menu and a Settings page (`task-28` took its pane out), and
    // all three go together the moment the plugin is switched off — the rule `Plugins::renders`
    // already keeps for a Mermaid diagram: the window asks before it draws.
    let mut harness = harness("Some prose.");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    steady(&mut harness);
    // **The pane, since `task-1848` made the board one.** This asserted a tab opening and closing; what
    // withdrawing a contribution means for a pane is that its slot goes, so that is what is checked.
    let slot = harness.state().plugin_ui.slot_of("agent-tasks/board").expect("its slot");
    assert!(harness.state().plugin_ui.is_visible(slot), "the board's pane is showing");
    did(&mut harness, "plugins disable agent-tasks");
    steady(&mut harness);
    assert!(
        harness.state().plugin_ui.slot_of("agent-tasks/board").is_none(),
        "the pane went with the plugin, in the same frame"
    );
    let listed = did(&mut harness, "action list");
    let menus: Vec<&str> = listed["actions"]
        .as_array()
        .expect("the actions")
        .iter()
        .filter_map(|entry| entry["menu"].as_str())
        .collect();
    assert!(!menus.contains(&"Agent-Tasks"), "the menu goes with the plugin: {menus:?}");
    assert_eq!(refused(&mut harness, "plugins view agent-tasks"), "not-found");
    // And switching it back on brings all of it back, with no restart.
    did(&mut harness, "plugins enable agent-tasks");
    steady(&mut harness);
    did(&mut harness, "plugins pane agent-tasks/board --show");
    steady(&mut harness);
    let slot = harness.state().plugin_ui.slot_of("agent-tasks/board").expect("its slot is back");
    assert!(harness.state().plugin_ui.is_visible(slot), "the pane is offered again");
}

#[test]
fn the_board_can_be_read_and_changed_entirely_from_the_command_line() {
    // Unluminous's rule: everything a person can do in the window an agent can do too, through the same code.
    // A board drawn with `egui` is invisible to a test and to an agent unless it can be read, which is
    // what `plugins view` is for — a screenshot cannot answer how many tickets are in progress.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    let empty = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(empty["total"], 0);
    let lanes: Vec<&str> = empty["lanes"]
        .as_array()
        .expect("the lanes")
        .iter()
        .filter_map(|lane| lane["status"].as_str())
        .collect();
    assert_eq!(
        lanes,
        ["new", "qa_failed", "in_progress", "agent_done"],
        "four lanes, in drawn order"
    );

    let made = did(&mut harness, "plugins run agent-tasks new-task Rewrite the importer");
    assert_eq!(made["task"], "task-1");
    let with_one = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(with_one["total"], 1);
    let new_lane = with_one["lanes"].as_array().expect("the lanes")[0].clone();
    assert_eq!(new_lane["count"], 1);
    assert_eq!(
        new_lane["cards"].as_array().expect("the cards")[0]["title"],
        "Rewrite the importer"
    );

    // Todos and comments, which are what the card's two counts are.
    did(&mut harness, "plugins run agent-tasks todo-add task-1 Read the old importer");
    did(&mut harness, "plugins run agent-tasks todo-add task-1 Write the new one");
    did(&mut harness, "plugins run agent-tasks todo-done task-1 1");
    did(&mut harness, "plugins run agent-tasks comment task-1 The format changed in April.");
    let card = did(&mut harness, "plugins view agent-tasks")["lanes"]
        .as_array()
        .expect("the lanes")[0]["cards"]
        .as_array()
        .expect("the cards")[0]
        .clone();
    assert_eq!(card["todos"], "1/2");
    assert_eq!(card["comments"], 1);

    // Moved between lanes, which is what a drag does.
    did(&mut harness, "plugins run agent-tasks move-task task-1 agent_done 0");
    let moved = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(moved["lanes"].as_array().expect("the lanes")[0]["count"], 0);
    assert_eq!(moved["lanes"].as_array().expect("the lanes")[3]["count"], 1);

    // A lane that does not exist is refused with the four that do.
    let reply = run(&mut harness, "plugins run agent-tasks move-task task-1 done 0");
    assert!(!reply.ok);
    assert!(reply.message.contains("agent_done"), "{}", reply.message);
}

#[test]
fn a_ticket_carries_its_todos_and_its_comments_when_it_is_asked_for_by_key() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task Plugin architecture");
    did(&mut harness, "plugins run agent-tasks todo-add task-1 Weigh the four mechanisms");
    did(&mut harness, "plugins run agent-tasks comment task-1 Zed has no UI surface at all.");
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    assert_eq!(ticket["task"]["key"], "task-1");
    assert_eq!(ticket["todos"].as_array().expect("the todos").len(), 1);
    assert_eq!(ticket["todos"].as_array().expect("the todos")[0]["done"], false);
    let comments = ticket["comments"].as_array().expect("the comments");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0]["author"], "human");
    assert!(comments[0]["body"].as_str().expect("a body").contains("Zed"));
    assert_eq!(refused(&mut harness, "plugins run agent-tasks task task-99"), "failed");
}

#[test]
fn the_search_finds_a_ticket_by_its_key_its_title_or_its_description() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task Plugin architecture for UI");
    did(&mut harness, "plugins run agent-tasks new-task Rewrite the importer");
    let found = did(&mut harness, "plugins run agent-tasks search plugin");
    assert_eq!(
        found["found"].as_array().expect("what it found"),
        &vec![serde_json::json!("task-1")]
    );
    let by_key = did(&mut harness, "plugins run agent-tasks search task-2");
    assert_eq!(by_key["found"].as_array().expect("what it found").len(), 1);
    let nothing = did(&mut harness, "plugins run agent-tasks search mermaid");
    assert!(nothing["found"].as_array().expect("what it found").is_empty());
}

#[test]
fn plugins_show_lists_every_command_the_board_answers() {
    // The catalogue is `&'static`, so a plugin's own commands cannot be rows in it. `plugins show` is
    // what an agent reads instead, and it asks the provider rather than a list written down twice.
    let mut harness = harness("");
    let shown = did(&mut harness, "plugins show agent-tasks");
    assert_eq!(shown["kind"], "ui");
    let commands: Vec<&str> = shown["commands"]
        .as_array()
        .expect("the commands")
        .iter()
        .filter_map(|command| command["command"].as_str())
        .collect();
    for expected in [
        "board",
        "task",
        "new-task",
        "move-task",
        "todo-add",
        "comment",
        "start",
        "resume",
        "search",
    ] {
        assert!(commands.contains(&expected), "`{expected}` is not offered: {commands:?}");
    }
    assert!(
        shown["commands"].as_array().expect("the commands").iter().all(|command| {
            command["summary"].as_str().is_some_and(|summary| !summary.is_empty())
        }),
        "every command says what it does, because the description is what makes an agent choose it"
    );
    assert!(!shown["limitations"].as_str().expect("what it does not do").is_empty());
}

#[test]
fn a_manifest_changed_by_hand_takes_effect_on_a_reload_with_no_restart() {
    // The property this design has that the reference editor's dynamic plugins buy with a page of restrictions: a provider
    // is already in the binary, so loading a plugin is reading a file. This writes a manifest, reloads, and
    // asserts the window changed — which is the claim. Disabling a plugin instead would have tested the tick
    // box.
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-plugin-reload");
    let settings = folder.join(".unluminous-settings");
    let plugin = settings.join("plugins").join("agent-tasks");
    std::fs::create_dir_all(&plugin).expect("a plugin folder");
    let manifest = |label: &str, side: &str| {
        format!(
            "plugin.id = agent-tasks\nplugin.name = Agent-Tasks\nplugin.kind = ui\n\
             ui.provider = agent-tasks\npane.id = board\npane.label = {label}\npane.side = {side}\n"
        )
    };
    std::fs::write(plugin.join("plugin.conf"), manifest("The Board", "left")).expect("a manifest");
    let mut harness = harness_in(&folder);
    harness.state_mut().use_store(unluminous_app::services::store::Store::at(&settings));
    steady(&mut harness);
    // A plugin on disk shadows the bundled one of the same id, which is what makes a manifest correctable by
    // hand at all.
    let shown = did(&mut harness, "plugins show agent-tasks");
    assert_eq!(shown["bundled"], false, "the one on disk is the one that loaded");
    let listed = did(&mut harness, "plugins list");
    assert!(
        listed["plugins"]
            .as_array()
            .expect("the plugins")
            .iter()
            .any(|it| it["id"] == "agent-tasks"),
        "it is installed"
    );
    assert_eq!(
        pane_label(&harness, "agent-tasks/board"),
        "The Board",
        "the label came from the file"
    );
    // Change the file and reload. No restart, and the window is different in the same frame.
    std::fs::write(plugin.join("plugin.conf"), manifest("Tickets", "bottom"))
        .expect("a changed manifest");
    let reloaded = did(&mut harness, "plugins reload");
    assert!(reloaded["refused"].as_array().expect("what was refused").is_empty());
    steady(&mut harness);
    assert_eq!(
        pane_label(&harness, "agent-tasks/board"),
        "Tickets",
        "the reload read the file again"
    );
    // And a manifest that will not parse is skipped with its reason rather than stopping Unluminous.
    std::fs::write(plugin.join("plugin.conf"), "plugin.id = agent-tasks\nplugin.kind = wasm\n")
        .expect("a manifest Unluminous refuses");
    let refused = did(&mut harness, "plugins reload");
    let reasons = refused["refused"].as_array().expect("what was refused");
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(reasons[0].as_str().expect("a reason").contains("wasm"), "{reasons:?}");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_plugins_pane_takes_the_windows_own_transparency_and_font() {
    // A pane that ignored the opacity setting would be the one opaque rectangle in a window whose
    // transparency is the whole character of the product. The provider is handed the value rather than
    // reaching for it, so this is what proves the handing over works.
    use unluminous_app::services::plugin_ui::Look;
    let mut harness = harness("");
    did(&mut harness, "settings set appearance.background.opacity 0.5");
    did(&mut harness, "settings set appearance.font.size 17");
    steady(&mut harness);
    let look = Look::of(&harness.state().settings, &harness.state().renderer);
    assert_eq!(look.opacity, 0.5);
    assert_eq!(look.font_size, 17.0);
    assert_eq!(
        look.ground(look.palette.editor).a(),
        128,
        "half opacity reaches the pane's own ground"
    );
}

/// The board in a narrow editing area, which is the layout the pane used to be for.
///
/// `task-28` removed the pane, and the one lane at a time layout it chose below 900 points did not go with
/// it: a tab in a narrow window reaches the same code. So this is the same picture of the same arrangement,
/// taken where it still happens.
#[test]
fn the_board_in_a_narrow_editing_area() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    // Narrow, by giving most of the width to the explorer, which is what a 420 point pane used to be.
    did(&mut harness, "panel size explorer --width 700");
    did(&mut harness, "plugins run agent-tasks new-sprint Current Sprint");
    did(
        &mut harness,
        "plugins run agent-tasks new-task Unluminous \u{2014} Plugin architecture for UI",
    );
    did(&mut harness, "plugins run agent-tasks new-task Rust vector db");
    did(&mut harness, "plugins run agent-tasks new-task Rust vector synthetic testing");
    did(&mut harness, "plugins run agent-tasks move-task task-2 in_progress 0");
    did(&mut harness, "plugins run agent-tasks move-task task-3 in_progress 1");
    did(&mut harness, "plugins run agent-tasks todo-add task-2 Read the pgvector docs");
    did(&mut harness, "plugins run agent-tasks todo-done task-2 1");
    did(
        &mut harness,
        "plugins run agent-tasks comment task-2 Graded against a configured baseline.",
    );
    // `new-task` opens the ticket it made, so the lanes are asked for by name rather than assumed.
    did(&mut harness, "plugins run agent-tasks board");
    steady(&mut harness);
    harness.snapshot(shot("agent_tasks_narrow_tab").as_str());
}

#[test]
fn the_board_as_a_tab_filling_the_editing_area() {
    let mut harness = harness("");
    did(&mut harness, "plugins run agent-tasks new-sprint Current Sprint");
    did(
        &mut harness,
        "plugins run agent-tasks new-task Unluminous \u{2014} Plugin architecture for UI",
    );
    did(&mut harness, "plugins run agent-tasks new-task Rust vector db");
    did(&mut harness, "plugins run agent-tasks move-task task-2 agent_done 0");
    did(&mut harness, "plugins run agent-tasks board");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    steady(&mut harness);
    harness.snapshot(shot("agent_tasks_tab").as_str());
}

/// `task-1765`: the decoration switched off draws the board flat, in the same frame.
///
/// The off switch is a control like any other and Unluminous's rule is that a control has a test. It is also
/// the answer for a machine where the rasteriser is too slow, so it has to actually work rather than
/// merely exist — and the flat form is a separate path through every part of the board, since each one
/// asks `look.chrome.is_recording()` and draws the other shape when it is false.
#[test]
fn the_board_with_its_decoration_switched_off() {
    let mut harness = harness("");
    did(&mut harness, "settings set plugins.chrome false");
    did(&mut harness, "plugins run agent-tasks new-sprint Current Sprint");
    did(
        &mut harness,
        "plugins run agent-tasks new-task Unluminous \u{2014} Plugin architecture for UI",
    );
    did(&mut harness, "plugins run agent-tasks new-task Rust vector db");
    did(&mut harness, "plugins run agent-tasks move-task task-2 agent_done 0");
    did(&mut harness, "plugins run agent-tasks board");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    steady(&mut harness);
    harness.snapshot(shot("agent_tasks_flat").as_str());
}

/// `task-1765`: the same board in two windows is the same picture, pixel for pixel.
///
/// The `Decor` list being a pure function of what is drawn is asserted with no window in
/// `vello_canvas::tests::the_same_drawing_twice_gives_an_identical_list`, over a drawing written out by
/// hand. This is the same property over a **real board**, all the way through the rasteriser and the
/// texture upload, which is what the other 414 accepted images quietly rest on: an image accepted today
/// and compared tomorrow means nothing if the same state can produce two pictures.
///
/// **Two windows rather than two frames of one**, because the second frame of one window is a cache hit —
/// the canvas compares the drawing against what it already has and hands back the same texture, which
/// proves the cache works and not that the rasteriser is deterministic. Two windows rasterise twice.
#[test]
fn the_same_board_in_two_windows_is_the_same_picture() {
    fn board(harness: &mut Harness<'static, UnluminousApp>) {
        did(harness, "plugins run agent-tasks new-sprint Current Sprint");
        did(
            harness,
            "plugins run agent-tasks new-task Unluminous \u{2014} Plugin architecture for UI",
        );
        did(harness, "plugins run agent-tasks new-task Rust vector db");
        did(harness, "plugins run agent-tasks move-task task-2 in_progress 0");
        did(harness, "plugins run agent-tasks board");
        did(harness, "plugins pane agent-tasks/board --show");
        steady(harness);
    }
    let mut one = harness("");
    board(&mut one);
    let first = one.render().expect("render the board");
    let mut two = harness("");
    board(&mut two);
    let second = two.render().expect("render the board in a second window");
    assert_eq!(first.dimensions(), second.dimensions());
    assert!(
        first.as_raw() == second.as_raw(),
        "the same board drew two different pictures, so no accepted image of it means anything"
    );
}

/// `task-1765`: the header keeps its one action at the width the rail is admitted at.
///
/// **The width is reached with the font, not with the explorer**, because the explorer is clamped at 620
/// points and cannot squeeze the editing area far enough on this window: an earlier version of this test
/// asked for 800, got 620, and left the board 523 points wide against a threshold of 342 — so it was not
/// at the boundary at all, which is what the third review caught. Every measurement in the threshold
/// scales with the editor's font except the shadow's own reach, so 32 point text raises it to about 418
/// and the board is then within a hundred points of it.
///
/// At that width there is no room for the heading, the count, the search box and the button. Something
/// has to give and it must not be the button: a board somebody cannot add a ticket to is a broken board,
/// where a heading that is cut short is a heading that is cut short.
#[test]
fn the_board_keeps_add_task_at_the_width_the_rail_appears_at() {
    // A board of its own, for the reason `a_window_with_its_own_board` gives: this one adds a sprint with a
    // deliberately long name, and adding it to somebody's real board is both a change to their file and a
    // test that fails the second time it is run.
    let mut harness =
        a_window_with_its_own_board("the_board_keeps_add_task_at_the_width_the_rail_appears_at");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    // **Docked to a side and made narrow**, which is the only way to give this pane a small width: the
    // manifest docks it to the **bottom**, so it is as wide as the window whatever the explorer does, and
    // `panel size explorer --width 800` — which this test used to rely on — left it 1144 points wide.
    did(&mut harness, "plugins pane agent-tasks/board --side right");
    harness.state_mut().set_plugin_pane_width_for("agent-tasks/board", 360.0);
    did(&mut harness, "settings set appearance.font.size 32");
    did(
        &mut harness,
        "plugins run agent-tasks new-sprint A sprint with a long enough name to crowd the row",
    );
    did(
        &mut harness,
        "plugins run agent-tasks new-task Unluminous \u{2014} Plugin architecture for UI",
    );
    did(&mut harness, "plugins run agent-tasks board");
    steady(&mut harness);
    // **`+ Add Task` survived, which is the whole of the point.** At this width there is no room for the
    // heading, the count, the search box and the button, and the button is the one that must not give way:
    // a board somebody cannot add a ticket to is a broken board, where a heading that is cut short is a
    // heading that is cut short.
    //
    // This used to assert the board's own rail was drawn beside it, as evidence that the width really was
    // past the threshold. It is not: measured, at a 32 point font the rail is absent at **every** explorer
    // width, because `components::agent_tasks` withdraws it when the pane is shorter than the buttons need
    // as well as when it is narrower — `area.height() < tall + PAD * 2.0` — and a 32 point font makes the
    // buttons taller than this pane. So the rail's absence was never evidence about the width. What is
    // evidence about the width is the pane's rectangle, which is asserted outright.
    let pane = harness.state().plugin_pane_area_for("agent-tasks/board").expect("the board's pane");
    assert!(
        pane.width() < 400.0,
        "the pane was asked for at 360 points wide and is {}",
        pane.width()
    );
    harness.get_by_label("+ Add Task");
    harness.snapshot(shot("agent_tasks_narrow_header").as_str());
}

/// A window whose Agent-Tasks board is a file of its own, rather than the person's real one.
///
/// **This is `CLAUDE.md`'s rule, and these tests were breaking it.** "Tests must not read or write the
/// settings of the person running them." A window a test builds has no store, and `AgentTasks` answers
/// that with a board **in memory** — which is exactly the rule stated at the top of its own
/// `set_context`. But the moment `plugins pane … --show` builds the provider against a window that has
/// a store, the board is the file that store points at, and with no store named that is
/// `~/Library/Application Support/Unluminous/plugins/agent-tasks/board.sqlite3`: somebody's real board.
/// Measured: `plugins view agent-tasks` in these tests reported that path, and the assertions then failed
/// against the sprints and tickets already in it.
///
/// `use_store` is what answers it, and it needs no new mechanism: it already points the plugin's own
/// folder at the store's, which is what `the_pane_is_moved_and_put_away_from_the_command_line` does.
fn a_window_with_its_own_board(name: &str) -> Harness<'static, UnluminousApp> {
    let folder = std::env::temp_dir().join("unluminous-boards").join(name);
    std::fs::remove_dir_all(&folder).ok();
    std::fs::create_dir_all(&folder).expect("a folder for this test's board");
    let mut harness = harness("");
    harness.state_mut().use_store(unluminous_app::services::store::Store::at(&folder));
    harness
}

/// A board with two sprints, a backlog and three epics, for the three listings.
///
/// **`name` is the caller's own**, because these run in parallel and a board is a file: three tests
/// sharing one folder is three tests writing one SQLite file, and the second to arrive was refused with
/// `UNIQUE constraint failed: task_epic.name` — which reads as a fault in the board rather than in the
/// fixture. It is the rule `git_folder(name)` already keeps for the same reason.
fn a_board_with_sprints(name: &str) -> Harness<'static, UnluminousApp> {
    let mut harness = a_window_with_its_own_board(name);
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-sprint August 2nd Half");
    did(&mut harness, "plugins run agent-tasks new-epic Unluminous");
    did(&mut harness, "plugins run agent-tasks epic-colour Unluminous #8B6BFF");
    did(&mut harness, "plugins run agent-tasks new-epic Rust-Db");
    did(&mut harness, "plugins run agent-tasks epic-colour Rust-Db #FFB648");
    for title in ["Upgrade and GPU stuck", "Unluminous agent chat plugin", "Rust db scoring system"]
    {
        did(&mut harness, &format!("plugins run agent-tasks new-task {title}"));
    }
    did(&mut harness, "plugins run agent-tasks back");
    did(&mut harness, "plugins run agent-tasks priority task-1 high");
    did(&mut harness, "plugins run agent-tasks move-task task-2 agent_done");
    did(&mut harness, "plugins run agent-tasks assign task-3 codex");
    // One in the backlog, and one sprint that is not the active one.
    did(&mut harness, "plugins run agent-tasks sprint-assign task-3 backlog");
    did(&mut harness, "plugins run agent-tasks new-sprint September");
    did(&mut harness, "plugins run agent-tasks sprint-activate August 2nd Half");
    steady(&mut harness);
    harness
}

/// `task-1771`: the Backlog view is what the page this board is modelled on has — sprints as groups, the
/// backlog last, rows rather than cards, and a ticket dragged from one group to another changes its sprint.
#[test]
fn the_backlog_groups_by_sprint_and_a_row_dragged_between_them_moves_the_ticket() {
    let mut harness = a_board_with_sprints(
        "the_backlog_groups_by_sprint_and_a_row_dragged_between_them_moves_the_ticket",
    );
    did(&mut harness, "plugins run agent-tasks view backlog");
    steady(&mut harness);
    harness.get_by_label("August 2nd Half");
    harness.get_by_label_contains("task-3");
    harness.snapshot(shot("agent_tasks_backlog").as_str());

    // **The move a drop makes, asked for the way the command line asks for it.** A row let go over a group
    // calls `AgentTasks::drop_the_carried_row`, which is `set_sprint_of` — the same change
    // `sprint-assign` makes, and the one place either path reaches.
    //
    // The **pointer** drag is not driven here, and that is a limit of the harness rather than a gap in the
    // feature: measured, `egui_kittest` reports `is_decidedly_dragging` for the frames of a synthesised
    // drag, and the row inside the plugin's pane still answers `dragged()` false, so `Pressed::carrying`
    // is never set and the drop has no ticket to move. `the_explorer_row_drag` and the tab drag work
    // because those widgets are the window's own, drawn straight into the frame's `Ui`. Driving a real
    // window is layer 4, which is where this half is checked.
    did(&mut harness, "plugins run agent-tasks sprint-assign task-3 August 2nd Half");
    steady(&mut harness);
    let read = did(&mut harness, "plugins run agent-tasks task task-3");
    assert_eq!(read["task"]["key"], "task-3");
    let listed = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(listed["view"], "backlog");
    // The board shows the active sprint, so a ticket that moved into it is on the board now.
    let on_the_board = listed["lanes"]
        .as_array()
        .expect("the lanes")
        .iter()
        .flat_map(|lane| lane["cards"].as_array().expect("its cards"))
        .any(|card| card["key"] == "task-3");
    assert!(on_the_board, "the ticket was dragged into the active sprint: {listed:#?}");
}

/// The Completed view is the same shape with the finished sprints, and each of them folds.
#[test]
fn the_completed_view_groups_finished_sprints_and_each_one_folds() {
    let mut harness =
        a_board_with_sprints("the_completed_view_groups_finished_sprints_and_each_one_folds");
    did(&mut harness, "plugins run agent-tasks sprint-complete August 2nd Half");
    did(&mut harness, "plugins run agent-tasks view completed");
    steady(&mut harness);
    harness.get_by_label("August 2nd Half");
    harness.snapshot(shot("agent_tasks_completed").as_str());

    harness.get_by_label("Fold August 2nd Half").click();
    steady(&mut harness);
    harness.snapshot(shot("agent_tasks_completed_folded").as_str());
}

/// The Epics view: a card an epic, with the seven colours, a rename and a delete that asks first.
#[test]
fn the_epics_view_is_a_grid_of_cards_that_can_be_renamed_recoloured_and_deleted() {
    let mut harness = a_board_with_sprints(
        "the_epics_view_is_a_grid_of_cards_that_can_be_renamed_recoloured_and_deleted",
    );
    did(&mut harness, "plugins run agent-tasks view epics");
    steady(&mut harness);
    // **The card is asked for by one of its controls, not by the epic's bare name.** No control on an
    // epic card is named the epic alone — they are `Rename <name>`, `Delete <name>` and
    // `<name> colour <hex>` — so a bare `Unluminous` never named anything here. It passed until
    // `task-1808` renamed this epic from `Quill`, at which point the only node answering to `Unluminous`
    // was the application menu in the title bar, and the test had been asserting on that instead.
    harness.get_by_label("Rename Unluminous");
    harness.snapshot(shot("agent_tasks_epics").as_str());

    // Recolouring is one press, and it is the same command the command line runs.
    harness.get_by_label("Unluminous colour #2FCFA6").click();
    steady(&mut harness);
    let epics = did(&mut harness, "plugins view agent-tasks")["epics"].clone();
    let unluminous = epics
        .as_array()
        .expect("the epics")
        .iter()
        .find(|epic| epic["name"] == "Unluminous")
        .expect("the Unluminous epic")
        .clone();
    assert_eq!(unluminous["color"], "#2FCFA6");

    // Deleting asks first, and what it asks is a second press rather than a dialog: a plugin cannot open
    // the window's own confirmation.
    harness.get_by_label("Delete Unluminous").click();
    steady(&mut harness);
    harness.get_by_label("Really delete Unluminous");
    harness.get_by_label("Keep Unluminous").click();
    steady(&mut harness);
    assert_eq!(
        did(&mut harness, "plugins view agent-tasks")["epics"].as_array().expect("the epics").len(),
        2,
        "cancelling keeps it"
    );
}

/// Five things the `task-1771` review found about the board's new commands and views.
#[test]
fn the_reviews_findings_about_the_boards_commands() {
    let mut harness = a_board_with_sprints("the_reviews_findings_about_the_boards_commands");

    // **An id names a sprint or an epic outright**, which is what the board's own buttons pass. Renaming
    // from the Epics view could not work at all before this: the card sends the epic's id and the command
    // walked the arguments looking for a *name*.
    let epics = did(&mut harness, "plugins view agent-tasks")["epics"].clone();
    assert!(epics.as_array().expect("the epics").iter().any(|epic| epic["name"] == "Unluminous"));
    did(&mut harness, "plugins run agent-tasks view epics");
    steady(&mut harness);
    harness.get_by_label("Rename Unluminous").click();
    steady(&mut harness);
    harness.get_by_label("Epic name").click();
    steady(&mut harness);
    harness.input_mut().events.push(egui::Event::Text(" IDE".to_owned()));
    steady(&mut harness);
    harness.get_by_label("Save Unluminous").click();
    steady(&mut harness);
    let epics = did(&mut harness, "plugins view agent-tasks")["epics"].clone();
    assert!(
        epics.as_array().expect("the epics").iter().any(|epic| epic["name"] == "Unluminous IDE"),
        "the card renamed it: {epics:#?}"
    );

    // **`general` is the one epic that stays**, and the rule is the command's rather than the card's: the
    // card draws no Delete on it, and renaming it first would otherwise have exposed one.
    did(&mut harness, "plugins run agent-tasks new-epic general");
    assert_eq!(refused(&mut harness, "plugins run agent-tasks epic-delete general"), "failed");

    // **A ticket that comes back from a completed sprint goes to the foot of the backlog**, not on top of
    // whatever was already at position 0.
    did(&mut harness, "plugins run agent-tasks new-task Left unfinished");
    did(&mut harness, "plugins run agent-tasks back");
    did(&mut harness, "plugins run agent-tasks sprint-complete August 2nd Half");
    did(&mut harness, "plugins run agent-tasks view backlog");
    steady(&mut harness);
    let listed = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(listed["view"], "backlog");
    // Whatever the backlog holds, no two of its New tickets share a place.
    let store = did(&mut harness, "plugins run agent-tasks task task-3");
    assert_eq!(store["task"]["key"], "task-3");

    // **`panel zoom` keeps the range it documents** rather than clamping silently.
    assert_eq!(refused(&mut harness, "panel zoom explorer 100"), "usage");
    assert_eq!(refused(&mut harness, "panel zoom explorer 0.1"), "usage");
    did(&mut harness, "panel zoom explorer 2.0");
}

/// Everything a person can do to a sprint or an epic, an agent can do too.
#[test]
fn the_sprints_and_the_epics_are_driven_entirely_from_the_command_line() {
    let mut harness =
        a_board_with_sprints("the_sprints_and_the_epics_are_driven_entirely_from_the_command_line");
    // A sprint is named by its name, which is what is on the screen.
    did(&mut harness, "plugins run agent-tasks sprint-rename September October");
    did(&mut harness, "plugins run agent-tasks sprint-activate October");
    let view = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(view["sprint"]["name"], "October");
    assert_eq!(view["sprint"]["status"], "active");

    // Completing a sprint puts whatever is not in Agent Done back in the backlog, and says how many.
    did(&mut harness, "plugins run agent-tasks sprint-activate August 2nd Half");
    let said = run(&mut harness, "plugins run agent-tasks sprint-complete August 2nd Half");
    assert!(said.message.contains("completed"), "{}", said.message);

    // An epic renamed, recoloured and deleted.
    did(&mut harness, "plugins run agent-tasks epic-rename Rust-Db Rust");
    did(&mut harness, "plugins run agent-tasks epic-color Rust #FF4F7A");
    did(&mut harness, "plugins run agent-tasks epic-delete Rust");
    let epics = did(&mut harness, "plugins view agent-tasks")["epics"].clone();
    assert!(
        !epics.as_array().expect("the epics").iter().any(|epic| epic["name"] == "Rust"),
        "the epic is gone: {epics:#?}"
    );
    // And the refusals name what there is, which is what a caller who guessed wrong needs.
    assert_eq!(refused(&mut harness, "plugins run agent-tasks sprint-activate Nonesuch"), "failed");
    assert_eq!(
        refused(&mut harness, "plugins run agent-tasks epic-colour Unluminous sideways"),
        "failed"
    );
}

/// `task-1771`: *"when an agent finishes a task and moves it to agent done, for some reason a side panel is
/// opening up that shows the task. I'm not sure what that is, but I don't want it."*
///
/// It was not the finishing. `task <key>` is what an agent runs to read a ticket, over and over while it
/// works, and it used to **open** the ticket as well — which split the board in two under whoever was
/// looking at it. Reading and showing are two commands now, and only one of them changes the window.
#[test]
fn reading_a_ticket_answers_with_it_and_leaves_the_board_showing_the_lanes() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-sprint Current Sprint");
    did(&mut harness, "plugins run agent-tasks new-task Plugin architecture for UI");
    did(&mut harness, "plugins run agent-tasks todo-add task-1 Weigh the four mechanisms");
    did(&mut harness, "plugins run agent-tasks todo-add task-1 Write the manifest keys");
    did(&mut harness, "plugins run agent-tasks todo-done task-1 1");
    did(
        &mut harness,
        "plugins run agent-tasks comment task-1 Zed and Lapce both have no UI surface.",
    );
    // `+ Add Task` opens what it made, so that its six fields can be filled in; put it away again, because
    // what is being measured here is what **reading** does.
    did(&mut harness, "plugins run agent-tasks back");
    steady(&mut harness);

    // Everything a ticket holds comes back, which is the whole point of the command.
    let read = did(&mut harness, "plugins run agent-tasks task task-1");
    assert_eq!(read["task"]["key"], "task-1");
    assert_eq!(read["todos"].as_array().expect("its todos").len(), 2);
    assert_eq!(read["comments"].as_array().expect("its comments").len(), 1);
    steady(&mut harness);
    // And the board is still the board.
    let view = did(&mut harness, "plugins view agent-tasks");
    assert!(view["detail"].is_null(), "reading a ticket must not open it: {view:#?}");
    assert_eq!(view["modal"], false);

    // Showing one is its own command, and that is the modal.
    did(&mut harness, "plugins run agent-tasks open task-1");
    steady(&mut harness);
    let view = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(view["modal"], true, "`open` is what puts a ticket in front of somebody");
    assert_eq!(view["detail"]["task"]["key"], "task-1");
}

#[test]
fn the_plugins_own_menu_with_its_submenu_open() {
    let mut harness = harness("");
    harness.state_mut().menu_placement = MenuPlacement::InWindow;
    steady(&mut harness);
    // **One `Plugins` menu, with a submenu per plugin.** `task-1848`: "Plugins menu items at the top
    // should be moved to a Plugins menu item, which lists each plugin, and has sub menus for their
    // options." Before that each plugin took a menu of its own in the bar, so this used to open
    // `Agent-Tasks` directly — the bar's width was then decided by how many plugins were switched on.
    harness
        .get_all_by_label(unluminous_app::app::actions::PLUGINS_MENU)
        .next()
        .expect("the Plugins menu in the bar")
        .click();
    steady(&mut harness);
    // Every plugin's heading is drawn inline under it, which is what a submenu is here.
    for plugin in ["Agent-Chat", "Agent-Tasks", "Database"] {
        harness
            .get_all_by_label(plugin)
            .next()
            .unwrap_or_else(|| panic!("{plugin} is a submenu under Plugins"));
    }
    harness.snapshot(shot("agent_tasks_menu").as_str());
}

#[test]
fn where_a_contributed_pane_was_left_is_remembered_against_its_own_name() {
    use unluminous_app::app::dock::{Panel, Side};
    use unluminous_app::settings;
    // A pane's side and its two measurements are recorded against `<plugin id>/<pane id>`, so a second
    // plugin installed later cannot be handed the first one's width. This is that round trip through a
    // real settings file rather than through the value in memory.
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-plugin-pane-layout");
    let store = unluminous_app::services::store::Store::at(folder.join(".unluminous-settings"));
    let mut panes = settings::Panes::new();
    panes.dock.set_plugin_panes(&[Side::Right], &[false]);
    panes.dock.dock(Panel::Plugin(0), Side::Bottom, None);
    panes.set_height_of(Panel::Plugin(0), 333.0);
    let keys = vec!["agent-tasks/board".to_owned()];
    settings::save_with(&store, &settings::Settings::new(), &panes, &keys);
    let (_, read) = settings::load_with(&store, &keys);
    assert_eq!(read.dock.side_of(Panel::Plugin(0)), Side::Bottom, "where it was dragged to");
    assert_eq!(read.height_of(Panel::Plugin(0)), 333.0, "and how tall it was left");
    // Read back with nothing named, which is what the first read of a window does: the pane keeps its
    // manifest's side, because nothing recorded yet is not the same as recorded as the default.
    let (_, unnamed) = settings::load_with(&store, &[]);
    assert_eq!(unnamed.dock.side_of(Panel::Plugin(0)), Side::Right);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_window_gives_the_plugins_a_turn_on_the_clock() {
    // The watchdog runs from the frame rather than from a thread, and `plugins run agent-tasks tick` is
    // the same path asked for by hand. This drives the path an agent uses and reads the board back.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task Something to work");
    // A card in progress with no session is not the watchdog's business, because nobody launched it: it is
    // there because a person put it there.
    did(&mut harness, "plugins run agent-tasks move-task task-1 in_progress 0");
    let ticked = did(&mut harness, "plugins run agent-tasks tick");
    assert_eq!(
        ticked["acted"].as_array().expect("what it acted on").len(),
        0,
        "a card with no recorded session has no worker to nudge or reclaim"
    );
    let board = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(
        board["lanes"].as_array().expect("the lanes")[2]["count"],
        1,
        "and it was left alone"
    );
}

#[test]
fn a_pane_that_asks_for_a_project_is_absent_when_there_is_none() {
    // Unluminous's rule everywhere: a control that cannot apply is absent rather than dimmed. Agent-Tasks asks for
    // `always`, so the other condition is driven through a manifest of its own — installing one is what a
    // person does, and it is the only way to exercise a value no shipped plugin asks for.
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-plugin-in-project");
    let settings = folder.join(".unluminous-settings");
    let plugin = settings.join("plugins").join("needs-a-project");
    std::fs::create_dir_all(&plugin).expect("a plugin folder");
    std::fs::write(
        plugin.join("plugin.conf"),
        "plugin.id = needs-a-project\nplugin.name = Needs A Project\nplugin.kind = ui\n\
         ui.provider = agent-tasks\npane.id = board\npane.applies = in_project\n",
    )
    .expect("a manifest");
    let (plugins, problems) = unluminous_app::services::plugins::Plugins::load(Some(
        &unluminous_app::services::store::Store::at(&settings),
    ));
    assert!(problems.is_empty(), "the manifest should parse: {problems:?}");
    let mut ui = unluminous_app::app::plugin_panes::PluginUi::default();
    ui.refresh(&plugins);
    let slot = ui.slot_of("needs-a-project/board").expect("the contributed pane");
    ui.set_project(Some(folder.clone()));
    assert!(ui.applies(slot), "with a project open, `in_project` applies");
    ui.set_project(None);
    assert!(!ui.applies(slot), "with none, it does not, so its rail button is absent");
    assert!(
        ui.set_visible(slot, true).is_some_and(|said| said.contains("project")),
        "and showing it is refused with what it asked for"
    );
    // `always` is the other value, and applies with no project at all. Agent-Tasks used to be what
    // exercised it, until `task-28` took its pane out; a second manifest of its own is what
    // `the_pane_is_moved_and_put_away_from_the_command_line`'s own comment asks for, so this value
    // is not left to whichever plugin happens to ask for it.
    let always_plugin = settings.join("plugins").join("always-applies");
    std::fs::create_dir_all(&always_plugin).expect("a plugin folder");
    std::fs::write(
        always_plugin.join("plugin.conf"),
        "plugin.id = always-applies\nplugin.name = Always Applies\nplugin.kind = ui\n\
         ui.provider = agent-tasks\npane.id = board\npane.applies = always\n",
    )
    .expect("a manifest");
    let (plugins, problems) = unluminous_app::services::plugins::Plugins::load(Some(
        &unluminous_app::services::store::Store::at(&settings),
    ));
    assert!(problems.is_empty(), "the manifest should parse: {problems:?}");
    let mut ui = unluminous_app::app::plugin_panes::PluginUi::default();
    ui.refresh(&plugins);
    let board = ui.slot_of("always-applies/board").expect("the contributed pane");
    ui.set_project(None);
    assert!(ui.applies(board), "`always` applies with no project open");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_pane_that_is_showing_stays_the_pane_that_is_showing_when_the_plugins_change() {
    // Which slot a pane is in comes from the manifests and moves when a plugin is switched on or off, so
    // what is showing is held by the pane's own name. Keyed by slot, switching one plugin off would leave
    // another plugin's pane showing or hidden according to what the first one was doing.
    //
    // Agent-Tasks no longer contributes a pane (`task-28` took it out of its own manifest), so this test
    // drives one from a manifest written for it, the way `the_pane_is_moved_and_put_away_from_the_command_line`
    // does.
    let folder = copy_out_of_the_repository(&sample_folder(), "unluminous-plugin-pane-persists");
    let settings = folder.join(".unluminous-settings");
    let plugin = settings.join("plugins").join("agent-tasks");
    std::fs::create_dir_all(&plugin).expect("a plugin folder");
    std::fs::write(
        plugin.join("plugin.conf"),
        "plugin.id = agent-tasks\nplugin.name = Agent-Tasks\nplugin.kind = ui\n\
         ui.provider = agent-tasks\npane.id = board\npane.label = Agent-Tasks\npane.side = right\n\
         pane.width = 420\n",
    )
    .expect("a manifest that contributes a pane");
    let mut harness = harness_in(&folder);
    harness.state_mut().use_store(unluminous_app::services::store::Store::at(&settings));
    steady(&mut harness);
    did(&mut harness, "plugins pane agent-tasks/board --show");
    steady(&mut harness);
    assert!(showing(&harness, "agent-tasks/board"));
    // Switching another plugin off and on again is a `Surfaces` rebuild, which is the moment a slot could
    // move underneath the pane that is showing. There is a second contributed pane in the window now —
    // Agent-Chat's, since `task-1767` — so the slots really do move, which is what this is about.
    did(&mut harness, "plugins disable rust");
    steady(&mut harness);
    assert!(showing(&harness, "agent-tasks/board"), "the board is still showing");
    assert!(!showing(&harness, "agent-chat/chat"), "and the pane nobody opened is still not");
    did(&mut harness, "plugins enable rust");
    steady(&mut harness);
    assert!(showing(&harness, "agent-tasks/board"));
    // And switching the board itself off stops it showing rather than leaving a stale flag behind for
    // whatever pane lands in that slot next.
    did(&mut harness, "plugins disable agent-tasks");
    steady(&mut harness);
    assert!(!showing(&harness, "agent-tasks/board"));
    did(&mut harness, "plugins enable agent-tasks");
    steady(&mut harness);
    assert!(
        !showing(&harness, "agent-tasks/board"),
        "a plugin switched back on does not bring its pane back showing: nobody asked for it"
    );
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn reloading_the_plugins_withdraws_a_pane_whose_contribution_has_gone() {
    // A manifest edited by hand can take a pane away, and a pane whose plugin no longer offers it would
    // draw nothing and could not be told what it was. So a reload withdraws it, for the reason switching a
    // plugin off does. `task-1848` moved the board from a tab to a pane, so a pane is what this drives.
    let mut harness = harness("Some prose.");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    steady(&mut harness);
    let slot = harness.state().plugin_ui.slot_of("agent-tasks/board").expect("its slot");
    assert!(harness.state().plugin_ui.is_visible(slot));
    // Switching the plugin off is the same rebuild a reload does, and it is the one a test can drive
    // without writing a manifest into the person's own settings folder.
    did(&mut harness, "plugins disable agent-tasks");
    steady(&mut harness);
    assert!(
        harness.state().plugin_ui.slot_of("agent-tasks/board").is_none(),
        "the board's pane went with the plugin"
    );
    assert_eq!(refused(&mut harness, "plugins pane agent-tasks/board --show"), "not-found");
}

#[test]
fn a_command_with_its_arguments_missing_is_refused_rather_than_taking_the_window_down() {
    // A command line that panics is worse than one that says what it needed, and an agent gets a command's
    // arguments wrong more often than a person does. Every command that reads the rest of the line is
    // driven here with nothing after its verb.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task Something");
    for line in [
        "plugins run agent-tasks todo-add",
        "plugins run agent-tasks todo-add task-1",
        "plugins run agent-tasks comment",
        "plugins run agent-tasks comment task-1",
        "plugins run agent-tasks comment-send task-1",
        "plugins run agent-tasks send task-1",
        "plugins run agent-tasks task",
        "plugins run agent-tasks move-task",
        "plugins run agent-tasks move-task task-1",
        "plugins run agent-tasks priority task-1",
        "plugins run agent-tasks assign task-1",
        "plugins run agent-tasks todo-done task-1",
        "plugins run agent-tasks new-epic",
        "plugins run agent-tasks new-sprint",
        "plugins run agent-tasks view",
        "plugins run agent-tasks start",
        "plugins run agent-tasks resume",
        "plugins run agent-tasks stop",
        "plugins run agent-tasks interrupt",
        "plugins run agent-tasks heartbeat",
    ] {
        let reply = run(&mut harness, line);
        // Either it was refused with a sentence, or it did something harmless. What must not happen is a
        // panic, and reaching this assertion at all is what proves there was none.
        assert!(reply.ok || !reply.message.is_empty(), "`{line}` was refused with nothing to read");
    }
    // And the ticket is still there afterwards, which is what proves nothing was left half done. By its key
    // rather than by the board's count: `new-sprint` with no name names itself and succeeds, which makes an empty
    // sprint the active one, so the count is a count of the wrong thing.
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    assert_eq!(ticket["task"]["key"], "task-1");
}

#[test]
fn resume_session_is_refused_for_a_codex_ticket_and_says_what_to_press_instead() {
    // The two agents differ and the difference is not a preference: Claude takes the session id it is
    // given, and Codex names its own. Starting a fresh Codex agent and calling it a resumed one would be
    // the one outcome every check on this board exists to prevent.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task Something for Codex");
    did(&mut harness, "plugins run agent-tasks assign task-1 codex");
    let reply = run(&mut harness, "plugins run agent-tasks resume task-1");
    assert!(!reply.ok, "a Codex ticket cannot be resumed");
    assert!(reply.message.contains("names its own sessions"), "{}", reply.message);
    assert!(reply.message.contains("Start"), "it says what to press instead: {}", reply.message);
    // And a Claude ticket that has never had a session says that instead, which is a different miss.
    did(&mut harness, "plugins run agent-tasks assign task-1 claude");
    let reply = run(&mut harness, "plugins run agent-tasks resume task-1");
    assert!(!reply.ok);
    assert!(reply.message.contains("never had a session"), "{}", reply.message);
}

#[test]
fn a_ticket_cannot_be_started_twice_and_a_failed_start_gives_the_claim_back() {
    // Two agents on one ticket is the worst thing this board can do, so **only an unclaimed ticket can be
    // claimed** and a claim whose agent could not be spawned is given back.
    //
    // Driven with `step` rather than `run`, because starting an agent on a machine that has one leaves a
    // terminal printing and a printing terminal asks for frames: `Harness::run` gives four steps to settle and
    // panics otherwise, which is right for a settled window and wrong here. That is the rule `task-1654` wrote
    // down about waiting loops, wearing a different hat.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task Something to work");
    did(&mut harness, "plugins run agent-tasks board");
    // Asked without letting the window settle, because a started agent prints and a printing terminal asks for
    // frames: `run` would give up after four steps. `did_while_waiting` is the helper the debugger's own tests
    // use for exactly this.
    let ctx = harness.ctx.clone();
    let reply = harness
        .state_mut()
        .run_command_line("plugins run agent-tasks start task-1", &ctx)
        .expect("start was answered on the frame it was asked");
    harness.step();
    let board = did_while_waiting(&mut harness, "plugins view agent-tasks");
    let lanes = board["lanes"].as_array().expect("the lanes").clone();
    if reply.ok {
        // The machine has an agent to launch, so it started. A second Start is then refused rather than
        // launching a second agent on the same conversation.
        assert_eq!(lanes[2]["count"], 1, "it is in progress");
        let again = harness
            .state_mut()
            .run_command_line("plugins run agent-tasks start task-1", &ctx)
            .expect("the second start was answered");
        harness.step();
        assert!(!again.ok, "a ticket that already has a session cannot be started again");
        assert!(
            again.message.contains("already") || again.message.contains("Resume session"),
            "{}",
            again.message
        );
        did_while_waiting(&mut harness, "plugins run agent-tasks stop task-1");
    } else {
        // It could not start, so the claim was given back and the ticket is where it was. The store's own tests
        // drive the release directly; this is the window doing it.
        let _ = &ctx;
        assert_eq!(lanes[0]["count"], 1, "the ticket is still in New: {}", reply.message);
        assert_eq!(lanes[2]["count"], 0);
        assert_eq!(
            lanes[0]["cards"].as_array().expect("the cards")[0]["session"],
            serde_json::Value::Null,
            "and it names no session"
        );
    }
}

#[test]
fn sending_a_ticket_back_from_agent_done_to_qa_failed_asks_its_agent_to_come_back() {
    // The board being replaced resumes the session on this transition, because a person rejecting finished work
    // wants to tell the agent why. A resume that cannot happen says so in the status bar rather than failing
    // the move: the move is what was asked for.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task Finished work");
    did(&mut harness, "plugins run agent-tasks move-task task-1 agent_done 0");
    did(&mut harness, "plugins run agent-tasks move-task task-1 qa_failed 0");
    steady(&mut harness);
    let board = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(board["lanes"].as_array().expect("the lanes")[1]["count"], 1, "it moved");
    // It has never had a session, so the attempt to hand one back says exactly that.
    let said = board["message"].as_str().unwrap_or_default();
    assert!(
        said.contains("never had a session"),
        "the move says why the session could not come back: `{said}`"
    );
}

/// Which agent a new ticket goes to, read from the provider rather than from a command.
///
/// The same field the Settings page writes and the chooser under the New lane cycles, so a test that reads it
/// here is reading what both of them changed.
fn chosen_agent(harness: &mut Harness<'static, UnluminousApp>) -> String {
    let provider =
        harness.state_mut().plugin_ui.provider("agent-tasks").expect("the board is open");
    let tasks = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
        .expect("the Agent-Tasks provider");
    tasks.configuration().agent.name().to_owned()
}

#[test]
fn the_arrow_keys_move_a_ring_round_the_board_and_enter_opens_what_it_is_on() {
    // The board can be driven from the keyboard, which the first pass could not do at all: every card had to be
    // clicked. The keys are read only while the window says this plugin holds them, so the test clicks the board
    // first, which is what gives it the keyboard.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task First in New");
    did(&mut harness, "plugins run agent-tasks new-task Second in New");
    did(&mut harness, "plugins run agent-tasks new-task Waiting on review");
    did(&mut harness, "plugins run agent-tasks move-task task-3 agent_done 0");
    did(&mut harness, "plugins run agent-tasks back");
    steady(&mut harness);
    // Nothing is ringed until a key is pressed, so a board nobody has touched draws no ring.
    assert_eq!(chosen_card(&mut harness), None, "no ring before a key is pressed");
    harness.get_by_label("Board background").click();
    steady(&mut harness);
    harness.key_press(egui::Key::ArrowDown);
    steady(&mut harness);
    assert_eq!(
        chosen_card(&mut harness),
        Some(("new".to_owned(), 0)),
        "the first press lands on the first card"
    );
    harness.key_press(egui::Key::ArrowDown);
    steady(&mut harness);
    assert_eq!(
        chosen_card(&mut harness),
        Some(("new".to_owned(), 1)),
        "and down moves down the lane"
    );
    // Past the last card stops at the last card rather than wrapping to the top.
    harness.key_press(egui::Key::ArrowDown);
    steady(&mut harness);
    assert_eq!(chosen_card(&mut harness), Some(("new".to_owned(), 1)), "and stops at the last one");
    // Right steps over the two empty lanes rather than stopping on `QA FAILED 0`.
    harness.key_press(egui::Key::ArrowRight);
    steady(&mut harness);
    assert_eq!(
        chosen_card(&mut harness),
        Some(("agent_done".to_owned(), 0)),
        "right steps over the lanes that hold nothing"
    );
    // Enter opens the ticket the ring is on.
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    let provider =
        harness.state_mut().plugin_ui.provider("agent-tasks").expect("the board is open");
    let tasks = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
        .expect("the Agent-Tasks provider");
    assert_eq!(
        tasks.detail().task.as_ref().map(|task| task.key.clone()),
        Some("task-3".to_owned()),
        "Enter opened the ticket the ring was on"
    );
}

/// Which card the keyboard's ring is on: the lane's name and how far down it.
fn chosen_card(harness: &mut Harness<'static, UnluminousApp>) -> Option<(String, usize)> {
    let provider =
        harness.state_mut().plugin_ui.provider("agent-tasks").expect("the board is open");
    let tasks = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
        .expect("the Agent-Tasks provider");
    tasks.chosen.map(|(lane, row)| (lane.name().to_owned(), row))
}

#[test]
fn a_ticket_can_name_its_jira_issue_and_copy_the_link_to_it() {
    // The JIRA panel on the ticket. Nothing here talks to JIRA and nothing here pretends to: the key is recorded
    // and the link is copied, and both say so.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task A ticket that came from a JIRA issue");
    did(&mut harness, "plugins run agent-tasks jira-key task-1 ENX-1932");
    did(&mut harness, "plugins run agent-tasks open task-1");
    steady(&mut harness);
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    assert_eq!(ticket["task"]["jira"], "ENX-1932", "the ticket names its issue");
    // With no `jira_url` on the row, the key itself is what Copy hands over: there is no configured JIRA site to
    // build an address against, and a guessed one would open nothing.
    harness.get_by_label("Copy issue link").click();
    // Read across frames rather than from the last one. The modal is drawn after the point in the frame where a
    // plugin's copy is handed to egui, so the request the button makes is acted on one frame later, and only that
    // frame's output carries it.
    let mut copied = String::new();
    for _ in 0..4 {
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
    assert_eq!(copied, "ENX-1932", "the key is what was copied");
    // Cleared by emptying the field, which is what the command does with nothing after it.
    did(&mut harness, "plugins run agent-tasks jira-key task-1");
    steady(&mut harness);
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    assert_eq!(ticket["task"]["jira"], serde_json::Value::Null, "and it can be cleared");
}

#[test]
fn a_person_can_change_their_own_comment_and_cannot_change_an_agents() {
    // `Edit` on a comment, which the browser board has and the first pass did not. The refusal is in the store
    // rather than only in the button: what an agent said is a record of what it said, so the command line cannot
    // rewrite one either.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task A ticket with comments on it");
    did(&mut harness, "plugins run agent-tasks comment task-1 The forma changed in April.");
    // The comments and their buttons are in the modal, so the ticket is opened in it.
    did(&mut harness, "plugins run agent-tasks open task-1");
    steady(&mut harness);
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    let comments = ticket["comments"].as_array().expect("the comments");
    let id = comments[0]["id"].as_i64().expect("the comment's id");
    // Through the buttons, which is what a person presses: `Edit`, type, `Save`. Every comment's
    // button says `Edit` on its face, so its accessible name carries who wrote it and when, the
    // way `choice_button_named` names it, to tell one ticket's several `Edit` buttons apart.
    harness.get_by_label("Edit the comment by human just now").click();
    steady(&mut harness);
    {
        let provider =
            harness.state_mut().plugin_ui.provider("agent-tasks").expect("the board is open");
        let tasks = provider
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
            .expect("the Agent-Tasks provider");
        assert_eq!(tasks.detail().editing_comment, Some(id), "`Edit` opens that comment");
        assert_eq!(
            tasks.detail().comment_edit,
            "The forma changed in April.",
            "and the draft starts as what the comment says"
        );
        tasks.detail_mut().comment_edit = "The format changed in April.".to_owned();
    }
    harness.get_by_label("Save").click();
    steady(&mut harness);
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    let comments = ticket["comments"].as_array().expect("the comments");
    assert_eq!(comments[0]["body"], "The format changed in April.", "the comment was changed");
    assert_eq!(comments.len(), 1, "and changed rather than added to");
    // That an agent's comment cannot be changed is the store's rule, tested where the rule lives:
    // `store::tests::an_agents_comment_is_a_record_and_cannot_be_edited`.
}

#[test]
fn the_new_lane_chooses_an_agent_and_starts_the_next_ticket_with_it() {
    // The quick launch under the New lane's heading: the chooser names which agent, and the play button next to
    // it starts the ticket at the top of New without opening it. Both are pressed here by the labels they carry,
    // which is what a person does with them.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task First in the lane");
    // Creating a ticket opens it, so the pane is showing that ticket rather than the lanes. `back` returns to
    // the lanes, which is where the quick launch is. It also shuts the modal, and that matters: the modal is an
    // `egui::Area` in the foreground layer, so while it is open nothing on the board behind it can be pressed.
    did(&mut harness, "plugins run agent-tasks back");
    steady(&mut harness);
    // The chooser names the agent a new ticket goes to, which starts as `claude`.
    harness.get_by_label("Agent for a new ticket: claude").click();
    steady(&mut harness);
    assert_eq!(chosen_agent(&mut harness), "codex", "pressing the chooser names the other agent");
    // And it is the same setting the Settings page writes, so the two cannot disagree.
    harness.get_by_label("Agent for a new ticket: codex").click();
    steady(&mut harness);
    assert_eq!(chosen_agent(&mut harness), "claude", "and pressing it again comes back round");
    // The play button starts the ticket at the top of New. No agent is on this machine under test, so what it
    // proves is that the button reaches `start` for that ticket and reports what happened.
    harness.get_by_label("Start the next ticket with claude").click();
    // `nudge`, not `run`: starting a ticket launches a terminal, and a live terminal asks for a frame whenever
    // it prints, so a `run` that insists the window goes quiet fails for a reason that is not a fault.
    nudge(&mut harness);
    // `did_while_waiting`, for the same reason: the command goes down the same path but steps the window rather
    // than insisting it settles.
    let board = did_while_waiting(&mut harness, "plugins view agent-tasks");
    let said = board["message"].as_str().unwrap_or_default();
    assert!(!said.is_empty(), "the play button says what happened: `{said}`");
}

#[test]
fn the_detail_can_name_a_ticket_add_a_todo_and_post_a_comment() {
    // `+ Add task` creates an untitled row, so the detail has to be able to name it, or the board can make a
    // ticket it cannot label. All three go through the same functions the buttons call.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task");
    steady(&mut harness);
    let board = harness.state_mut();
    let provider = board.plugin_ui.provider("agent-tasks").expect("the board is open");
    // Reaching the provider by its own type is what a button in the pane does; the command line path is
    // covered elsewhere.
    let tasks = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
        .expect("the Agent-Tasks provider");
    assert_eq!(tasks.detail().task.as_ref().expect("the new ticket").title, "");
    tasks.detail_mut().title_draft = "Named after the fact".to_owned();
    tasks.save_the_title().expect("the title saved");
    tasks.detail_mut().todo_draft = "Read the old importer".to_owned();
    tasks.post_the_todo().expect("the todo posted");
    tasks.detail_mut().draft = "The format changed in April.".to_owned();
    tasks.post_the_comment(false).expect("the comment posted");
    steady(&mut harness);
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    assert_eq!(ticket["task"]["title"], "Named after the fact");
    assert_eq!(ticket["task"]["todos"], "0/1");
    assert_eq!(ticket["todos"].as_array().expect("the todos")[0]["text"], "Read the old importer");
    assert_eq!(ticket["comments"].as_array().expect("the comments").len(), 1);
    assert_eq!(ticket["comments"].as_array().expect("the comments")[0]["author"], "human");
}

/// `task-28`: "I'm unable to get an agent to do work because the model is a text field."
///
/// Choosing in each dropdown writes the column it names. One test per field would be seven tests over one
/// function; what makes this one test rather than seven is that each field is checked on the row afterwards, so
/// a field that wrote nothing, or wrote somebody else's column, fails on its own assertion.
/// `task-28`: "The description, comments, etc should have icons to view as raw, or as markdown."
///
/// Both views, on both things, from the buttons a person presses and from the command an agent runs. The premise
/// the ticket carried was that this depends on a markdown plugin; there is none, and markdown is built into
/// `unluminous-core`, so the buttons are always there.
#[test]
fn a_description_and_a_comment_are_read_as_markdown_or_as_their_source() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-sprint Current Sprint");
    did(&mut harness, "plugins run agent-tasks new-task Read me either way");
    steady(&mut harness);
    let board = harness.state_mut();
    let tasks = board
        .plugin_ui
        .provider("agent-tasks")
        .and_then(|provider| provider.as_any_mut())
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
        .expect("the Agent-Tasks provider");
    tasks
        .save_the_description("# A heading\n\nSome prose, and `code`.\n")
        .expect("the description");
    steady(&mut harness);
    did(&mut harness, "plugins run agent-tasks comment task-1 ## From a person\n\n- one\n- two");
    steady(&mut harness);

    // The description starts as its source, because that is the field somebody writes in.
    let view = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(view["showing"]["description"], "raw", "a description opens open for writing");
    // A comment starts rendered, because a comment is read far more often than it is written.
    assert!(
        view["showing"]["comments_as_source"].as_array().expect("the list").is_empty(),
        "no comment starts as its source"
    );

    // The button a person presses, named for what it does.
    harness.get_by_label("Read the description as markdown").click();
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins view agent-tasks")["showing"]["description"], "markdown");
    harness.get_by_label("Read the description as its source").click();
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins view agent-tasks")["showing"]["description"], "raw");

    // And the same change from the command line, which is the agent's way to it.
    did(&mut harness, "plugins run agent-tasks show description markdown");
    assert_eq!(did(&mut harness, "plugins view agent-tasks")["showing"]["description"], "markdown");
    did(&mut harness, "plugins run agent-tasks show description raw");
    assert_eq!(did(&mut harness, "plugins view agent-tasks")["showing"]["description"], "raw");

    // One comment, by its own id, which is what the reported comments carry.
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    let id = ticket["comments"][0]["id"].as_i64().expect("the comment's id");
    did(&mut harness, &format!("plugins run agent-tasks show comment {id} raw"));
    let view = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(
        view["showing"]["comments_as_source"].as_array().expect("the list").len(),
        1,
        "that comment is being read as its source"
    );
    did(&mut harness, &format!("plugins run agent-tasks show comment {id} markdown"));
    assert!(did(&mut harness, "plugins view agent-tasks")["showing"]["comments_as_source"]
        .as_array()
        .expect("the list")
        .is_empty());

    // A word that is neither, and a comment that is not on this ticket, are refused with what there is.
    let reply = run(&mut harness, "plugins run agent-tasks show description sideways");
    assert!(!reply.ok);
    assert!(reply.message.contains("markdown"), "{}", reply.message);
    let reply = run(&mut harness, "plugins run agent-tasks show comment 9999 raw");
    assert!(!reply.ok);
    assert!(reply.message.contains("no comment 9999"), "{}", reply.message);
}

#[test]
fn choosing_in_each_dropdown_writes_the_field_it_names() {
    use unluminous_app::services::agent_tasks::agent;
    use unluminous_app::services::agent_tasks::model::Assignee;
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-sprint Current Sprint");
    did(&mut harness, "plugins run agent-tasks new-task Choose things for me");
    // Creating a ticket opens it as a **new** one — `+ Add Task`'s own state, which is what keeps
    // `Status` off the form: moving a lane before a ticket even has a title is not a thing anybody
    // wants. Re-opening it the way a click on an existing card does clears that, so `Status` is on
    // the form for the rest of this test to find.
    did(&mut harness, "plugins run agent-tasks open task-1");
    steady(&mut harness);

    // Choose one option out of one dropdown, by the words a person reads in the list.
    let pick = |harness: &mut Harness<'static, UnluminousApp>, control: &str, said: &str| {
        harness.get_by_label(control).click();
        steady(harness);
        harness.get_by_label(said).click();
        steady(harness);
    };

    pick(&mut harness, "Priority", "high");
    pick(&mut harness, "Assignee", "codex");
    // The model list follows the agent that was just chosen, which is the reason `Assignee` is chosen first.
    let codex_model =
        agent::models_for(Assignee::Codex, None).first().cloned().expect("a Codex model");
    pick(&mut harness, "Model", &codex_model);
    pick(&mut harness, "Effort", "high");
    pick(&mut harness, "Status", "IN PROGRESS");

    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    assert_eq!(ticket["task"]["priority"], "high", "the Priority dropdown wrote the priority");
    assert_eq!(ticket["task"]["assignee"], "codex", "the Assignee dropdown wrote the assignee");
    assert_eq!(ticket["task"]["model"], codex_model, "the Model dropdown wrote the model");
    assert_eq!(ticket["task"]["effort"], "high", "the Effort dropdown wrote the effort");
    assert_eq!(ticket["task"]["status"], "in_progress", "the Status dropdown moved the lane");

    // A dropdown that may hold nothing says so, and choosing that clears the column rather than writing an
    // empty string that nothing knows how to read.
    pick(&mut harness, "Effort", "Model default");
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    assert!(ticket["task"]["effort"].is_null(), "choosing nothing clears it: {}", ticket["task"]);
}

#[test]
fn add_task_opens_an_editor_with_every_field_a_ticket_needs() {
    // The fault this fixes: `+ Add Task` made a row and offered a title. A ticket needs a priority, an
    // assignee, a model, an effort, a project and an epic before it can be started, and none of the six had a
    // control, so a ticket could be created and not configured.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task");
    steady(&mut harness);
    // The modal is open, on a ticket nobody has named, which is what makes its footer an editor's.
    let view = did(&mut harness, "plugins view agent-tasks");
    assert_eq!(view["total"], 1);
    // `Discard` and `Done` are the editor's own footer, and a control rather than a painted heading: a ticket
    // that exists gets `Close` instead.
    assert!(harness.query_all_by_label_contains("Discard").count() > 0, "the editor's footer");
    assert!(harness.query_all_by_label_contains("Done").count() > 0);
    // Every field is reachable by name, which is what proves each has a control rather than a comment saying it
    // should have one.
    for control in ["Assignee", "Model", "Effort", "Priority", "Epic", "Project"] {
        assert!(
            harness.query_all_by_label_contains(control).count() > 0,
            "`{control}` has no control in the editor"
        );
    }
    // `task-28`: these are **dropdowns** now rather than rows of buttons, so an option is in the widget tree
    // once its list is open. Each list is opened and read, which is also what proves the control opens at all.
    for (control, options) in [
        ("Assignee", ["claude", "codex", "human"].as_slice()),
        ("Effort", ["low", "medium", "high", "xhigh", "max"].as_slice()),
        ("Priority", ["low", "medium", "high"].as_slice()),
    ] {
        harness.get_by_label(control).click();
        steady(&mut harness);
        for option in options {
            assert!(
                harness.query_all_by_label_contains(option).count() > 0,
                "`{option}` is not in the `{control}` list once it is open"
            );
        }
        // Shut it again, or the next one would open into a popup egui has already claimed.
        harness.get_by_label(control).click();
        steady(&mut harness);
    }
    // And the `Model` list offers the models the chosen agent has, which is what the ticket asked for: it used
    // to be a text field, so an agent could not be started without an identifier typed from memory.
    harness.get_by_label("Model").click();
    steady(&mut harness);
    for model in unluminous_app::services::agent_tasks::agent::models_for(
        unluminous_app::services::agent_tasks::model::Assignee::Claude,
        None,
    ) {
        assert!(
            harness.query_all_by_label_contains(&model).count() > 0,
            "`{model}` is not in the Model list"
        );
    }
    harness.get_by_label("Model").click();
    steady(&mut harness);
    // And each of them writes. Driven through the same function the buttons call.
    let board = harness.state_mut();
    let tasks = board
        .plugin_ui
        .provider("agent-tasks")
        .and_then(|provider| provider.as_any_mut())
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
        .expect("the Agent-Tasks provider");
    let id = tasks.detail().task.as_ref().expect("the new ticket").id;
    use unluminous_app::services::agent_tasks::Field;
    tasks.edit_field(id, Field::Assignee("codex".to_owned())).expect("the assignee");
    tasks.edit_field(id, Field::Priority("high".to_owned())).expect("the priority");
    tasks.edit_field(id, Field::Model("gpt-5.3-codex".to_owned())).expect("the model");
    tasks.edit_field(id, Field::Effort("xhigh".to_owned())).expect("the effort");
    tasks.edit_field(id, Field::Project("/tmp".to_owned())).expect("the project");
    tasks.detail_mut().title_draft = "Configured all the way".to_owned();
    tasks.save_the_title().expect("the title");
    tasks.save_the_description("Two lines of markdown.\n\nAnd a second.").expect("the description");
    steady(&mut harness);
    let ticket = did(&mut harness, "plugins run agent-tasks task task-1");
    assert_eq!(ticket["task"]["title"], "Configured all the way");
    assert_eq!(ticket["task"]["assignee"], "codex");
    assert_eq!(ticket["task"]["priority"], "high");
    assert_eq!(ticket["task"]["model"], "gpt-5.3-codex");
    assert_eq!(ticket["task"]["effort"], "xhigh");
    assert!(ticket["description"].as_str().expect("a description").contains("markdown"));
    // A value nothing knows is refused with what there is, rather than written.
    let board = harness.state_mut();
    let tasks = board
        .plugin_ui
        .provider("agent-tasks")
        .and_then(|provider| provider.as_any_mut())
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
        .expect("the provider");
    let problem = tasks
        .edit_field(id, Field::Assignee("gemini".to_owned()))
        .expect_err("an assignee the board does not know");
    assert!(problem.contains("gemini") && problem.contains("claude"), "{problem}");
    let problem = tasks
        .edit_field(id, Field::Effort("enormous".to_owned()))
        .expect_err("an effort the agents do not know");
    assert!(problem.contains("enormous") && problem.contains("xhigh"), "{problem}");
}

#[test]
fn the_ticket_modal_holds_every_section_the_browser_board_has() {
    // `tasks/agent-tasks-ui-tdd.md` §2.4 is the list. Each section is found by the name a person reads, which
    // is what makes this a check on the interface rather than on the code that draws it.
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task Something to work on");
    did(&mut harness, "plugins run agent-tasks close");
    did(&mut harness, "plugins run agent-tasks todo-add task-1 Read the old importer");
    did(&mut harness, "plugins run agent-tasks comment task-1 The format changed in April.");
    did(&mut harness, "plugins run agent-tasks open task-1");
    steady(&mut harness);
    for section in [
        "Description",
        "Todos",
        "Agent terminal",
        "Comments",
        "Post comment",
        "Send to terminal",
        "Assignee",
        "Priority",
        "Epic",
        "Project",
        "Status",
        "Start Work",
        "Delete task",
        "Close",
    ] {
        assert!(
            harness.query_all_by_label_contains(section).count() > 0,
            "`{section}` is not in the ticket modal"
        );
    }
    // The key is the heading, not `New task`: this ticket has been named.
    assert!(harness.query_all_by_label_contains("task-1").count() > 0);
    // **Both sections fold**, which is what the page this is modelled on does with its todos — and what a
    // person does to a ticket whose agent has written a screenful. The control is the heading; the command is
    // the agent's half of the same thing, and it reaches the same two flags.
    let folded = did(&mut harness, "plugins run agent-tasks fold terminal shut");
    assert_eq!(folded["terminal"], false);
    assert_eq!(folded["todos"], true, "shutting one leaves the other alone");
    steady(&mut harness);
    // The heading says which way it is, which is what a disclosure's name is for.
    assert!(harness.query_all_by_label_contains("Agent terminal, shut").count() > 0);
    did(&mut harness, "plugins run agent-tasks fold terminal open");
    steady(&mut harness);
    assert!(harness.query_all_by_label_contains("Agent terminal, open").count() > 0);
    assert_eq!(refused(&mut harness, "plugins run agent-tasks fold sideways"), "failed");
    // And the modal closes, leaving the board.
    did(&mut harness, "plugins run agent-tasks close");
    steady(&mut harness);
    assert_eq!(
        harness.query_all_by_label_contains("Post comment").count(),
        0,
        "the modal is gone once it is closed"
    );
}

/// `Enter` while typing in a ticket does not close it.
///
/// `task-28`: pressing `Enter` in the description or the comment box shut the modal, because the footer
/// took `Enter` as its own primary press and this modal's last button is `Close`. Its body holds a
/// multiline description and two fields that post on `Enter`, so `Enter` belongs to the body — the commit
/// panel's exception, reached for the same reason. `Escape` still closes it, and so does the button.
#[test]
fn enter_while_typing_in_a_ticket_does_not_close_it() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-task Something to work on");
    did(&mut harness, "plugins run agent-tasks close");
    did(&mut harness, "plugins run agent-tasks open task-1");
    steady(&mut harness);
    assert!(
        harness.query_all_by_label_contains("Post comment").count() > 0,
        "the modal is open to begin with"
    );
    for _ in 0..3 {
        harness.key_press(egui::Key::Enter);
        steady(&mut harness);
    }
    assert!(
        harness.query_all_by_label_contains("Post comment").count() > 0,
        "Enter is the body's, so the ticket is still open after three presses"
    );
    // And the ways out still work.
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert_eq!(
        harness.query_all_by_label_contains("Post comment").count(),
        0,
        "Escape still closes it, which `modal::show` owns for every dialog"
    );
}

#[test]
fn a_ticket_in_full_as_a_modal() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-sprint Current Sprint");
    did(
        &mut harness,
        "plugins run agent-tasks new-task Unluminous \u{2014} Plugin architecture for UI",
    );
    did(&mut harness, "plugins run agent-tasks close");
    did(&mut harness, "plugins run agent-tasks priority task-1 high");
    did(&mut harness, "plugins run agent-tasks todo-add task-1 Weigh the four mechanisms");
    did(&mut harness, "plugins run agent-tasks todo-add task-1 Write the manifest keys");
    did(&mut harness, "plugins run agent-tasks todo-add task-1 Draw the pane and the modal");
    did(&mut harness, "plugins run agent-tasks todo-done task-1 1");
    did(
        &mut harness,
        "plugins run agent-tasks comment task-1 Zed and Lapce both have no UI surface at all.",
    );
    did(&mut harness, "plugins run agent-tasks open task-1");
    steady(&mut harness);
    harness.state_mut().plugin_ui.provider("agent-tasks").expect("the board");
    // A description with two paragraphs in it, written the way a person writes one.
    let board = harness.state_mut();
    let tasks = board
        .plugin_ui
        .provider("agent-tasks")
        .and_then(|provider| provider.as_any_mut())
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_tasks::AgentTasks>())
        .expect("the provider");
    tasks
        .save_the_description(
            "Widen the plugin system so a plugin can draw: a rail button, a pane, a tab, a menu and a \
             Settings page.\n\nthe reference editor does this declaratively and VS Code does it with 46 contribution \
             points. Zed and Lapce have no UI surface at all.",
        )
        .expect("a description");
    steady(&mut harness);
    harness.snapshot(shot("agent_tasks_modal").as_str());
}

#[test]
fn the_editor_for_a_new_ticket() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "plugins run agent-tasks new-sprint Current Sprint");
    did(&mut harness, "plugins run agent-tasks new-task");
    steady(&mut harness);
    harness.snapshot(shot("agent_tasks_editor").as_str());
}

#[test]
fn the_boards_own_settings_page_with_its_agent_configuration() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-tasks/board --show");
    did(&mut harness, "action run settings");
    steady(&mut harness);
    harness.get_all_by_label("Agent-Tasks").last().expect("the Settings row").click();
    steady(&mut harness);
    harness.snapshot(shot("agent_tasks_settings").as_str());
}

/// Reach the chat plugin's own state, the way its buttons do.
///
/// `UiProvider::as_any_mut` exists for exactly two callers and this is one of them.
fn with_the_chat(
    harness: &mut Harness<'static, UnluminousApp>,
    act: impl FnOnce(&mut unluminous_app::services::agent_chat::AgentChat),
) {
    let provider = harness
        .state_mut()
        .plugin_ui
        .opened("agent-chat", "agent-chat")
        .expect("the chat provider");
    let chat = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_chat::AgentChat>())
        .expect("the chat provider as its own type");
    act(chat);
    // **Stepped rather than run.** A pane with an answer arriving asks to be drawn again, so
    // `Harness::run` gives it four steps to go quiet and panics when it does not — which is right for
    // a settled window and wrong for a streaming one. `did_while_waiting` is the same rule, and
    // `task-1654` wrote it down for the loops that wait on git.
    harness.step();
    harness.step();
}

/// Show the pane with a conversation already in it.
fn a_chat(said: &[(unluminous_chat::Role, &str)]) -> Harness<'static, UnluminousApp> {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    let said: Vec<(unluminous_chat::Role, String)> =
        said.iter().map(|(role, text)| (*role, (*text).to_owned())).collect();
    with_the_chat(&mut harness, move |chat| {
        for (role, text) in said {
            let id = chat.session_mut().chat.next_id();
            chat.session_mut().chat.push(unluminous_chat::Message::said(id, role, text));
        }
    });
    harness
}

/// The ticket's first sentence, as data: a right hand column with a button in the rail.
#[test]
fn the_chat_contributes_a_pane_on_the_right_with_a_button_in_the_rail() {
    use unluminous_app::app::dock::{Panel, Side};
    let mut harness = harness("");
    let listed = did(&mut harness, "plugins list");
    let plugins = listed["plugins"].as_array().expect("the plugins");
    let chat =
        plugins.iter().find(|plugin| plugin["id"] == "agent-chat").expect("the agent-chat plugin");
    assert_eq!(chat["kind"], "ui");
    assert_eq!(chat["provider"], "agent-chat");
    let contributes: Vec<&str> = chat["contributes"]
        .as_array()
        .expect("what it adds")
        .iter()
        .filter_map(|it| it.as_str())
        .collect();
    assert_eq!(
        contributes,
        ["pane", "menu", "settings page"],
        "a pane, a menu and a page, and no tab"
    );

    // It is in a dock slot, so the rail has a button for it and the dock has a column for it.
    let slot =
        harness.state().plugin_ui.slot_of("agent-chat/chat").expect("the chat's pane is in a slot");
    // `Agent-Chat pane`, because the plugin's menu is called `Agent-Chat` and no two controls in one
    // window may share a name.
    assert!(
        harness.get_all_by_label("Agent-Chat pane").count() > 0,
        "the rail draws a button for the contributed pane"
    );

    // Shown, moved to another edge and put away, which is what its header's drag and its rail button
    // both do — none of it is code in this plugin, because `task-1697` built it for every panel.
    let shown = did(&mut harness, "plugins pane agent-chat/chat --show");
    assert_eq!(shown["showing"], true);
    assert_eq!(shown["side"], "right", "the ticket asks for a right panel");
    let moved = did(&mut harness, "plugins pane agent-chat/chat --side bottom");
    assert_eq!(moved["side"], "bottom");
    assert_eq!(side_of(&harness, Panel::Plugin(slot as u8)), Side::Bottom);
    did(&mut harness, "plugins pane agent-chat/chat --side right");
    let away = did(&mut harness, "plugins pane agent-chat/chat --hide");
    assert_eq!(away["showing"], false);
    steady(&mut harness);
    assert_eq!(harness.state().panel_rect_for_tests(Panel::Plugin(slot as u8)).width(), 0.0);
}

/// `task-1794`: the chat pane goes on drawing after the panels are reset, and says so honestly.
///
/// The shoot reported the pane painting **nothing at all** — no ground, no divider, no composer, no
/// rail highlight — while `plugins pane agent-chat/chat --show` answered "showing on the right",
/// `plugins view agent-chat` returned the whole conversation and `send` streamed an answer nobody
/// could see. Four takes were lost to it.
///
/// Two pieces of state decide whether a contributed pane is on the screen and only one was ever
/// asked. `PluginUi::is_visible` is the provider's own flag; the **dock** separately has to know the
/// pane exists, and `Layout::reset` is `*self = Layout::new()`, which has no contributed panes in
/// it. So `panel reset` — and `settings reset`, which builds a fresh `Panes` — did not move the pane,
/// it lost it: `panels_on` filters through `Panel::all(0)`, `regions` hands back `Rect::ZERO`, and
/// `show_the_plugin_panes` skips anything under a point. Nothing anywhere reported it.
///
/// This drives the shoot's own baseline and then asserts all three: the layout, the picture, and
/// what the command says.
#[test]
fn the_chat_pane_still_draws_after_the_panels_are_reset() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    let slot = harness.state().plugin_ui.slot_of("agent-chat/chat").expect("a slot");

    // Hiding it once was enough to be worth reporting, so the cycle is driven first.
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "plugins pane agent-chat/chat --hide");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    assert!(
        harness.state().plugin_pane_is_showing(slot),
        "a hide and a show is not enough to lose it"
    );

    // The video's own baseline: the panels put back, and a handful of settings written.
    did(&mut harness, "panel reset");
    did(&mut harness, "settings set appearance.font.size 16");
    did(&mut harness, "settings set editor.line_numbers true");
    did(&mut harness, "settings set terminal.font.size 14");
    did(&mut harness, "settings set editor.suggestions automatic");
    did(&mut harness, "settings set appearance.background.opacity 1.0");
    steady(&mut harness);

    // The dock still knows it exists, which is the half that went missing.
    assert!(
        harness.state().plugin_pane_is_reachable(slot),
        "the reset lost the contributed pane rather than moving it"
    );
    let rect = harness.state().panel_rect_for_tests(Panel::Plugin(slot as u8));
    assert!(rect.width() > 1.0 && rect.height() > 1.0, "the pane has no room to draw in: {rect:?}");

    // It is really drawn: its own furniture is in the tree, which is what "no composer" means.
    assert!(
        harness.get_all_by_label_contains("Agent-Chat").count() > 0,
        "the pane's header should be drawn"
    );

    // And the command tells the truth about it rather than reporting a flag.
    let reply = did(&mut harness, "plugins pane agent-chat/chat --show");
    assert_eq!(reply["showing"], true);
    assert_eq!(reply["side"], "right");

    // The pixels, which is what the ticket asks for: a pane that paints nothing looks like a window
    // with no pane, and only a picture says which of the two this is.
    harness.snapshot(shot("agent_chat_after_a_panel_reset").as_str());
}

/// `task-1794`: a contributed pane's width and height are settable from the command line.
///
/// `panel size` knew only the four built-in panels and `settings set` refused
/// `panes.agent-chat/chat.width` — a key **Unluminous writes into its own settings file**. So the chat
/// pane in the product video had to be widened to 1150 by editing `settings.conf` between restarts,
/// because at its manifest's 420 the answer wrapped to three words a line. Every built-in panel's
/// width was settable; this is the pane rule with a gap in it, and the gap was the whole of it.
#[test]
fn a_contributed_panes_size_is_settable_from_the_command_line() {
    use unluminous_app::app::dock::Panel;
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    let slot = harness.state().plugin_ui.slot_of("agent-chat/chat").expect("a slot");
    let panel = Panel::Plugin(slot as u8);

    // The command the ticket names, with the number the video needed.
    let sized = did(&mut harness, "panel size agent-chat/chat --width 1150");
    assert_eq!(sized["width"], 1150.0);
    assert_eq!(sized["panel"], "agent-chat/chat", "it answers by the name it was asked by");
    assert_eq!(harness.state().panes.width_of(panel), 1150.0);

    // The height too, because a pane docked to a strip is read by that instead.
    did(&mut harness, "panel size agent-chat/chat --height 480");
    assert_eq!(harness.state().panes.height_of(panel), 480.0);

    // And it really is drawn wider, which is the point of setting it. Asked for 1150 in a window
    // 1180 wide it gets what is left after the editing area's own minimum, which is `regions`' rule
    // rather than anything about plugins — so the size that fits is what the rectangle is measured
    // against, and the number above is what is kept.
    did(&mut harness, "panel size agent-chat/chat --width 700");
    steady(&mut harness);
    let rect = harness.state().panel_rect_for_tests(panel);
    assert!(
        (rect.width() - 700.0).abs() < 1.0,
        "the pane should be drawn at the width it was given: {rect:?}"
    );

    // The same numbers through the settings, which is the other half of the ticket: a key Unluminous
    // writes is a key Unluminous reads.
    did(&mut harness, "settings set panes.agent-chat/chat.width 900");
    assert_eq!(harness.state().panes.width_of(panel), 900.0);
    let got = did(&mut harness, "settings get panes.agent-chat/chat.width");
    assert_eq!(got["value"], "900");

    // `settings list` names them all, which is what the refusal told people to run.
    let listed = did(&mut harness, "settings list");
    let rows: Vec<String> = listed["lines"]
        .as_array()
        .map(|all| all.iter().filter_map(|row| row.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    for key in ["width", "height", "zoom"] {
        assert!(
            rows.iter().any(|row| row.starts_with(&format!("panes.agent-chat/chat.{key}"))),
            "`settings list` should name panes.agent-chat/chat.{key}: {rows:#?}"
        );
    }

    // `panel list` stays Unluminous's own four — a contributed pane is listed by `plugins list` and
    // moved by `plugins pane`, which is the split `the_board_contributes_a_tab_and_no_pane` states.
    // What makes the name reachable from here is the refusal, asserted at the end of this test.

    // Docking by the same name, which `panel zoom` already accepted and `panel dock` did not.
    let docked = did(&mut harness, "panel dock agent-chat/chat left");
    assert_eq!(docked["side"], "left");
    did(&mut harness, "panel dock agent-chat/chat right");

    // Reset puts it back to what the **manifest** asked for rather than to a number inside Unluminous,
    // because that is what "what a new Unluminous has" means for a pane Unluminous did not write.
    let back = did(&mut harness, "settings reset panes.agent-chat/chat.width");
    assert_eq!(back["value"], "420", "the chat manifest asks for 420");
    assert_eq!(harness.state().panes.width_of(panel), 420.0);

    // A key that names no pane is still refused, so the dynamic half has not opened the door to
    // anything at all.
    let refused = run(&mut harness, "settings set panes.no-such/pane.width 400");
    assert!(!refused.ok, "a pane nothing contributes is not a setting");
    let refused = run(&mut harness, "panel size no-such/pane --width 400");
    assert!(!refused.ok);
    assert!(
        refused.message.contains("agent-chat/chat"),
        "the refusal should name the panes there are: {}",
        refused.message
    );

    // And the **sentence** names the pane too, not just the payload beside it. `Panel::label` is a
    // `&'static str` and answered "Plugin pane" for every contributed pane, so with two of them
    // installed the reply read the same whichever one was asked about — unactionable for the caller
    // reading the plain shape rather than the JSON. It is the pane's own header wording, which is
    // what the manifest's `label` is for.
    for said in [
        run(&mut harness, "panel size agent-chat/chat --width 1150").message,
        run(&mut harness, "panel dock agent-chat/chat right").message,
        run(&mut harness, "panel zoom agent-chat/chat").message,
    ] {
        assert!(
            said.starts_with("Agent-Chat"),
            "the sentence should name the pane it is about, not `Plugin pane`: {said}"
        );
    }
    // Unluminous's own four are unchanged by that, which is what keeps `Resize explorer` meaning what
    // it meant — the wording here is the label, not the wire name.
    assert!(run(&mut harness, "panel dock explorer left").message.starts_with("Project"));
    did(&mut harness, "panel dock explorer left");
}

/// The other half of the same ticket: a pane that cannot be drawn is **refused**, not reported as
/// showing.
///
/// The cause found above is fixed, so nothing a person or an agent can type reaches this state any
/// more — the dock is put back into it by hand here, which is exactly what a future regression would
/// do. Without the guard the command answers `showing: true` about a pane that paints nothing, and
/// there is no question an agent can ask that reports the difference. That is the AI-first contract
/// with a hole in it, which is how the ticket puts it.
#[test]
fn a_pane_the_window_has_no_room_for_is_refused_rather_than_called_showing() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    let slot = harness.state().plugin_ui.slot_of("agent-chat/chat").expect("a slot");
    assert!(harness.state().plugin_pane_is_showing(slot));

    // The state the fault left behind: the provider still says yes, the dock has never heard of it.
    harness.state_mut().panes.dock.reset();
    steady(&mut harness);
    assert!(harness.state().plugin_ui.is_visible(slot), "the provider's flag is untouched");
    assert!(!harness.state().plugin_pane_is_showing(slot), "but nothing would be drawn");

    let refused = run(&mut harness, "plugins pane agent-chat/chat --show");
    assert!(!refused.ok, "a pane that would draw nothing must not answer that it is showing");
    assert!(
        refused.message.contains("would draw nothing"),
        "the refusal should say what is wrong: {}",
        refused.message
    );
}

/// Nothing said yet: the badge, the heading and the four starter chips.
#[test]
fn the_chat_pane_with_nothing_said_in_it() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    harness.snapshot(shot("agent_chat_empty").as_str());
}

/// A question and an answer: the two bubbles, each with its own corner squared off.
#[test]
fn a_question_and_an_answer_are_two_bubbles_on_opposite_sides() {
    let mut harness = a_chat(&[
        (unluminous_chat::Role::User, "Why is `relayout` keeping a paragraph it should have thrown away?"),
        (
            unluminous_chat::Role::Assistant,
            "Because the fingerprint does not carry the hidden flag.\n\n```rust\nlet fingerprint = (text, style, hidden);\n```\n\n| Case | Kept |\n|---|---|\n| edited | no |\n| folded | yes |\n",
        ),
    ]);
    harness.snapshot(shot("agent_chat_conversation").as_str());
}

/// An answer arriving: what has come so far, with the state dot showing that it is still coming.
#[test]
fn an_answer_that_is_still_arriving_shows_what_has_come_so_far() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    with_the_chat(&mut harness, |chat| {
        chat.session_mut().ask(unluminous_chat::Message::said(
            0,
            unluminous_chat::Role::User,
            "Explain the fold.",
        ));
        chat.session_mut()
            .reply(unluminous_chat::Reply::Started { model: "claude-opus-5".to_owned() });
        chat.session_mut().reply(unluminous_chat::Reply::Text(
            "A hidden paragraph produces no lines and keeps".to_owned(),
        ));
    });
    assert_eq!(did_while_waiting(&mut harness, "plugins view agent-chat")["state"], "streaming");
    harness.snapshot(shot("agent_chat_streaming").as_str());
}

/// A tool the model asked for is run by the window, through the same `run_cli` a menu entry runs.
///
/// **The whole loop, in a real window**: the provider resolves the call against the catalogue, hands
/// the window a `Request::RunCommand`, the window runs it and hands the answer back through
/// `UiProvider::answered`, and the result goes into the conversation as a `tool` message. The tool
/// limit is one, so the round after it is refused rather than sent — no test here makes a request.
#[test]
fn a_tool_the_model_asked_for_is_run_by_the_window_and_its_answer_comes_back() {
    let mut harness = harness("Some prose.");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    // **An address row, because only an address row asks Unluminous to run anything.** The two rows that
    // ship run a command-line agent, and an agent has its own tools — so with one of those chosen
    // this loop does not exist and the turn would sit in `waiting-for-tools` for ever waiting on a
    // window that is deliberately not going to answer.
    did(&mut harness, "plugins run agent-chat use local");
    did(&mut harness, "plugins run agent-chat tools on");
    with_the_chat(&mut harness, |chat| {
        chat.configuration_mut().tool_limit = 1;
        chat.session_mut().ask(unluminous_chat::Message::said(
            0,
            unluminous_chat::Role::User,
            "What is open?",
        ));
        chat.session_mut().reply(unluminous_chat::Reply::ToolCall {
            id: "t1".to_owned(),
            name: "unluminous_tab".to_owned(),
            arguments: "{\"command\":\"list\"}".to_owned(),
        });
        chat.session_mut()
            .reply(unluminous_chat::Reply::Finished { reason: "tool_use".to_owned() });
    });
    // Two steps: one for `let_the_plugins_catch_up` to hand the call over and the window to run it,
    // one for the answer to land in the conversation.
    harness.step();
    harness.step();
    let view = did(&mut harness, "plugins view agent-chat");
    let tools = view["conversation"]["messages"][1]["tools"].as_array().expect("the tool calls");
    let answer = tools[0]["answer"].as_str().expect("the window answered the call");
    assert!(
        answer.contains("untitled") || answer.contains("tabs"),
        "the answer is what `tab list` really said: {answer}"
    );
    assert_eq!(tools[0]["failed"], false);
    // And the turn stops at the limit rather than asking the model again, which is what stops a loop
    // nobody is watching from being funded.
    assert_eq!(view["state"], "failed");
    assert!(
        view["conversation"]["messages"]
            .as_array()
            .expect("the messages")
            .iter()
            .any(|message| message["failure"].as_str().is_some_and(|said| said.contains("limit"))),
        "the refusal says it was the limit: {view}"
    );
}

/// Press and let go at one point, the way a person clicks something with no name to find it by.
fn click_at(harness: &mut Harness<'static, UnluminousApp>, at: egui::Pos2) {
    let modifiers = egui::Modifiers::default();
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    harness.step();
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        });
        harness.step();
    }
    steady(harness);
}

/// The model selector at the top right of the chat is `rux`'s dropdown, and choosing a row in it
/// chooses that endpoint. `task-2096`.
///
/// The rows of a `rux` menu carry no names, so the row is found by where `rux` puts it: six points
/// under the trigger, six points of padding, and one row height per row above it.
#[test]
fn the_model_selector_is_a_dropdown_and_a_row_in_it_chooses_that_endpoint() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    with_the_chat(&mut harness, |chat| {
        let mut second = chat.configuration().providers[0].clone();
        second.name = "second".to_owned();
        chat.configuration_mut().providers.push(second);
    });
    steady(&mut harness);
    let names: Vec<String> = {
        let mut names = Vec::new();
        with_the_chat(&mut harness, |chat| {
            names = chat.configuration().providers.iter().map(|one| one.name.clone()).collect();
        });
        names
    };
    let trigger = harness.get_by_label("Model").rect();
    harness.get_by_label("Model").click();
    steady(&mut harness);
    let row = {
        let painter = egui::Painter::new(
            harness.ctx.clone(),
            egui::LayerId::background(),
            egui::Rect::EVERYTHING,
        );
        rux::text::measure(&painter, rux::Style::CONTROL, "Ag").y + 16.0
    };
    let last = names.len() - 1;
    let at =
        egui::pos2(trigger.center().x, trigger.bottom() + 6.0 + 6.0 + row * (last as f32 + 0.5));
    click_at(&mut harness, at);
    let mut chosen = String::new();
    with_the_chat(&mut harness, |chat| {
        chosen = chat.provider().map(|one| one.name.clone()).unwrap_or_default();
    });
    assert_eq!(
        chosen, "second",
        "the last row of the dropdown was pressed; the rows are {names:?}"
    );
}

/// Zooming a file does not resize the chat pane. `task-2096`.
///
/// Zooming a file walks `appearance.font.size`, and the chat used to take its size from that, so every
/// notch over the file grew the chat's composer and its answers too. The composer's height is measured
/// before and after the editor's font is made twice as large.
#[test]
fn zooming_a_file_leaves_the_chat_pane_the_size_it_was() {
    let mut harness = harness("Some prose.");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    let before = harness.get_by_label("Message").rect();
    did(&mut harness, "settings set appearance.font.size 32");
    steady(&mut harness);
    let after = harness.get_by_label("Message").rect();
    assert_eq!(before.height(), after.height(), "the composer changed size with the editor's font");

    // The chat's own zoom still works.
    did(&mut harness, "panel zoom agent-chat/chat 1.5");
    steady(&mut harness);
    let zoomed = harness.get_by_label("Message").rect();
    assert!(zoomed.height() > after.height(), "{zoomed:?} against {after:?}");
}

/// A tool call can take a picture of the window, which answers on a later frame.
///
/// `task-2096`: a screenshot used to come back as *"waits for something to happen, and a tool call
/// cannot wait"*, because `window screenshot` is answered once a frame has been painted and the tool
/// call path refused anything that was not answered at once. It is held now and answered when the
/// picture has been written.
#[test]
fn a_tool_call_can_take_a_screenshot_of_the_window() {
    let mut harness = harness("Some prose.");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "plugins run agent-chat use local");
    did(&mut harness, "plugins run agent-chat tools on");
    let picture =
        std::env::temp_dir().join(format!("unluminous-tool-screenshot-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&picture);
    let arguments = serde_json::json!({
        "command": "screenshot",
        "arguments": { "file": picture.to_string_lossy() },
    })
    .to_string();
    with_the_chat(&mut harness, |chat| {
        chat.configuration_mut().tool_limit = 1;
        chat.session_mut().ask(unluminous_chat::Message::said(
            0,
            unluminous_chat::Role::User,
            "What does the window look like?",
        ));
        chat.session_mut().reply(unluminous_chat::Reply::ToolCall {
            id: "t1".to_owned(),
            name: "unluminous_window".to_owned(),
            arguments,
        });
        chat.session_mut()
            .reply(unluminous_chat::Reply::Finished { reason: "tool_use".to_owned() });
    });
    // The picture settles for a quarter of a second before it is asked for, so this waits in real
    // time rather than counting frames.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let tool = loop {
        harness.step();
        let view = did(&mut harness, "plugins view agent-chat");
        let tool = view["conversation"]["messages"][1]["tools"][0].clone();
        if tool["answer"].is_string() {
            break tool;
        }
        assert!(std::time::Instant::now() < deadline, "the call was never answered: {view}");
        std::thread::sleep(std::time::Duration::from_millis(30));
    };
    assert_eq!(tool["failed"], false, "the screenshot was refused: {tool}");
    assert!(!tool["answer"].as_str().unwrap_or("").contains("waits for something"), "{tool}");
    assert!(picture.is_file(), "the picture was written: {tool}");
    let _ = std::fs::remove_file(&picture);
}

/// A tool call while it is running, and the same call once it has been answered.
///
/// **Built rather than driven.** Answering the last outstanding tool is what sends the next request,
/// and no screenshot test here makes one; the loop itself is
/// `a_tool_the_model_asked_for_is_run_by_the_window_and_its_answer_comes_back` above.
#[test]
fn a_tool_call_is_shown_while_it_runs_and_when_it_has_finished() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "plugins run agent-chat use local");
    did(&mut harness, "plugins run agent-chat tools on");
    steady(&mut harness);
    with_the_chat(&mut harness, |chat| {
        let chat = chat.session_mut();
        let id = chat.chat.next_id();
        chat.chat.push(unluminous_chat::Message::said(
            id,
            unluminous_chat::Role::User,
            "What does git say?",
        ));
        let id = chat.chat.next_id();
        let mut answering =
            unluminous_chat::Message::said(id, unluminous_chat::Role::Assistant, "Let me look.");
        answering.tools.push(unluminous_chat::ToolCall::new(
            "t1",
            "unluminous_git",
            "{\"command\":\"status\"}",
        ));
        chat.chat.push(answering);
    });
    harness.snapshot(shot("agent_chat_tool_running").as_str());

    // Answered, so the block says how long it took and the model's next answer is under it.
    with_the_chat(&mut harness, |chat| {
        let chat = chat.session_mut();
        let tool = &mut chat.chat.messages[1].tools[0];
        tool.answer = Some("{\"branch\":\"main\",\"changed\":1}".to_owned());
        tool.took = Some(84);
        let id = chat.chat.next_id();
        chat.chat.push(unluminous_chat::Message::said(
            id,
            unluminous_chat::Role::Assistant,
            "The branch is `main`, with one change on it.",
        ));
    });
    let view = did(&mut harness, "plugins view agent-chat");
    let tools = view["conversation"]["messages"][1]["tools"].as_array().expect("the tool calls");
    assert_eq!(tools[0]["name"], "unluminous_git");
    assert!(tools[0]["answer"].as_str().expect("an answer").contains("main"));
    harness.snapshot(shot("agent_chat_tool_answered").as_str());
}

/// A picture goes up: attached to the draft, and drawn inside the bubble once it has been sent.
///
/// **Up only.** Neither API answers with a picture, so nothing comes back that way — what is drawn is
/// what was sent. The bytes are read at the moment it is attached rather than the path being
/// remembered, which is `model::Part::Picture`'s own reason: a conversation reopened after the file
/// has moved still shows what was sent.
#[test]
fn a_picture_attached_to_a_question_is_drawn_in_the_bubble_it_was_sent_with() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    let picture = sample_folder().join("picture.png");
    let attached =
        did(&mut harness, &format!("plugins run agent-chat attach {}", picture.display()));
    assert_eq!(attached["attachments"], 1);
    assert_eq!(
        did(&mut harness, "plugins view agent-chat")["attachments"][0]["name"],
        "picture.png"
    );

    // Sent, which here is done by hand because no test in this file makes a request: the attachment
    // becomes a `Part::Picture` on the message beside its words.
    with_the_chat(&mut harness, |chat| {
        let parts = chat.take_the_attachments();
        let id = chat.session_mut().chat.next_id();
        let mut asked = unluminous_chat::Message::new(id, unluminous_chat::Role::User);
        asked
            .parts
            .push(unluminous_chat::Part::Text("What is wrong with this diagram?".to_owned()));
        asked.parts.extend(parts);
        chat.session_mut().chat.push(asked);
        let id = chat.session_mut().chat.next_id();
        chat.session_mut().chat.push(unluminous_chat::Message::said(
            id,
            unluminous_chat::Role::Assistant,
            "The arrow leaves the subgraph rather than the node inside it.",
        ));
    });
    let view = did(&mut harness, "plugins view agent-chat");
    assert_eq!(view["attachments"].as_array().map(Vec::len), Some(0), "the draft was emptied");
    // Named and measured rather than printed: a base64 payload in a command line's answer is a
    // screenful of nothing anybody can read.
    let pictures =
        view["conversation"]["messages"][0]["pictures"].as_array().expect("the pictures");
    assert_eq!(pictures[0]["name"], "picture.png");
    assert_eq!(pictures[0]["media"], "image/png");
    assert!(pictures[0]["bytes"].as_u64().is_some_and(|bytes| bytes > 0));
    harness.snapshot(shot("agent_chat_picture").as_str());
}

/// A pane opened after a file was already showing is told which file, rather than waiting for a change.
///
/// The window compares before it clones and tells nothing on a frame where neither moved — which is
/// `task-1666`'s rule and is also what made the first version wrong: a pane opened *after* the tab was
/// opened had missed the only announcement there would ever be, so `plugins view agent-chat` said no
/// file was showing while one plainly was. The answer it is told is **which file, never its text**.
#[test]
fn a_chat_opened_after_a_file_is_told_which_file_is_showing() {
    let mut harness = harness("Some prose.");
    let opened = sample_folder().join("readme.md");
    did(&mut harness, &format!("tab open {} --permanent", opened.display()));
    steady(&mut harness);

    // Opened only now, which is the case the once-a-frame comparison would otherwise skip.
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    let view = did(&mut harness, "plugins view agent-chat");
    let showing = view["showing"].as_str().expect("the pane was told which file is showing");
    assert!(showing.ends_with("readme.md"), "{showing}");
    assert!(
        !serde_json::to_string(&view).expect("json").contains("Some prose."),
        "which file, never the file's text: {view}"
    );

    // And it follows the tab from then on, because that is the same announcement.
    did(
        &mut harness,
        &format!("tab open {} --permanent", sample_folder().join("notes.txt").display()),
    );
    steady(&mut harness);
    let followed = did(&mut harness, "plugins view agent-chat")["showing"]
        .as_str()
        .expect("the file that is showing")
        .to_owned();
    assert!(followed.ends_with("notes.txt"), "{followed}");
}

/// A refusal is the server's own words, kept beside whatever had already arrived.
#[test]
fn a_refusal_is_drawn_in_the_servers_own_words() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    with_the_chat(&mut harness, |chat| {
        chat.session_mut().ask(unluminous_chat::Message::said(
            0,
            unluminous_chat::Role::User,
            "Hello?",
        ));
        chat.session_mut().reply(unluminous_chat::Reply::Text("Half an ans".to_owned()));
        chat.session_mut().reply(unluminous_chat::Reply::Failed(
            "HTTP 429: rate_limit_error: too many requests".to_owned(),
        ));
    });
    let view = did(&mut harness, "plugins view agent-chat");
    assert_eq!(view["state"], "failed");
    harness.snapshot(shot("agent_chat_failed").as_str());
}

/// A folder holding stand-ins for `claude` and `codex`, put at the front of this process's `PATH`.
///
/// **So that a page showing whether an agent is installed draws the same thing on every machine.**
/// The rows that ship run a program, and whether one is found is exactly the sort of thing a
/// screenshot test must not depend on — it is installed on the machine this is developed on and it
/// will not be on the next one, and the page would then draw a red refusal there and a green line
/// here. Written once behind a `OnceLock` for the reason `sample_folder` is: two tests writing one
/// fixture is one test reading a file another has just truncated.
fn agents_on_the_path() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let folder = std::env::temp_dir().join("unluminous-screenshot-agents");
        std::fs::create_dir_all(&folder).expect("a folder for the stand-in agents");
        for name in ["claude", "codex"] {
            let at = folder.join(match cfg!(windows) {
                true => format!("{name}.exe"),
                false => name.to_owned(),
            });
            if !at.is_file() {
                std::fs::write(&at, b"").expect("a file that stands in for an agent");
            }
        }
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut folders = vec![folder];
        folders.extend(std::env::split_paths(&path));
        let joined = std::env::join_paths(folders).expect("a PATH");
        std::env::set_var("PATH", joined);
    });
}

/// The Endpoints list scrolls to its last row.
///
/// `task-1848` reported the page "not scrollable", with the list cut off mid-row and no way to reach the
/// endpoint below the fold. The scrolling area was there; what was missing is that the page never said how
/// tall it drew — it tracked the height all the way down and then discarded it with `let _ = pen;` — so the
/// area believed the contents were zero tall. Every row is painted at an absolute position and allocates
/// nothing of its own.
///
/// **Asserted on a row moving up the window** rather than on egui's own scroll state, which is stored under
/// an id composed from the parent and is not the salt this page passes. A control in a different place after
/// a wheel is what a person sees, and zero pixels of movement was the fault.
#[test]
fn the_chats_settings_page_scrolls_to_its_last_row() {
    agents_on_the_path();
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "action run settings");
    steady(&mut harness);
    harness.get_all_by_label("Agent-Chat").last().expect("the Settings row").click();
    steady(&mut harness);

    // The first endpoint's `Use` button, near the top of the scrolling area.
    let where_it_is = |harness: &mut Harness<'_, UnluminousApp>| -> f32 {
        harness.get_all_by_label("Use").next().expect("an endpoint row").rect().top()
    };
    let before = where_it_is(&mut harness);

    // A wheel with the pointer over that row: egui gives a wheel to the area the pointer is inside, so one
    // sent with the pointer nowhere scrolls nothing.
    let over = harness.get_all_by_label("Use").next().expect("an endpoint row").rect().center();
    harness.input_mut().events.push(egui::Event::PointerMoved(over));
    steady(&mut harness);
    harness.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, -240.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::default(),
    });
    steady(&mut harness);
    steady(&mut harness);

    let after = where_it_is(&mut harness);
    assert!(
        after < before - 20.0,
        "the page moved up: it was at {before} and is at {after}. Without `allocate_space` the scrolling \
         area is told the contents are zero tall and nothing moves at all."
    );
}

/// A notice from a plugin becomes a toast the window draws and a person can dismiss.
///
/// `task-1848`: "I'm not seeing any errors when I try to chat with my agent. Nothing happens... no error."
/// Every reason `AgentChat::send` refuses went to `Request::Message`, which is the status bar — a sentence
/// in the smallest text at the far bottom edge, replaced by whatever is reported next. The chat pane's
/// failures are `Request::Notice` now, and this is the window's half: a notice is drawn, it is named so it
/// can be pressed, and `Escape` puts it away one at a time.
#[test]
fn a_notice_is_drawn_over_the_window_and_can_be_dismissed() {
    use unluminous_app::components::toast::Kind;
    let mut harness = harness("Some prose.");
    steady(&mut harness);
    assert!(harness.state().toasts.is_empty(), "nothing has gone wrong yet");

    harness.state_mut().toasts.say("could not send: no endpoint is configured.", Kind::Problem);
    steady(&mut harness);
    assert_eq!(harness.state().toasts.len(), 1);
    // Named, so a test can find it and a person's pointer has something to press.
    assert_eq!(
        harness.get_all_by_label("Dismiss notice 1").count(),
        1,
        "the notice draws a cross that names which one it belongs to"
    );

    harness.state_mut().toasts.say("and a second thing went wrong.", Kind::Problem);
    steady(&mut harness);
    assert_eq!(harness.state().toasts.len(), 2);
    harness.key_press(egui::Key::Escape);
    steady(&mut harness);
    assert_eq!(harness.state().toasts.len(), 1, "Escape dismissed the newest and left the other");

    harness.get_by_label("Dismiss notice 1").click();
    steady(&mut harness);
    assert!(harness.state().toasts.is_empty(), "the cross dismissed it");
}

/// The bottom of the page, which is the picture `task-2003` was reported with.
///
/// *"the settings configurations look like crap"*, and what the screenshot showed was the permission
/// buttons, the System box and the History box with **no labels, no headings and no sentences beside
/// them**. That is a clip rather than a style: the page paints at absolute positions through a painter
/// cut to `Ui::available_rect_before_wrap`, which inside a `ScrollArea` is the viewport moved up by the
/// scroll offset — so scrolling threw away the bottom of the window, and only the fields and the
/// buttons survived because those are clipped by the scrolling area itself. `modal::down_the_page`.
///
/// A picture rather than a query, because what went missing is **painted** text: a heading, a label and
/// a note register no widget, so there is nothing for `get_by_label` to fail to find. It is the one
/// thing that would have caught this and the one thing that will catch it again.
#[test]
fn the_chats_settings_page_keeps_its_labels_when_it_is_scrolled() {
    agents_on_the_path();
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "action run settings");
    steady(&mut harness);
    harness.get_all_by_label("Agent-Chat").last().expect("the Settings row").click();
    steady(&mut harness);

    // Far enough to reach the last section, with the pointer inside the page so egui gives it the wheel.
    let over = harness.get_all_by_label("Use").next().expect("an endpoint row").rect().center();
    harness.input_mut().events.push(egui::Event::PointerMoved(over));
    steady(&mut harness);
    for _ in 0..8 {
        harness.input_mut().events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -240.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::default(),
        });
        steady(&mut harness);
    }
    steady(&mut harness);
    harness.snapshot(shot("agent_chat_settings_scrolled").as_str());
}

/// The two rows that run an agent and the one that sends to an address, and never a key.
#[test]
fn the_chats_settings_page_lists_the_endpoints() {
    agents_on_the_path();
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "action run settings");
    steady(&mut harness);
    harness.get_all_by_label("Agent-Chat").last().expect("the Settings row").click();
    steady(&mut harness);
    harness.snapshot(shot("agent_chat_settings").as_str());
}

/// Everything a person can do here an agent can do too, through the same code.
#[test]
fn the_chat_answers_the_command_line() {
    // **Both stand-in agents on `PATH`**, because since `task-2003` a row Unluminous ships for an agent
    // that is not installed is not offered — see `Configuration::forget_the_agents_that_are_not_installed`.
    // Without this the list of endpoints would be a fact about the machine the test is running on.
    agents_on_the_path();
    let mut harness = harness("");
    let shown = did(&mut harness, "plugins pane agent-chat/chat --show");
    assert_eq!(shown["showing"], true);

    // Which endpoints there are, what each is, and whether it could be reached. Never a key.
    let providers = did(&mut harness, "plugins run agent-chat providers");
    let rows = providers["providers"].as_array().expect("the endpoints");
    let names: Vec<&str> = rows.iter().filter_map(|one| one["name"].as_str()).collect();
    assert_eq!(names, ["claude", "codex", "local"]);
    // **The two rows the ticket names run the command line, not the API.** *"connection to Claude
    // and codex etc through cli"* — so what a row holds is a program, and Unluminous holds no key for
    // either of them.
    assert_eq!(rows[0]["wire"], "claude-cli");
    assert_eq!(rows[0]["command"], "claude");
    assert_eq!(rows[0]["runs_a_program"], true);
    assert_eq!(rows[0]["key"], false);
    assert_eq!(rows[0]["key_env"], "");
    assert_eq!(rows[1]["wire"], "codex-cli");
    assert_eq!(rows[1]["command"], "codex");
    // The third is an address, so the ticket's "configure url" has something to configure and a
    // machine with neither agent installed can still be pointed at an llama.cpp.
    assert_eq!(rows[2]["wire"], "openai");
    assert_eq!(rows[2]["runs_a_program"], false);
    assert_eq!(rows[2]["url"], "http://127.0.0.1:8080/v1/chat/completions");
    // What an agent may do without asking, which is the setting that replaced the key. `full` since
    // `task-2003` — *"Agent chat should have full by default"* — and see `Permission::Full` for why.
    assert_eq!(providers["permission"], "full");
    let printed = serde_json::to_string(&providers).expect("json");
    assert!(!printed.contains("sk-"), "no key is ever printed: {printed}");

    // Choosing one, and one nobody has.
    did(&mut harness, "plugins run agent-chat use local");
    assert_eq!(did(&mut harness, "plugins view agent-chat")["provider"]["name"], "local");
    let reply = run(&mut harness, "plugins run agent-chat use gemini");
    assert!(!reply.ok);
    assert!(reply.message.contains("claude"), "the refusal lists what there is: {}", reply.message);

    // The tools switch, which is the same switch the composer's first button is.
    assert_eq!(did(&mut harness, "plugins run agent-chat tools on")["tools"], true);
    assert_eq!(did(&mut harness, "plugins view agent-chat")["tools"], true);
    assert_eq!(did(&mut harness, "plugins run agent-chat tools off")["tools"], false);

    // The conversation as data, which is what a screenshot cannot answer.
    with_the_chat(&mut harness, |chat| {
        chat.session_mut().ask(unluminous_chat::Message::said(
            0,
            unluminous_chat::Role::User,
            "Hello",
        ));
        chat.session_mut().reply(unluminous_chat::Reply::Text("Hello yourself.".to_owned()));
        chat.session_mut().reply(unluminous_chat::Reply::Finished { reason: "stop".to_owned() });
    });
    let messages = did(&mut harness, "plugins run agent-chat messages");
    assert_eq!(messages["messages"][0]["text"], "Hello");
    assert_eq!(messages["messages"][1]["text"], "Hello yourself.");
    assert_eq!(did(&mut harness, "plugins run agent-chat last")["text"], "Hello yourself.");
    assert_eq!(did(&mut harness, "plugins run agent-chat state")["state"], "finished");

    // A new conversation, and a command nobody has.
    did(&mut harness, "plugins run agent-chat new");
    assert_eq!(
        did(&mut harness, "plugins run agent-chat messages")["messages"].as_array().map(Vec::len),
        Some(0)
    );
    let reply = run(&mut harness, "plugins run agent-chat levitate");
    assert!(!reply.ok);
    assert!(reply.message.contains("levitate"), "{}", reply.message);
}

/// Sending to a row that cannot answer refuses before anything is started, and says what to do.
#[test]
fn sending_to_an_endpoint_that_cannot_answer_is_refused_before_anything_is_sent() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "plugins run agent-chat use claude");
    with_the_chat(&mut harness, |chat| {
        chat.configuration_mut().providers[0].command = NO_SUCH_AGENT.to_owned();
    });
    let reply = run(&mut harness, "plugins run agent-chat send Are you there?");
    assert!(!reply.ok);
    assert!(
        reply.message.contains(NO_SUCH_AGENT),
        "the refusal names the program it would have run: {}",
        reply.message
    );
    // Nothing went into the conversation, so the question is still there to be sent once a key is set.
    assert_eq!(
        did(&mut harness, "plugins run agent-chat messages")["messages"].as_array().map(Vec::len),
        Some(0)
    );
}

/// An endpoint that sends to an address and needs no key, pointed at a port nothing answers.
///
/// It gets `send` past its own checks. **No test using it ever reaches a request**: each one is
/// already busy, so what `send` does is queue.
fn an_endpoint_that_needs_no_key(harness: &mut Harness<'static, UnluminousApp>) {
    with_the_chat(harness, |chat| {
        let row = &mut chat.configuration_mut().providers[0];
        row.name = "stand-in".to_owned();
        row.wire = unluminous_chat::Wire::OpenAi;
        row.command = String::new();
        row.url = "http://127.0.0.1:1/v1/chat/completions".to_owned();
        row.model = "a-model".to_owned();
        row.key_env = String::new();
    });
    did(harness, "plugins run agent-chat use stand-in");
}

/// `task-2060`: a question asked while an answer is arriving waits its turn, and is on the screen at
/// once.
///
/// *"I should be able to send new messages that get added to the queue when the agent is working. I
/// should see my message immediately posted after I send it."*
#[test]
fn a_question_sent_while_an_answer_is_arriving_is_queued_and_drawn_at_once() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    steady(&mut harness);
    an_endpoint_that_needs_no_key(&mut harness);
    // A turn in flight, driven through the session so nothing is put on a wire.
    with_the_chat(&mut harness, |chat| {
        let id = chat.session_mut().chat.next_id();
        chat.session_mut().ask(unluminous_chat::Message::said(
            id,
            unluminous_chat::Role::User,
            "Why is `relayout` keeping a paragraph it should have thrown away?",
        ));
        chat.session_mut().reply(unluminous_chat::Reply::Text("Because the".to_owned()));
    });
    let state = did(&mut harness, "plugins run agent-chat state");
    assert_eq!(state["busy"], true, "{state}");

    let sent = did(&mut harness, "plugins run agent-chat send And the fingerprint?");
    assert_eq!(sent["queued"], true, "it waits its turn rather than being refused: {sent}");
    assert_eq!(sent["waiting"], 1);
    steady(&mut harness);

    // It is a row in the conversation area the moment it is sent, which is the other half of the
    // ask: the question is on the screen before it has been asked of anything. The picture below is
    // what says so; `view` is what an agent driving the pane reads.
    let view = did(&mut harness, "plugins view agent-chat");
    let queued = view["queued"].as_array().expect("the queue is reported as data");
    assert_eq!(queued.len(), 1, "{view}");
    assert_eq!(queued[0]["text"], "And the fingerprint?");
    assert_eq!(view["draft"], "", "the composer is empty, as it is for a message that went");
    harness.snapshot(shot("agent_chat_queued").as_str());

    // Stop means stop, including what is waiting — and the words go back where they were typed.
    did(&mut harness, "plugins run agent-chat stop");
    steady(&mut harness);
    let after = did(&mut harness, "plugins view agent-chat");
    assert_eq!(after["queued"].as_array().map(Vec::len), Some(0), "{after}");
    assert_eq!(after["draft"], "And the fingerprint?", "nothing somebody typed is thrown away");
}

/// `task-2060`: the words of a message can be selected and copied.
///
/// The drag itself is `unluminous-cli input drag`, which `tasks/task-1914-testing-without-stealing-focus-tdd.md`
/// exists for; what is asserted here is the half a drag cannot show — that what comes back is the
/// **rendered** words rather than the markdown source, which is what somebody dragged across.
#[test]
fn selecting_a_whole_message_answers_with_the_words_as_they_are_drawn() {
    let mut harness = a_chat(&[
        (unluminous_chat::Role::User, "Why?"),
        (unluminous_chat::Role::Assistant, "## Because\n\nthe fingerprint does not carry it."),
    ]);
    steady(&mut harness);
    let mut selected = None;
    with_the_chat(&mut harness, |chat| {
        let id = chat.chat().messages.last().expect("the answer").id;
        chat.select_the_whole_message(id);
        selected = chat.selected_text();
    });
    let selected = selected.expect("the whole of the answer");
    assert!(selected.contains("Because"), "{selected}");
    assert!(selected.contains("the fingerprint does not carry it."), "{selected}");
    assert!(!selected.contains('#'), "the words as they are drawn, not the markdown: {selected}");
}

/// Enter sends what has been typed, and Shift+Enter does not.
///
/// Driven through the real field rather than by calling `send`, because what this is about is the key
/// press: `consume_key` matches by `Modifiers::matches_logically` and would take `Shift+Enter` for a
/// pattern of `NONE`, which is the trap `task-1678` and `task-1682` each recorded. The endpoint has no
/// key, so the send is refused before a request goes out and **no test here reaches a network**.
#[test]
fn enter_in_the_composer_sends_and_shift_enter_does_not() {
    let mut harness = harness("");
    did(&mut harness, "plugins pane agent-chat/chat --show");
    did(&mut harness, "plugins run agent-chat use claude");
    with_the_chat(&mut harness, |chat| {
        chat.configuration_mut().providers[0].command = NO_SUCH_AGENT.to_owned();
    });
    harness.get_by_label("Message").click();
    steady(&mut harness);
    harness.get_by_label("Message").type_text("Are you there?");
    steady(&mut harness);
    assert_eq!(
        did(&mut harness, "plugins view agent-chat")["draft"],
        "Are you there?",
        "what was typed reached the draft"
    );

    // Shift+Enter is a new line: nothing is sent and nothing is refused.
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Enter);
    steady(&mut harness);
    let after = did(&mut harness, "plugins view agent-chat");
    assert!(after["problem"].is_null(), "shift+enter did not try to send: {after}");
    assert_eq!(after["messages"], 0);

    // Enter sends, which here is refused before anything is started — and the refusal names the
    // program it would have run, which is what makes it a useful refusal.
    harness.key_press(egui::Key::Enter);
    steady(&mut harness);
    let after = did(&mut harness, "plugins view agent-chat");
    let problem = after["problem"].as_str().expect("the refusal reached the pane");
    assert!(problem.contains(NO_SUCH_AGENT), "{problem}");
    // And what was typed is still there, because a refusal must not eat somebody's question.
    assert!(
        after["draft"].as_str().expect("the draft").contains("Are you there?"),
        "the refusal ate the draft: {after}"
    );
}
