//! Several projects in one window, a project tab each (`task-2322`).
//!
//! A `ProjectWindow` holds one `UnluminousApp` a project tab and draws the one showing. These tests open
//! projects as tabs, switch between them, and read back that each tab kept its own project: its explorer's
//! folder, its open files, which panels it was showing and how wide they were, while the settings, which are
//! one person's, follow whichever tab is showing. Then the four `project tab-*` commands, the tab's right click
//! menu, the plus's dropdown, the title bar without the branch widget, and a picture of the bar.

mod common;

use common::*;

use egui::vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::app::actions::Action;
use unluminous_app::app::project_tabs::ProjectWindow;
use unluminous_app::UnluminousApp;

/// Three small projects, each a folder with a file of its own.
fn projects() -> [std::path::PathBuf; 3] {
    [
        fixture(
            "unluminous-project-tabs-alpha",
            &[("alpha.md", "# Alpha\n"), ("notes.txt", "one\n")],
        ),
        fixture("unluminous-project-tabs-beta", &[("beta.md", "# Beta\n")]),
        fixture("unluminous-project-tabs-gamma", &[("gamma.md", "# Gamma\n")]),
    ]
}

/// A window whose first tab is `folder`, built the way a test builds an `UnluminousApp`.
fn window_on(folder: &std::path::Path) -> Harness<'static, ProjectWindow> {
    let folder = folder.to_path_buf();
    let mut harness = builder().with_size(vec2(WINDOW[0], WINDOW[1])).build_eframe(move |cc| {
        let mut app = UnluminousApp::new(folder);
        app.prepare(&cc.egui_ctx);
        app.draw_deterministically();
        ProjectWindow::new(app)
    });
    settle_the_window(&mut harness);
    harness
}

/// Draw until the window is quiet, and never fail because it was not quiet soon enough. See `common::steady`.
fn settle_the_window(harness: &mut Harness<'static, ProjectWindow>) {
    for _ in 0..STEADY_TRIES {
        if harness.try_run().is_ok() {
            return;
        }
    }
}

/// Run a command line against the tab that is showing, insisting it worked.
fn drive(harness: &mut Harness<'static, ProjectWindow>, line: &str) -> serde_json::Value {
    let ctx = harness.ctx.clone();
    let reply = harness
        .state_mut()
        .run_command_line(line, &ctx)
        .unwrap_or_else(|| panic!("`{line}` was not answered"));
    note_a_drive(&reply.command, reply.ok);
    settle_the_window(harness);
    assert!(reply.ok, "`{line}` was refused: {}", reply.message);
    reply.result
}

/// The same against a tab that is not necessarily showing.
fn drive_tab(
    harness: &mut Harness<'static, ProjectWindow>,
    tab: usize,
    line: &str,
) -> unluminous_cli::protocol::Reply {
    let ctx = harness.ctx.clone();
    let reply = harness
        .state_mut()
        .run_command_line_on(tab, line, &ctx)
        .unwrap_or_else(|| panic!("`{line}` was not answered"));
    settle_the_window(harness);
    reply
}

/// Open `folder` as a project tab, the way `project tab-open` does.
fn open_tab(harness: &mut Harness<'static, ProjectWindow>, folder: &std::path::Path) {
    drive(harness, &format!("project tab-open \"{}\"", folder.display()));
}

fn root_of(app: &UnluminousApp) -> std::path::PathBuf {
    app.tree.root().to_path_buf()
}

/// A second project opens as a tab: two pills, the new one showing, and its explorer on its own folder.
#[test]
fn a_second_project_opens_as_a_tab_of_its_own_and_is_shown() {
    let [alpha, beta, _] = projects();
    let mut harness = window_on(&alpha);
    open_tab(&mut harness, &beta);

    let window = harness.state();
    assert_eq!(window.tab_count(), 2);
    assert_eq!(window.showing(), 1, "the tab just opened is the one showing");
    assert_eq!(root_of(window.app()), beta);
    assert_eq!(root_of(window.tab(0)), alpha, "the first tab is still on its own project");
    harness.get_by_label("Project tab: unluminous-project-tabs-alpha");
    harness.get_by_label("Project tab: unluminous-project-tabs-beta");
    harness.get_by_label("New Project Tab");
}

