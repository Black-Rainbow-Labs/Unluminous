//! The Skills Manager in a real window: the pane lists the skills as cards, a card's button installs into the
//! project's own folders, a skill's page has a switch for each agent, New Skill makes one and opens its SKILL.md,
//! and every one of those is reached by `plugins run skills` too. `task-2396`.
//!
//! A test's window has no settings folder, so the provider is given a home folder of its own under the temporary
//! folder (`services::skills::home_for_a_window_with_no_settings`) and never touches the agents of the person
//! running the tests. Each test empties that home first, because a run before this one may have left installs.

mod common;

use common::*;

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::UnluminousApp;

/// A window on a small project with the Skills Manager showing, and the home folder its installs go to.
fn a_window(
    name: &str,
) -> (std::path::PathBuf, std::path::PathBuf, Harness<'static, UnluminousApp>) {
    let folder = fixture(name, &[("readme.md", "# A project\n")]);
    for agent in [".claude", ".agents"] {
        let _ = std::fs::remove_dir_all(folder.join(agent));
    }
    let mut harness = harness_in(&folder);
    harness.set_size(egui::vec2(1600.0, 1000.0));
    did(&mut harness, "plugins pane skills --show");
    steady(&mut harness);
    let home = std::path::PathBuf::from(
        did(&mut harness, "plugins run skills harnesses")["home"]
            .as_str()
            .expect("the home is named")
            .to_owned(),
    );
    let _ = std::fs::remove_dir_all(&home);
    did(&mut harness, "plugins run skills filter all");
    pictures_in_place(&mut harness);
    (folder, home, harness)
}

