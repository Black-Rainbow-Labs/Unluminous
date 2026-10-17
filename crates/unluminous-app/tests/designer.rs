//! The Designer in a real window: a file in the designs folder opens in a tab of the Designer pane, the page is
//! placed at the width of a device, what the page reports reaches the notes, and handing a note to the agent
//! writes the copy of the design and queues a message in Agent-Chat. `task-2254`. The pane's own tabs, the blank
//! tab and the tabs coming back after a restart are `task-2260`.
//!
//! **No page is drawn and none is needed.** A native web view is in no screenshot Unluminous takes, and the
//! harness has no window for one to live in, so what these tests check is everything around it: the tab, the
//! rectangle the page is asked to fill, and what the window does with a report from the page, which is fed in
//! through `act_on_browser_events`, the seam `task-1905` added for exactly this. The page itself is checked in
//! the real window with `tools/capture-window.ps1`.

mod common;

use common::*;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::services::browser::BrowserEvent;
use unluminous_app::services::designer::Designer;
use unluminous_app::UnluminousApp;

const DESIGN: &str = "<!doctype html>\n<html><body><h1 class=\"hero-title\">Ship designs</h1><p>Hello</p></body></html>\n";

/// A project with one design in `.designs` and one HTML file outside it.
fn a_project(name: &str) -> std::path::PathBuf {
    fixture(
        name,
        &[
            (".designs/landing.html", DESIGN),
            ("index.html", "<p>an ordinary page</p>"),
            ("readme.md", "# A project\n"),
        ],
    )
}

/// A window with the design open in a tab of the Designer pane.
fn a_window_on_a_design(name: &str) -> (std::path::PathBuf, Harness<'static, UnluminousApp>) {
    let folder = a_project(name);
    let mut harness = harness_in(&folder);
    // Wide enough for the explorer, the Designer pane at its own width and the editing area beside them.
    harness.set_size(egui::vec2(1700.0, 900.0));
    steady(&mut harness);
    did(&mut harness, "tab open .designs/landing.html");
    steady(&mut harness);
    (folder, harness)
}

/// The browser tab the page is placed for.
fn the_pages_id(harness: &Harness<'static, UnluminousApp>) -> u64 {
    harness.state().browser_placements().first().expect("the page is placed").id
}

/// Hand the window what the page's script would have reported.
fn the_page_reports(harness: &mut Harness<'static, UnluminousApp>, reports: serde_json::Value) {
    let id = the_pages_id(harness);
    let json = serde_json::json!({ "reports": reports }).to_string();
    harness.state_mut().act_on_browser_events(vec![BrowserEvent::Reported { id, json }]);
    steady(harness);
}

fn the_designer_says(harness: &mut Harness<'static, UnluminousApp>) -> serde_json::Value {
    did(harness, "plugins view designer")
}

/// The Designer pane's tabs, as `plugins run designer tabs` answers them.
fn the_tabs(harness: &mut Harness<'static, UnluminousApp>) -> serde_json::Value {
    did(harness, "plugins run designer tabs")
}

#[test]
fn a_file_in_the_designs_folder_opens_in_the_designer_pane_and_one_outside_it_does_not() {
    let folder = a_project("unluminous-designer-opens");
    let mut harness = harness_in(&folder);
    let before = harness.state().files.len();
    did(&mut harness, "tab open .designs/landing.html");
    steady(&mut harness);
    // `task-2260`: the design is a tab of the Designer pane, not a tab of the editing area.
    assert_eq!(harness.state().files.len(), before, "no tab was added to the editing area");
    assert!(harness.state().files.active().plugin.is_none());
    let tabs = the_tabs(&mut harness);
    assert_eq!(tabs["tabs"], serde_json::json!(["landing.html"]), "the blank tab took the design");
    assert_eq!(tabs["active"], 0);
    assert_eq!(the_designer_says(&mut harness)["showing"], "landing.html");
    assert_eq!(did(&mut harness, "plugins pane designer --show")["showing"], true);

    // Opening it again shows the tab rather than making a second one.
    did(&mut harness, "tab open .designs/landing.html");
    steady(&mut harness);
    assert_eq!(the_tabs(&mut harness)["tabs"].as_array().map(Vec::len), Some(1));

    // An HTML file anywhere else opens as it always did.
    did(&mut harness, "tab open index.html");
    steady(&mut harness);
    assert!(harness.state().files.active().plugin.is_none());
    assert_eq!(harness.state().files.active().path(), Some(folder.join("index.html").as_path()));
}