/// Each tab keeps its own open files, its explorer, its terminal and its panel sizes across being switched
/// away from and back, and the settings, which are one person's, follow the tab that is showing.
#[test]
fn each_tab_keeps_its_own_files_panels_and_layout_and_the_settings_follow_the_one_showing() {
    let [alpha, beta, _] = projects();
    let mut harness = window_on(&alpha);
    drive(&mut harness, "tab open alpha.md");
    drive(&mut harness, "panel size explorer --width 333");
    open_tab(&mut harness, &beta);
    drive(&mut harness, "tab open beta.md");
    drive(&mut harness, "explorer hide");
    drive(&mut harness, "terminal show");
    drive(&mut harness, "theme set \"Unluminous Light\"");

    // Back to the first.
    drive(&mut harness, "project tab-show 0");
    {
        let window = harness.state();
        assert_eq!(window.showing(), 0);
        let alpha_app = window.app();
        assert!(
            alpha_app.files.active().document.path().is_some_and(|path| path.ends_with("alpha.md")),
            "the first tab still shows alpha.md"
        );
        assert!(
            alpha_app.explorer_visible,
            "hiding the explorer in the second tab left the first's alone"
        );
        assert!(
            !alpha_app.terminal.visible,
            "showing the terminal in the second tab left the first's alone"
        );
        assert_eq!(
            alpha_app.panes.explorer_width, 333.0,
            "the first tab's explorer kept its width"
        );
        assert_eq!(
            alpha_app.settings.theme, "unluminous/light",
            "the theme chosen in the second tab is the theme of every tab"
        );
        let beta_app = window.tab(1);
        assert!(beta_app
            .files
            .active()
            .document
            .path()
            .is_some_and(|path| path.ends_with("beta.md")));
        assert!(!beta_app.explorer_visible);
        assert!(beta_app.terminal.visible);
        assert_ne!(beta_app.panes.explorer_width, 333.0, "a layout is each tab's own");
    }
    // And forward again, and the second is exactly as it was left.
    drive(&mut harness, "project tab-show beta");
    let window = harness.state();
    assert_eq!(window.showing(), 1);
    assert!(!window.app().explorer_visible);
    assert!(window.app().terminal.visible);
}

/// Opening project tabs leaves the interface size alone (`task-2325`: *"When i open a new project tab, the entire
/// window fonts and icons will either get bigger, or get smaller, and toggle back and forth between big and small
/// with each new project i open."*). The size is egui's zoom, one for the window, and a new tab's setup used to
/// ask for 1.0 on it in the same frame the tab's own size was asked for, which egui then dropped.
#[test]
fn opening_project_tabs_keeps_the_interface_size() {
    let [alpha, beta, gamma] = projects();
    let mut harness = window_on(&alpha);
    drive(&mut harness, "settings set appearance.ui.font.size 20");
    assert_eq!(harness.ctx.zoom_factor(), 1.6, "20 points is a zoom of 1.6");
    for folder in [&beta, &gamma] {
        open_tab(&mut harness, folder);
        assert_eq!(
            harness.ctx.zoom_factor(),
            1.6,
            "the window kept its size after opening {}",
            folder.display()
        );
    }
    drive(&mut harness, "project tab-show 0");
    assert_eq!(harness.ctx.zoom_factor(), 1.6, "and after going back to the first tab");
    assert_eq!(harness.state().app().settings.ui_font_size, 20.0);
}

