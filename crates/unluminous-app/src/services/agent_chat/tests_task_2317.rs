//! `task-2317`: two windows on two projects each run their chat's agent in their own project.
//!
//! *"Open up two different folders in Unluminous. Open up agent-chat in each instance of Unluminous. The first
//! folder that was opened will be the agent-chat's root folder in both instances."* The plugin's folder, its
//! configuration file and its conversations are one for every window, and two things in them were about one
//! project while being read by every window: the folder chosen on the Settings page, and the conversation a
//! pane opens on.

use super::*;

fn a_folder(name: &str) -> PathBuf {
    let folder =
        std::env::temp_dir().join(format!("unluminous-chat-2317-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a folder");
    folder
}

/// A pane on the plugin's folder `plugin`, in a window on `project`, the way the window opens one.
fn a_pane(plugin: &Path, project: &Path) -> AgentChat {
    let mut chat = AgentChat::new();
    chat.open(&Context {
        folder: Some(plugin.to_path_buf()),
        project: Some(project.to_path_buf()),
        ..Context::default()
    })
    .expect("opened");
    chat
}

/// A folder chosen in one window is that window's project's, and the other window keeps its own project.
#[test]
fn a_folder_chosen_in_one_window_is_not_the_other_windows_folder() {
    let plugin = a_folder("plugin");
    let first = a_folder("first");
    let second = a_folder("second");
    let elsewhere = a_folder("elsewhere");

    let mut one = a_pane(&plugin, &first);
    let mut two = a_pane(&plugin, &second);
    assert_eq!(one.working_folder(), Some(first.clone()));
    assert_eq!(two.working_folder(), Some(second.clone()));

    // The first window chooses a folder, and the second window reads the file again before its next turn.
    one.choose_the_folder(&elsewhere.display().to_string()).expect("chosen");
    two.follow_the_configuration_file();
    assert_eq!(one.working_folder(), Some(elsewhere.clone()));
    assert_eq!(
        two.working_folder(),
        Some(second.clone()),
        "the second window is still on its project"
    );

    // Choosing the project itself is choosing nothing, and writes no row.
    two.choose_the_folder(&second.display().to_string()).expect("chosen");
    assert_eq!(Configuration::read(&plugin).0.folders, vec![(first.clone(), elsewhere.clone())]);

    // A third window on the first project starts where the first one chose.
    let three = a_pane(&plugin, &first);
    assert_eq!(three.working_folder(), Some(elsewhere.clone()));

    for folder in [plugin, first, second, elsewhere] {
        let _ = std::fs::remove_dir_all(folder);
    }
}

/// The single `folder` an earlier version wrote applied to every window, and is read as no choice.
#[test]
fn the_folder_an_earlier_version_wrote_for_every_window_is_not_used() {
    let plugin = a_folder("legacy");
    let project = a_folder("legacy-project");
    let other = a_folder("legacy-other");
    std::fs::write(plugin.join(Configuration::FILE), format!("folder = {}\n", other.display()))
        .expect("the file");
    let chat = a_pane(&plugin, &project);
    assert_eq!(chat.working_folder(), Some(project.clone()));
    for folder in [plugin, project, other] {
        let _ = std::fs::remove_dir_all(folder);
    }
}

/// A window opens on the newest conversation from its own folder, never on another project's.
#[test]
fn a_window_opens_on_its_own_projects_newest_conversation() {
    let plugin = a_folder("talks");
    let first = a_folder("talks-first");
    let second = a_folder("talks-second");

    let store = Store::at(Some(plugin.clone()));
    let mut older = Conversation::new("older", "Claude Code");
    older.folder = second.display().to_string();
    older.changed = 100;
    older.name = "about the second".to_owned();
    older.messages.push(Message::said(1, Role::User, "about the second".to_owned()));
    store.write(&older).expect("written");
    let mut newer = Conversation::new("newer", "Claude Code");
    newer.folder = first.display().to_string();
    newer.changed = 200;
    newer.name = "about the first".to_owned();
    newer.messages.push(Message::said(1, Role::User, "about the first".to_owned()));
    store.write(&newer).expect("written");

    let one = a_pane(&plugin, &first);
    let two = a_pane(&plugin, &second);
    let three = a_pane(&plugin, &a_folder("talks-third"));
    assert_eq!(one.session.chat.id, "newer");
    assert_eq!(two.session.chat.id, "older", "the second window opens its own conversation");
    assert!(
        three.session.chat.messages.is_empty() && three.session.chat.id != "newer",
        "a project with no conversation of its own starts a new one"
    );
    for folder in [plugin, first, second] {
        let _ = std::fs::remove_dir_all(folder);
    }
}