/// `task-2260`: *"It should have a blank tab showing by default, and remember which tabs/files are open between
/// restarts."* The pane starts on a blank tab that lists the designs, `New tab`, a tab's pill and its cross are
/// buttons, and a second window on the project opens the same tabs on the same one.
#[test]
fn the_designer_pane_has_its_own_tabs_starting_blank_and_remembered_between_windows() {
    let folder = fixture(
        "unluminous-designer-tabs",
        &[
            (".designs/landing.html", DESIGN),
            (".designs/pricing.html", DESIGN),
            ("readme.md", "# A project\n"),
        ],
    );
    let mut harness = harness_in(&folder);
    did(&mut harness, "plugins pane designer --show");
    steady(&mut harness);
    let tabs = the_tabs(&mut harness);
    assert_eq!(tabs["tabs"], serde_json::json!([null]), "one blank tab to begin with");
    assert!(harness.state().browser_placements().is_empty(), "a blank tab has no page");
    harness.get_by_label("Tab: New tab");
    // The blank tab is the list of designs, and a row opens its design in the same tab.
    harness.get_by_label("landing.html").click();
    steady(&mut harness);
    steady(&mut harness);
    assert_eq!(the_tabs(&mut harness)["tabs"], serde_json::json!(["landing.html"]));
    assert_eq!(harness.state().browser_placements().len(), 1, "the design's page is placed");

    // `New tab` adds a blank tab after the one showing, and a design opened from it takes that tab.
    harness.get_by_label("New tab").click();
    steady(&mut harness);
    assert_eq!(the_tabs(&mut harness)["tabs"], serde_json::json!(["landing.html", null]));
    assert!(harness.state().browser_placements().is_empty(), "the blank tab is showing");
    harness.get_by_label("pricing.html").click();
    steady(&mut harness);
    steady(&mut harness);
    let tabs = the_tabs(&mut harness);
    assert_eq!(tabs["tabs"], serde_json::json!(["landing.html", "pricing.html"]));
    assert_eq!(tabs["active"], 1);

    // A tab's pill shows it, and the agent's half does the same.
    harness.get_by_label("Tab: landing.html").click();
    steady(&mut harness);
    assert_eq!(the_tabs(&mut harness)["active"], 0);
    assert_eq!(the_designer_says(&mut harness)["showing"], "landing.html");
    did(&mut harness, "plugins run designer show-tab pricing");
    steady(&mut harness);
    assert_eq!(the_designer_says(&mut harness)["showing"], "pricing.html");

    // The tabs are written down for the project, so the next window opens them again.
    let written = std::fs::read_to_string(folder.join(".unluminous/designs.conf")).unwrap();
    assert!(written.contains("tabs.1 = .designs/pricing.html"), "{written}");
    let mut again = harness_in(&folder);
    did(&mut again, "plugins pane designer --show");
    steady(&mut again);
    steady(&mut again);
    let tabs = the_tabs(&mut again);
    assert_eq!(tabs["tabs"], serde_json::json!(["landing.html", "pricing.html"]));
    assert_eq!(tabs["active"], 1, "on the tab that was showing");
    assert_eq!(again.state().browser_placements().len(), 1, "and its page is placed");

    // Closing tabs by their cross: the last one closed leaves a blank tab, and the pages are let go of.
    harness.get_by_label("Close tab: pricing.html").click();
    steady(&mut harness);
    assert_eq!(the_tabs(&mut harness)["tabs"], serde_json::json!(["landing.html"]));
    did(&mut harness, "plugins run designer close-tab");
    steady(&mut harness);
    assert_eq!(the_tabs(&mut harness)["tabs"], serde_json::json!([null]));
    assert!(harness.state().browser_placements().is_empty());
    assert_eq!(the_designer_says(&mut harness)["open"].as_array().map(Vec::len), Some(0));
    assert_eq!(refused(&mut harness, "plugins run designer show-tab 4"), "failed");

    // A design that was deleted is not brought back as a tab.
    std::fs::write(
        folder.join(".unluminous/designs.conf"),
        "tabs.count = 2\ntabs.active = 1\ntabs.0 = .designs/gone.html\ntabs.1 = .designs/landing.html\n",
    )
    .unwrap();
    let mut third = harness_in(&folder);
    let tabs = the_tabs(&mut third);
    assert_eq!(tabs["tabs"], serde_json::json!(["landing.html"]));
    assert_eq!(tabs["active"], 0);
}