/// Draw until every card picture asked for has been read on its thread and put on the card, so a screenshot shows
/// the pictures rather than the stand ins they arrive in place of (`task-2401`).
fn pictures_in_place(harness: &mut Harness<'static, UnluminousApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        steady(harness);
        let view = did(harness, "plugins view skills");
        if view["loading"] == false && view["picturesWaiting"] == 0 {
            steady(harness);
            if did(harness, "plugins view skills")["picturesWaiting"] == 0 {
                return;
            }
        }
        assert!(std::time::Instant::now() < deadline, "the pictures never arrived");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// `task-2401`: the pane is drawn the frame it is shown, with the spinner and the words saying it is looking,
/// before the skills folders have been read, and the cards follow with their pictures.
#[test]
fn the_pane_is_drawn_at_once_and_its_pictures_follow() {
    let folder = fixture("unluminous-skills-at-once", &[("readme.md", "# A project\n")]);
    let mut harness = harness_in(&folder);
    harness.set_size(egui::vec2(1600.0, 1000.0));
    run(&mut harness, "plugins pane skills --show");
    harness.step();
    let view = did(&mut harness, "plugins view skills");
    let looked =
        view["loading"] == true || view["listed"].as_array().is_some_and(|ids| !ids.is_empty());
    assert!(looked, "the pane answered before the read: {view}");
    pictures_in_place(&mut harness);
    harness.get_by_label("Skill: Unluminous Task");
    assert_eq!(did(&mut harness, "plugins view skills")["loading"], false);
}

#[test]
fn the_pane_lists_the_unluminous_skills_first_and_a_card_installs_into_the_project() {
    let (folder, _home, mut harness) = a_window("unluminous-skills-install");
    let listed = did(&mut harness, "plugins view skills")["listed"].clone();
    assert_eq!(listed[0], "unluminous-task");
    assert_eq!(listed[1], "unluminous-supervisor");
    harness.get_by_label("Skill: Unluminous Task");
    harness.get_by_label("Install Unluminous Task for Project").click();
    steady(&mut harness);
    let copy = folder.join(".claude").join("skills").join("unluminous-task").join("SKILL.md");
    assert!(copy.exists(), "Claude Code's project folder has it");
    assert!(
        folder.join(".agents").join("skills").join("unluminous-task").join("SKILL.md").exists(),
        "and Codex's"
    );
    let shown = did(&mut harness, "plugins run skills show unluminous-task");
    let installed = shown["installed"].as_array().unwrap();
    assert!(installed.iter().any(|row| row["harness"] == "claude" && row["scope"] == "project"));
    // The same card's button now removes it.
    harness.get_by_label("Remove Unluminous Task from Project").click();
    steady(&mut harness);
    assert!(!copy.exists());
}

/// A skill's page lists the agents on this machine (none in a test's home, so the ones an install goes to: Claude
/// Code and Codex), with one switch between the project and global and one Install button a row.
#[test]
fn a_skills_page_lists_the_agents_here_with_one_switch_and_an_install_button_each() {
    let (_folder, home, mut harness) = a_window("unluminous-skills-page");
    did(&mut harness, "plugins run skills view caveman");
    steady(&mut harness);
    harness.get_by_label("All skills");
    assert!(harness.query_by_label("Install for Pi").is_none(), "Pi is not on this machine");
    harness.get_by_label("Install for: Global").click();
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins view skills")["scope"], "global");
    harness.get_by_label("Install for Codex").click();
    steady(&mut harness);
    let codex = home.join(".agents").join("skills").join("caveman").join("SKILL.md");
    assert!(codex.exists(), "Codex's global folder has it");
    let shown = did(&mut harness, "plugins run skills show caveman");
    assert!(shown["installed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["harness"] == "codex" && row["scope"] == "global"));
    harness.get_by_label("Remove for Codex").click();
    steady(&mut harness);
    assert!(!codex.exists(), "Remove takes it away");
    harness.get_by_label("All skills").click();
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins view skills")["viewing"], serde_json::Value::Null);
}

#[test]
fn new_skill_makes_one_and_opens_its_skill_md() {
    let (_folder, _home, mut harness) = a_window("unluminous-skills-new");
    harness.get_by_label("New Skill").click();
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins view skills")["modal"], "new-skill");
    harness.get_by_label("Cancel").click();
    steady(&mut harness);
    did(&mut harness, "plugins run skills new release-notes Write the release notes");
    steady(&mut harness);
    let open = harness
        .state()
        .files
        .active()
        .path()
        .map(std::path::Path::to_path_buf)
        .expect("a file is open");
    assert!(
        open.ends_with(std::path::Path::new("release-notes").join("SKILL.md")),
        "{}",
        open.display()
    );
    let text = std::fs::read_to_string(&open).unwrap();
    assert!(text.contains("description: Write the release notes"), "{text}");
    let listed = did(&mut harness, "plugins view skills")["listed"].clone();
    assert!(listed.as_array().unwrap().iter().any(|id| id == "release-notes"));
}

/// A skill found in an agent's folder is a card like the catalogue's, with the stand in picture, and its page
/// has the stand in as its banner. The picture is `skills_found`.
#[test]
fn a_skill_found_on_the_machine_is_a_card_with_the_stand_in_picture() {
    let (_folder, home, mut harness) = a_window("unluminous-skills-found");
    let found = home.join(".claude").join("skills").join("my-notes");
    std::fs::create_dir_all(&found).unwrap();
    std::fs::write(
        found.join("SKILL.md"),
        "---\nname: my-notes\ndescription: Keeps notes about the project as the work goes.\n---\n",
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        steady(&mut harness);
        let listed = did(&mut harness, "plugins view skills")["listed"].clone();
        if listed.as_array().unwrap().iter().any(|id| id == "my-notes") {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "the found skill never appeared");
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    did(&mut harness, "plugins run skills search my-notes");
    steady(&mut harness);
    harness.get_by_label("Skill: My Notes");
    harness.get_by_label("Install My Notes for Project");
    harness.snapshot(shot("skills_found").as_str());
}

/// `task-2402`: Grill Me's whole SKILL.md is "Call the Skill tool with grilling". Its page has a Calls row whose
/// chip opens Grilling's page, installing it installs Grilling too, and View / Edit opens Grilling's SKILL.md in a
/// tab beside it.
#[test]
fn a_skill_that_calls_another_names_it_installs_it_and_opens_it_beside_itself() {
    let (_folder, home, mut harness) = a_window("unluminous-skills-calls");
    assert_eq!(did(&mut harness, "plugins run skills show grill-me")["calls"], serde_json::json!(["grilling"]));
    did(&mut harness, "plugins run skills view grill-me");
    steady(&mut harness);
    harness.snapshot(shot("skills_page_calls").as_str());
    harness.get_by_label("Install for: Global").click();
    steady(&mut harness);
    harness.get_by_label("Install for Claude Code").click();
    steady(&mut harness);
    let skills = home.join(".claude").join("skills");
    assert!(skills.join("grill-me").join("SKILL.md").exists(), "Grill Me is installed");
    assert!(skills.join("grilling").join("SKILL.md").exists(), "and so is the skill it calls");
    harness.get_by_label("View / Edit").click();
    steady(&mut harness);
    let names: Vec<String> = (0..harness.state().files.len())
        .filter_map(|index| harness.state().files.get(index)?.path().map(|path| path.display().to_string()))
        .collect();
    assert!(names.iter().any(|name| name.contains("grilling")), "Grilling's SKILL.md is open: {names:?}");
    assert!(harness.state().files.active().path().is_some_and(|path| path.to_string_lossy().contains("grill-me")));
    harness.get_by_label("Grilling").click();
    steady(&mut harness);
    assert_eq!(did(&mut harness, "plugins view skills")["viewing"], "grilling");
}

#[test]
fn skills_pane() {
    let (_folder, _home, mut harness) = a_window("unluminous-skills-shot-pane");
    harness.snapshot(shot("skills_pane").as_str());
}

#[test]
fn skills_page() {
    let (_folder, _home, mut harness) = a_window("unluminous-skills-shot-page");
    did(&mut harness, "plugins run skills install unluminous-task claude --project");
    did(&mut harness, "plugins run skills view unluminous-task");
    steady(&mut harness);
    harness.snapshot(shot("skills_page").as_str());
}

#[test]
fn skills_new_dialog() {
    let (_folder, _home, mut harness) = a_window("unluminous-skills-shot-new");
    harness.get_by_label("New Skill").click();
    steady(&mut harness);
    harness.snapshot(shot("skills_new_dialog").as_str());
}