/// A tab that is not showing still answers the command line, which is what lets an agent started in it drive
/// it while the person looks at another project.
#[test]
fn a_tab_that_is_not_showing_answers_its_own_commands() {
    let [alpha, beta, _] = projects();
    let mut harness = window_on(&alpha);
    open_tab(&mut harness, &beta);
    let reply = drive_tab(&mut harness, 0, "tab open notes.txt");
    assert!(reply.ok, "{}", reply.message);
    let window = harness.state();
    assert_eq!(window.showing(), 1, "the command did not switch the window to the tab it was for");
    assert!(
        window
            .tab(0)
            .files
            .iter()
            .any(|file| file.document.path().is_some_and(|path| path.ends_with("notes.txt"))),
        "the hidden tab opened the file it was asked to"
    );
    assert!(
        !window
            .app()
            .files
            .iter()
            .any(|file| file.document.path().is_some_and(|path| path.ends_with("notes.txt"))),
        "and the tab showing did not"
    );
}

/// A folder already open in a tab is shown rather than opened a second time.
#[test]
fn opening_a_project_that_already_has_a_tab_shows_that_tab() {
    let [alpha, beta, _] = projects();
    let mut harness = window_on(&alpha);
    open_tab(&mut harness, &beta);
    let said = drive(&mut harness, &format!("project tab-open \"{}\"", alpha.display()));
    assert_eq!(said["already"], 0);
    assert_eq!(harness.state().tab_count(), 2, "no second tab on the same project");
    assert_eq!(harness.state().showing(), 0);
}

/// `project tabs` lists the tabs, `tab-close` closes one and shows its neighbour, and the last tab stays.
#[test]
fn the_tabs_are_listed_closed_and_the_last_one_stays() {
    let [alpha, beta, gamma] = projects();
    let mut harness = window_on(&alpha);
    open_tab(&mut harness, &beta);
    open_tab(&mut harness, &gamma);
    let listed = drive(&mut harness, "project tabs");
    assert_eq!(listed["count"], 3);
    assert_eq!(listed["showing"], 2);
    assert_eq!(listed["tabs"][1]["name"], "unluminous-project-tabs-beta");

    drive(&mut harness, "project tab-close");
    assert_eq!(harness.state().folders(), vec![alpha.clone(), beta.clone()]);
    assert_eq!(harness.state().showing(), 1, "the neighbour of the tab that went is shown");

    drive(&mut harness, "project tab-close 0");
    assert_eq!(harness.state().folders(), vec![beta.clone()]);
    let reply = drive_tab(&mut harness, 0, "project tab-close");
    assert!(!reply.ok, "the window's only tab is not closed");
    assert_eq!(harness.state().tab_count(), 1);
}

/// The next and previous chords go round the tabs.
#[test]
fn next_and_previous_go_round_the_tabs() {
    let [alpha, beta, gamma] = projects();
    let mut harness = window_on(&alpha);
    open_tab(&mut harness, &beta);
    open_tab(&mut harness, &gamma);
    let ctx = harness.ctx.clone();
    harness.state_mut().app_mut().run_action(Action::NextProjectTab, &ctx);
    settle_the_window(&mut harness);
    assert_eq!(harness.state().showing(), 0, "after the last comes the first");
    harness.state_mut().app_mut().run_action(Action::PreviousProjectTab, &ctx);
    settle_the_window(&mut harness);
    assert_eq!(harness.state().showing(), 2);
}

/// A right click on a pill opens its menu, and `Close Project Tab` in it closes that tab and not the one
/// showing.
#[test]
fn the_tab_menu_closes_the_tab_it_was_opened_on() {
    let [alpha, beta, _] = projects();
    let mut harness = window_on(&alpha);
    open_tab(&mut harness, &beta);
    let pill = harness.get_by_label("Project tab: unluminous-project-tabs-alpha").rect().center();
    right_click_on(&mut harness, pill);
    assert!(
        harness.state().app().project_tab_menu.is_some_and(|(_, index)| index == 0),
        "the menu is open on the first tab"
    );
    harness.get_by_label("Close Project Tab").click();
    settle_the_window(&mut harness);
    assert_eq!(harness.state().folders(), vec![beta]);
}