#[test]
fn the_page_is_placed_in_the_well_at_the_width_of_the_device() {
    let (_, mut harness) = a_window_on_a_design("unluminous-designer-placement");
    let placed = harness.state().browser_placements();
    assert_eq!(placed.len(), 1, "one page is placed for one design tab");
    assert_eq!(placed[0].zoom, Some(1.0), "Fit is not zoomed");
    assert!(placed[0].area.width() > 300.0, "Fit is the width of the well: {:?}", placed[0].area);

    did(&mut harness, "plugins run designer device phone");
    steady(&mut harness);
    let phone = harness.state().browser_placements()[0].clone();
    assert_eq!(phone.area.width(), 390.0, "a phone is laid out 390 wide");
    assert_eq!(phone.zoom, Some(1.0));
    assert_eq!(the_designer_says(&mut harness)["open"][0]["device"], "phone");

    // A desktop does not fit beside the notes, so it is zoomed down and still laid out 1440 wide.
    did(&mut harness, "plugins run designer device desktop");
    steady(&mut harness);
    let desktop = harness.state().browser_placements()[0].clone();
    let zoom = desktop.zoom.expect("a zoom is sent");
    assert!((0.25..1.0).contains(&zoom), "{zoom}");
    assert!(
        (f64::from(desktop.area.width()) / zoom - 1440.0).abs() < 2.0,
        "{:?} at {zoom}",
        desktop.area
    );
}

#[test]
fn what_the_page_reports_reaches_the_selection_the_composer_and_the_detached_marks() {
    let (_, mut harness) = a_window_on_a_design("unluminous-designer-reports");
    let element = |selector: &str| {
        serde_json::json!({ "selector": selector, "tag": "h1.hero-title", "excerpt": "Ship designs",
            "html": "<h1>", "styles": { "font-size": "64px" }, "rect": { "x": 10, "y": 20, "w": 300, "h": 60 } })
    };
    the_page_reports(
        &mut harness,
        serde_json::json!([{ "kind": "selected", "element": element("h1.hero-title") }]),
    );
    let said = the_designer_says(&mut harness);
    assert_eq!(said["open"][0]["selection"]["selector"], "h1.hero-title");
    assert_eq!(
        did(&mut harness, "plugins run designer selection")["selection"]["tag"],
        "h1.hero-title"
    );

    the_page_reports(
        &mut harness,
        serde_json::json!([{ "kind": "comment", "element": element("p.lede"), "fx": 0.5, "fy": 0.1 }]),
    );
    assert_eq!(the_designer_says(&mut harness)["open"][0]["composing"], "p.lede");

    // A note whose element the page could not find is marked detached.
    did(&mut harness, "plugins run designer comment h1.hero-title Too loud.");
    the_page_reports(
        &mut harness,
        serde_json::json!([{ "kind": "anchors", "found": [], "missing": [1] }]),
    );
    let said = the_designer_says(&mut harness);
    assert_eq!(said["open"][0]["detached"], serde_json::json!([1]));
    assert_eq!(said["open"][0]["notes"][0]["detached"], true);

    // And a click on a pin opens its thread.
    the_page_reports(&mut harness, serde_json::json!([{ "kind": "pin", "id": 1 }]));
    assert_eq!(the_designer_says(&mut harness)["open"][0]["active"], 1);
    the_page_reports(&mut harness, serde_json::json!([{ "kind": "cleared" }]));
    assert!(the_designer_says(&mut harness)["open"][0]["selection"].is_null());
}

/// An endpoint that needs no key, and a conversation with an answer arriving, so `send` queues rather than
/// starting anything: **no test here reaches a network**, which is how `plugins_board_and_chat.rs` does it.
fn the_chat_is_busy(harness: &mut Harness<'static, UnluminousApp>) {
    did(harness, "plugins pane agent-chat/chat --show");
    let provider = harness
        .state_mut()
        .plugin_ui
        .opened("agent-chat", "agent-chat")
        .expect("the chat provider");
    let chat = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_chat::AgentChat>())
        .expect("the chat as its own type");
    let row = &mut chat.configuration_mut().providers[0];
    row.name = "stand-in".to_owned();
    row.wire = unluminous_chat::Wire::OpenAi;
    row.command = String::new();
    row.url = "http://127.0.0.1:1/v1/chat/completions".to_owned();
    row.model = "a-model".to_owned();
    row.key_env = String::new();
    did(harness, "plugins run agent-chat use stand-in");
    let provider = harness
        .state_mut()
        .plugin_ui
        .opened("agent-chat", "agent-chat")
        .expect("the chat provider");
    let chat = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<unluminous_app::services::agent_chat::AgentChat>())
        .expect("the chat as its own type");
    let id = chat.session_mut().chat.next_id();
    chat.session_mut().ask(unluminous_chat::Message::said(
        id,
        unluminous_chat::Role::User,
        "An earlier question",
    ));
    chat.session_mut().reply(unluminous_chat::Reply::Text("Because the".to_owned()));
    harness.step();
    harness.step();
}

#[test]
fn asking_about_a_note_copies_the_design_and_queues_the_request_in_agent_chat() {
    let (folder, mut harness) = a_window_on_a_design("unluminous-designer-ask");
    the_chat_is_busy(&mut harness);
    let made = did(
        &mut harness,
        "plugins run designer comment main > h1.hero-title -- Too loud. Try 48px.",
    );
    assert_eq!(made["note"], 1);
    did(&mut harness, "plugins run designer ask 1");
    for _ in 0..4 {
        harness.step();
    }

    // The design was copied before the request went.
    let copies: Vec<_> = std::fs::read_dir(folder.join(".unluminous/designs/landing"))
        .expect("the history folder")
        .flatten()
        .collect();
    assert_eq!(copies.len(), 1);
    assert!(
        copies[0].file_name().to_string_lossy().ends_with("-too-loud-try-48px.html"),
        "{copies:?}"
    );
    assert_eq!(std::fs::read_to_string(copies[0].path()).unwrap(), DESIGN);

    // The request is in Agent-Chat's queue, exactly as `plugins run agent-chat send` would have put it.
    let view = did(&mut harness, "plugins view agent-chat");
    let queued = view["queued"].as_array().expect("the queue is data");
    assert_eq!(queued.len(), 1, "{view}");
    let text = queued[0]["text"].as_str().unwrap();
    assert!(text.contains("Design: .designs/landing.html"), "{text}");
    assert!(text.contains("Note 1 on"), "{text}");
    assert!(text.contains("Too loud. Try 48px."), "{text}");
    assert!(text.contains("designer reply --design .designs/landing.html 1"), "{text}");

    // The note says it was handed over, and the notes file is the person's own to commit.
    let notes = std::fs::read_to_string(folder.join(".designs/landing.comments.json")).unwrap();
    assert!(notes.contains("Sent to the agent"), "{notes}");
    assert!(notes.contains("main > h1.hero-title"), "{notes}");
}

#[test]
fn open_source_opens_the_file_as_text_and_the_designs_pane_lists_the_design() {
    let (folder, mut harness) = a_window_on_a_design("unluminous-designer-source");
    did(&mut harness, "plugins run designer source");
    steady(&mut harness);
    steady(&mut harness);
    let active = harness.state().files.active();
    assert_eq!(active.path(), Some(folder.join(".designs/landing.html").as_path()));
    assert!(active.plugin.is_none(), "the source is text, not a second design tab");

    let designs = did(&mut harness, "plugins run designer designs");
    assert_eq!(designs["designs"][0]["name"], "landing.html");
    let shown = did(&mut harness, "plugins pane designer/designs --show");
    assert_eq!(shown["showing"], true);
}