/// The plus opens the dropdown, which lists the recent projects and the folder button.
#[test]
fn the_plus_opens_the_dropdown_of_recent_projects_and_a_folder_button() {
    let [alpha, beta, gamma] = projects();
    let mut harness = window_on(&alpha);
    harness.state_mut().app_mut().recent = vec![gamma.clone(), beta.clone(), alpha.clone()];
    harness.get_by_label("New Project Tab").click();
    settle_the_window(&mut harness);
    assert!(harness.state().app().project_dropdown.is_some(), "the dropdown is open");
    harness.get_by_label("Choose a folder");
    harness.get_by_label("Recent project: unluminous-project-tabs-gamma");
    harness.get_by_label("Recent project: unluminous-project-tabs-beta").click();
    settle_the_window(&mut harness);
    assert_eq!(harness.state().tab_count(), 2);
    assert_eq!(
        root_of(harness.state().app()),
        beta,
        "the project chosen opened as a tab and is showing"
    );
    assert!(harness.state().app().project_dropdown.is_none(), "and the dropdown went");
}

/// The folder button asks the platform's folder picker, which a test replaces, and opens what it answers.
#[test]
fn the_folder_button_opens_the_folder_the_picker_answers_in_a_new_tab() {
    let [alpha, _, gamma] = projects();
    let mut harness = window_on(&alpha);
    fn picker(_: &str, _: &std::path::Path) -> Option<std::path::PathBuf> {
        // Where `projects` built it: every fixture is under `fixtures_root` (`task-2364`).
        Some(fixtures_root().join("unluminous-project-tabs-gamma"))
    }
    harness.state_mut().app_mut().folder_picker = picker;
    harness.get_by_label("New Project Tab").click();
    settle_the_window(&mut harness);
    harness.get_by_label("Choose a folder").click();
    settle_the_window(&mut harness);
    assert_eq!(root_of(harness.state().app()), gamma);
}

/// The title bar has no branch widget any more, and a tab's pill is pressed to show it.
#[test]
fn a_pill_is_pressed_to_show_its_tab_and_the_branch_widget_is_gone() {
    let [alpha, beta, _] = projects();
    let mut harness = window_on(&alpha);
    open_tab(&mut harness, &beta);
    assert!(harness.query_by_label("Switch branch").is_none());
    harness.get_by_label("Project tab: unluminous-project-tabs-alpha").click();
    settle_the_window(&mut harness);
    assert_eq!(harness.state().showing(), 0);
}

/// Every tab has its own address for its children, so an agent started in one drives that one.
#[test]
fn a_project_tab_in_a_test_window_has_no_command_channel_and_says_so_in_status() {
    let [alpha, beta, _] = projects();
    let mut harness = window_on(&alpha);
    open_tab(&mut harness, &beta);
    let status = drive(&mut harness, "status --section project");
    assert_eq!(status["projectTabs"]["count"], 2);
    assert_eq!(status["projectTabs"]["showing"], 1);
    assert_eq!(status["projectTabs"]["tabs"][0]["showing"], false);
}

/// A picture of the title bar with three project tabs, the third showing.
#[test]
fn the_title_bar_with_three_project_tabs() {
    let [alpha, beta, gamma] = projects();
    let mut harness = window_on(&alpha);
    open_tab(&mut harness, &beta);
    open_tab(&mut harness, &gamma);
    harness.snapshot(shot("project_tabs_three"));
    harness.get_by_label("New Project Tab").click();
    settle_the_window(&mut harness);
    harness.snapshot(shot("project_tabs_dropdown"));
}

/// Press the right mouse button at a point, the one interaction a kittest node cannot do.
fn right_click_on(harness: &mut Harness<'static, ProjectWindow>, at: egui::Pos2) {
    harness.input_mut().events.push(egui::Event::PointerMoved(at));
    harness.step();
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
    }
    settle_the_window(harness);
}