/// `task-2255`: *"I create new design, name it, the file is created, then i try clicking it in the list of designs
/// and nothing happens."* Since `task-2260` a design opens in the Designer pane, so opening one puts the pane on
/// the screen and leaves a hidden editing area hidden. The pane is named by its plugin's id alone.
#[test]
fn opening_a_design_puts_the_designer_pane_on_the_screen() {
    let folder = a_project("unluminous-designer-hidden-editor");
    let mut harness = harness_in(&folder);
    let shown = did(&mut harness, "plugins pane designer --show");
    assert_eq!(shown["pane"], "designer/designs", "the plugin's id names its one pane");
    did(&mut harness, "plugins pane designer --hide");
    harness.state_mut().editor_visible = false;
    steady(&mut harness);
    did(&mut harness, "plugins run designer open landing");
    steady(&mut harness);
    steady(&mut harness);
    assert!(!harness.state().editor_visible, "the editing area is not needed for a design");
    assert_eq!(did(&mut harness, "plugins pane designer --show")["showing"], true);
    assert_eq!(the_tabs(&mut harness)["tabs"], serde_json::json!(["landing.html"]));
    assert_eq!(harness.state().browser_placements().len(), 1);
}

#[test]
fn the_provider_is_reachable_as_its_own_type_the_way_the_buttons_reach_it() {
    let (_, mut harness) = a_window_on_a_design("unluminous-designer-own-type");
    let designer: &mut Designer = harness.state_mut().designer().expect("the designer");
    let design = std::path::PathBuf::from(".designs/landing.html");
    let id =
        designer.comment(&design, "h1", "From the button's own function.", "you").expect("a note");
    assert_eq!(id, 1);
    assert_eq!(the_designer_says(&mut harness)["designs"][0]["open"], 1);
}

#[test]
fn the_explorer_lists_the_designs_folder_and_the_one_settings_names() {
    let folder = fixture(
        "unluminous-designer-explorer",
        &[(".designs/landing.html", DESIGN), (".mocks/home.html", DESIGN), ("readme.md", "# x\n")],
    );
    let mut harness = harness_in(&folder);
    did(&mut harness, "plugins run designer designs");
    steady(&mut harness);
    let tree = did(&mut harness, "explorer tree").to_string();
    assert!(tree.contains(".designs"), "the plugin's own dot folder is listed: {tree}");
    assert!(!tree.contains(".mocks"), "and another dot folder is not, yet: {tree}");

    // Settings point the Designer at another dot folder, and the explorer is told in the next frames.
    let designer = harness.state_mut().designer().expect("the designer");
    designer.config.folder = std::path::PathBuf::from(".mocks");
    designer.write_the_configuration().expect("written");
    for _ in 0..3 {
        steady(&mut harness);
    }
    let tree = did(&mut harness, "explorer tree").to_string();
    assert!(tree.contains(".mocks"), "the folder the settings name is listed: {tree}");

    // And what opens as a design follows the folder: the old one is an ordinary page now.
    did(&mut harness, "tab open .designs/landing.html");
    assert!(harness.state().files.active().plugin.is_none(), "no longer a design");
    assert!(the_tabs(&mut harness)["tabs"]
        .as_array()
        .is_some_and(|tabs| tabs.iter().all(|tab| tab.is_null())));
    did(&mut harness, "tab open .mocks/home.html");
    steady(&mut harness);
    assert_eq!(the_tabs(&mut harness)["tabs"], serde_json::json!(["home.html"]));
}

#[test]
fn closing_a_design_tab_lets_go_of_its_page_and_switching_the_plugin_off_closes_it() {
    let (_, mut harness) = a_window_on_a_design("unluminous-designer-close");
    assert_eq!(harness.state().browser_placements().len(), 1);
    did(&mut harness, "plugins run designer close-tab landing");
    steady(&mut harness);
    assert!(harness.state().browser_placements().is_empty(), "no page is placed for a closed tab");
    assert_eq!(harness.state().design_pages(), 0, "and the page is let go of");
    assert_eq!(the_designer_says(&mut harness)["open"].as_array().map(Vec::len), Some(0));

    did(&mut harness, "tab open .designs/landing.html");
    steady(&mut harness);
    assert_eq!(harness.state().browser_placements().len(), 1);
    did(&mut harness, "plugins disable designer");
    steady(&mut harness);
    steady(&mut harness);
    assert!(harness.state().browser_placements().is_empty(), "a design goes with its plugin");
    assert_eq!(harness.state().design_pages(), 0);
}

#[test]
fn the_toolbar_and_the_pane_are_controls_a_person_can_press() {
    let (folder, mut harness) = a_window_on_a_design("unluminous-designer-controls");

    // The mode switch and the device switch are pills, named for what they set.
    harness.get_by_label("Mode: Comment").click();
    steady(&mut harness);
    assert_eq!(the_designer_says(&mut harness)["open"][0]["mode"], "comment");
    harness.get_by_label("Device: Phone").click();
    steady(&mut harness);
    assert_eq!(harness.state().browser_placements()[0].area.width(), 390.0);

    // A narrower pane makes the five device pills one button that cycles.
    did(&mut harness, "panel size designer/designs --width 640");
    steady(&mut harness);
    harness.get_by_label("Phone").click();
    steady(&mut harness);
    assert_eq!(the_designer_says(&mut harness)["open"][0]["device"], "fit", "the one after phone");

    // History opens the dialog, and it says there is nothing yet.
    harness.get_by_label("History").click();
    steady(&mut harness);
    assert!(harness.state_mut().designer().unwrap().modal.is_some());
    harness.get_by_label("Done").click();
    steady(&mut harness);
    assert!(harness.state_mut().designer().unwrap().modal.is_none());

    // A blank tab lists the designs with a chip that asks for a name, and Create makes the design and opens it in
    // that tab.
    harness.get_by_label("New tab").click();
    steady(&mut harness);
    harness.get_by_label("New Design").click();
    steady(&mut harness);
    assert!(harness.state_mut().designer().unwrap().modal.is_some(), "a name is asked for");
    harness.state_mut().designer().unwrap().modal =
        Some(unluminous_app::services::designer::Modal::NewDesign { name: "Pricing".to_owned() });
    steady(&mut harness);
    harness.get_by_label("Create").click();
    for _ in 0..4 {
        steady(&mut harness);
    }
    assert!(folder.join(".designs/Pricing.html").is_file());
    assert_eq!(the_tabs(&mut harness)["tabs"], serde_json::json!(["landing.html", "Pricing.html"]));
}

#[test]
fn a_design_tab_the_editing_area_remembered_comes_back_in_the_designer_pane() {
    // `plugin-tabs.txt` is what the released binary wrote for every plugin tab, and a design tab in the editing area,
    // from before the Designer pane had tabs of its own, names the design it showed. A window opened on the project
    // shows the design again, in the pane (`task-2260`), with its page and its notes.
    let folder = a_project("unluminous-designer-session");
    std::fs::create_dir_all(folder.join(".unluminous")).expect("the project's own folder");
    std::fs::write(folder.join(".unluminous/plugin-tabs.txt"), "designer:.designs/landing.html\n")
        .expect("what a previous window left");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    steady(&mut harness);
    steady(&mut harness);
    assert!(harness.state().files.active().plugin.is_none(), "not a tab of the editing area");
    assert_eq!(the_tabs(&mut harness)["tabs"], serde_json::json!(["landing.html"]));
    assert_eq!(harness.state().browser_placements().len(), 1, "and its page is placed");
    assert_eq!(the_designer_says(&mut harness)["showing"], "landing.html");

    // A design that has gone is not a reason to refuse to open the rest.
    std::fs::write(folder.join(".unluminous/plugin-tabs.txt"), "designer:.designs/gone.html\n")
        .expect("a design that was deleted");
    let mut harness = harness_in(&folder);
    harness.state_mut().restore_project();
    steady(&mut harness);
}

// ====================================================================================================
// The pictures. A native page is in no screenshot, so each one shows the carved well where the page is, the
// toolbar, the notes and the prompt box; the mocks in `_agent_output/task-2254-designer/mocks/` are what they
// are compared against by eye.

/// Three notes, written with fixed times so a picture is the same on every run. Note 1 is by the person and has
/// the agent's reply, note 2 is the person's and asks for a button change, note 3 is on a feature card.
const THREE_NOTES: &str = r#"{
  "version": 1, "design": "landing.html", "next": 4,
  "notes": [
    { "id": 1, "state": "open",
      "anchor": { "selector": "main > section.hero > h1.hero-title", "tag": "h1.hero-title",
                  "excerpt": "Analytics your whole team will actually open.", "fx": 1.0, "fy": 0.0,
                  "page": { "x": 560.0, "y": 90.0 }, "device": "fit" },
      "messages": [
        { "author": "you", "body": "Too long. Make it punchier and keep it to two lines at this width.", "at": "2026-01-05T14:32:00Z" },
        { "author": "claude", "body": "Shortened to \"Analytics your team opens.\"\nThe last two words use the accent colour now. See `.hero-title` in the `<style>` block.", "at": "2026-01-05T14:33:00Z" }
      ], "created": "2026-01-05T14:32:00Z", "resolved": null },
    { "id": 2, "state": "open",
      "anchor": { "selector": "a.btn-primary", "tag": "a.btn-primary", "excerpt": "Start free trial",
                  "fx": 1.0, "fy": 0.0, "page": { "x": 400.0, "y": 300.0 }, "device": "fit" },
      "messages": [ { "author": "you", "body": "The button feels weak next to the heading. Make it larger with a softer shadow.", "at": "2026-01-05T14:20:00Z" } ],
      "created": "2026-01-05T14:20:00Z", "resolved": null },
    { "id": 3, "state": "open",
      "anchor": { "selector": "div.feature-card", "tag": "div.feature-card", "excerpt": "Weekly digest",
                  "fx": 1.0, "fy": 0.0, "page": { "x": 300.0, "y": 600.0 }, "device": "fit" },
      "messages": [ { "author": "you", "body": "The icons are all the same colour. Give each card its own tint.", "at": "2026-01-05T14:10:00Z" } ],
      "created": "2026-01-05T14:10:00Z", "resolved": null },
    { "id": 4, "state": "resolved",
      "anchor": { "selector": "footer", "tag": "footer", "excerpt": "", "fx": 1.0, "fy": 0.0,
                  "page": { "x": 0.0, "y": 900.0 }, "device": "fit" },
      "messages": [ { "author": "you", "body": "Footer links are too small.", "at": "2026-01-04T10:00:00Z" } ],
      "created": "2026-01-04T10:00:00Z", "resolved": "2026-01-04T11:00:00Z" }
  ]
}"#;

/// A project with a design, three open notes, two other designs and three copies of the design.
fn a_busy_project(name: &str) -> std::path::PathBuf {
    let copy = "<!doctype html><p>an earlier copy</p>";
    fixture(
        name,
        &[
            (".designs/landing.html", DESIGN),
            (".designs/landing.comments.json", THREE_NOTES),
            (".designs/pricing.html", DESIGN),
            (".designs/onboarding.html", DESIGN),
            (".designs/pages/checkout.html", DESIGN),
            (".unluminous/designs/landing/20260105-143200-note-1-on-h1-hero-title-too-long.html", copy),
            (".unluminous/designs/landing/20260105-140500-send-open-notes-3-1-h1-hero-title.html", copy),
            (".unluminous/designs/landing/20260105-134800-make-the-hero-darker.html", copy),
            ("readme.md", "# A project\n"),
        ],
    )
}

fn a_chosen_element() -> serde_json::Value {
    serde_json::json!({ "selector": "main > section.hero > div.hero-copy > h1.hero-title",
        "tag": "h1.hero-title", "excerpt": "Analytics your whole team will actually open.",
        "html": "<h1 class=\"hero-title\">Analytics your whole team will actually open.</h1>",
        "styles": { "font-size": "62px", "font-weight": "800" }, "rect": { "x": 40, "y": 80, "w": 520, "h": 190 } })
}

/// The design open in the Designer pane with its notes read, an element chosen and note 1 open in the sidebar.
fn the_busy_tab(name: &str) -> Harness<'static, UnluminousApp> {
    let folder = a_busy_project(name);
    let mut harness = harness_in(&folder);
    did(&mut harness, "tab open .designs/landing.html");
    steady(&mut harness);
    the_page_reports(
        &mut harness,
        serde_json::json!([{ "kind": "selected", "element": a_chosen_element() }]),
    );
    the_page_reports(&mut harness, serde_json::json!([{ "kind": "pin", "id": 1 }]));
    harness
}

#[test]
fn designer_tab() {
    let mut harness = the_busy_tab("unluminous-designer-shot-tab");
    did(&mut harness, "plugins run designer mode comment");
    did(&mut harness, "plugins run designer device desktop");
    steady(&mut harness);
    harness.snapshot(shot("designer_tab").as_str());
}

#[test]
fn designer_tab_phone() {
    let mut harness = the_busy_tab("unluminous-designer-shot-phone");
    did(&mut harness, "plugins run designer device phone");
    did(&mut harness, "plugins run designer mode comment");
    the_page_reports(
        &mut harness,
        serde_json::json!([{ "kind": "anchors", "found": [], "missing": [3] }]),
    );
    the_page_reports(&mut harness, serde_json::json!([{ "kind": "pin", "id": 3 }]));
    // The chosen element's card is put away so the detached note is on the screen beside the phone.
    harness
        .state_mut()
        .designer()
        .unwrap()
        .clear_the_selection(std::path::Path::new(".designs/landing.html"))
        .unwrap();
    steady(&mut harness);
    harness.snapshot(shot("designer_tab_phone").as_str());
}

#[test]
fn designer_composer() {
    let mut harness = the_busy_tab("unluminous-designer-shot-composer");
    the_page_reports(
        &mut harness,
        serde_json::json!([{ "kind": "comment", "element": a_chosen_element(), "fx": 1.0, "fy": 0.0 }]),
    );
    harness
        .state_mut()
        .designer()
        .unwrap()
        .design_view_mut(std::path::Path::new(".designs/landing.html"))
        .unwrap()
        .composing
        .as_mut()
        .unwrap()
        .text = "Put the first half on one line and use the accent colour for \"actually open\"."
        .to_owned();
    steady(&mut harness);
    harness.snapshot(shot("designer_composer").as_str());
}

/// The blank tab beside the design's tab: the list of designs the pane opens one from.
#[test]
fn designer_pane() {
    let mut harness = the_busy_tab("unluminous-designer-shot-pane");
    did(&mut harness, "plugins run designer new-tab");
    steady(&mut harness);
    harness.snapshot(shot("designer_pane").as_str());
}

#[test]
fn designer_settings() {
    let mut harness = the_busy_tab("unluminous-designer-shot-settings");
    did(&mut harness, "action run settings");
    steady(&mut harness);
    harness.get_all_by_label("Designer").last().expect("the Settings row").click();
    steady(&mut harness);
    harness.snapshot(shot("designer_settings").as_str());
}

#[test]
fn designer_history() {
    let mut harness = the_busy_tab("unluminous-designer-shot-history");
    did(&mut harness, "plugins run designer history");
    steady(&mut harness);
    harness.snapshot(shot("designer_history").as_str());
}

#[test]
fn designer_tab_light() {
    let mut harness = the_busy_tab("unluminous-designer-shot-light");
    did(&mut harness, "plugins run designer device desktop");
    did(&mut harness, "theme set \"Unluminous Light\"");
    steady(&mut harness);
    harness.snapshot(shot("designer_tab_light").as_str());
}

/// `task-2263`: *"when i have Designer and files open, and click the url of the browser tab, suddenly the page
/// content disappears and the content in Designer appears, and vice versa."* With the Designer holding the keyboard,
/// a browser tab in the editing area is not a focused placement as well, so the one native view has one page to
/// go to. Both were focused before, and the tab, drawn first, took the view back on the frame after a click on the
/// Designer's page.
#[test]
fn a_browser_tab_beside_the_designer_does_not_claim_the_view_while_the_designer_has_the_keyboard() {
    let (_folder, mut harness) = a_window_on_a_design("beside-a-browser-tab");
    did(&mut harness, "browser open https://example.com/");
    steady(&mut harness);
    let designer = harness
        .state()
        .browser_placements()
        .into_iter()
        .map(|placement| placement.id)
        .collect::<Vec<_>>();
    assert_eq!(designer.len(), 2, "the design's page and the tab's are both placed: {designer:?}");
    // Pressing the Designer's page area gives the Designer the keyboard.
    let page = harness
        .state()
        .browser_placements()
        .into_iter()
        .find(|placement| placement.visible.left() < 900.0)
        .expect("the design's page, on the left")
        .visible;
    let at = page.center();
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    harness.step();
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
        harness.step();
    }
    steady(&mut harness);
    let focused: Vec<u64> = harness
        .state()
        .browser_placements()
        .into_iter()
        .filter(|placement| placement.focused)
        .map(|placement| placement.id)
        .collect();
    assert!(focused.len() <= 1, "at most one page is the focused one: {focused:?}");
}
