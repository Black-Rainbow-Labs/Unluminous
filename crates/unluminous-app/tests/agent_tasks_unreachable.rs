//! Agent-Tasks set to read agent-tasks-service when no service is running on this computer (task-2930).
//!
//! The provider used to fail to open, and the window kept that failure for its whole life, so every
//! `plugins run agent-tasks` command, `sign-in` included, answered that the service could not be reached. Now the
//! provider opens with no board, says why in the pane and in every board command's refusal, and still answers the
//! commands that choose a board. A provider whose `open` failed is also opened again on the next command.
//!
//! The sign in test starts a real `tasks-api` and is skipped with a message when none is built: `AGENT_TASKS_BIN`, or
//! `agent-tasks-service/target/release/tasks-api` beside this checkout. The session it gets is kept in a keychain
//! entry named for this process (`UNLUMINOUS_BOARD_SESSION_KEY`), never the one the person running the tests uses,
//! and is removed at the end.

mod common;

use common::*;

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use unluminous_app::UnluminousApp;

/// The owner password the test service is started with.
const PASSWORD: &str = "UnreachableBoardTest!2026";

/// A `tasks-api` that is stopped and whose files are removed when it is dropped.
struct Service {
    process: Child,
    port: u16,
    database: PathBuf,
    home: PathBuf,
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_file(&self.database);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// The `tasks-api` binary, or `None` when it has not been built.
fn the_service_binary() -> Option<PathBuf> {
    if let Ok(named) = std::env::var("AGENT_TASKS_BIN") {
        return Some(PathBuf::from(named)).filter(|path| path.exists());
    }
    let sibling = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../../agent-tasks-service/target/release/tasks-api{}",
        std::env::consts::EXE_SUFFIX
    ));
    sibling.exists().then_some(sibling)
}

/// A port nothing is listening on.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").expect("a free port").local_addr().expect("its address").port()
}

/// Start a service with an owner account, or say why the test is skipped and answer `None`.
fn start_the_service() -> Option<Service> {
    let Some(binary) = the_service_binary() else {
        eprintln!("SKIPPED: no tasks-api. Build agent-tasks-service, or set AGENT_TASKS_BIN.");
        return None;
    };
    let port = free_port();
    let home =
        std::env::temp_dir().join(format!("unluminous-unreachable-{port}-{}", std::process::id()));
    let database = std::env::temp_dir()
        .join(format!("unluminous-unreachable-{port}-{}.rdb", std::process::id()));
    std::fs::create_dir_all(home.join("dev")).expect("a projects folder");
    let process = Command::new(binary)
        .env("AGENT_TASKS_PORT", port.to_string())
        .env("AGENT_TASKS_TOKEN", format!("test-token-{port}-0123456789abcdef"))
        .env("AGENT_TASKS_DB", &database)
        .env("AGENT_TASKS_DEVICE_PORT", free_port().to_string())
        .env("AGENT_TASKS_UNLUMINOUS_HOME", &home)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("AGENT_TASKS_TAILSCALE", "/nonexistent/tailscale")
        .env("AGENT_TASKS_OWNER_USERNAME", "jason")
        .env("AGENT_TASKS_OWNER_PASSWORD", PASSWORD)
        .env("AGENT_TASKS_WORKTREES", "0")
        .env("AGENT_PTY_URL", "ws://127.0.0.1:1")
        .env("AGENT_PROJECTS_ROOT", home.join("dev"))
        .env("AGENT_PROJECTS_ROOT_FILE", home.join("projects-root.txt"))
        .env_remove("AGENT_PROJECTS_FILE")
        .env_remove("AGENT_PROJECTS_JSON")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("tasks-api starts");
    let service = Service { process, port, database, home };
    let deadline = Instant::now() + Duration::from_secs(20);
    let health = format!("http://127.0.0.1:{port}/health");
    while ureq::get(&health).call().is_err() {
        assert!(Instant::now() < deadline, "tasks-api did not answer {health}");
        std::thread::sleep(Duration::from_millis(100));
    }
    Some(service)
}

/// A window on a copy of the sample project whose settings folder has Agent-Tasks set to read agent-tasks-service,
/// with no board URL and no service in the settings folder's `boards` folder. Answers the window, the plugin's
/// folder and the copied project.
/// @param name - names the copy
fn a_window_set_to_an_absent_service(
    name: &str,
) -> (Harness<'static, UnluminousApp>, PathBuf, PathBuf) {
    let folder = copy_out_of_the_repository(&sample_folder(), name);
    let settings = folder.join(".unluminous").join("test-settings");
    let plugin = settings.join("plugins").join("agent-tasks");
    std::fs::create_dir_all(&plugin).expect("the plugin's folder");
    std::fs::write(plugin.join("settings.conf"), "board-source = service\n")
        .expect("the plugin's settings");
    let mut harness = harness_in(&folder);
    harness.state_mut().use_store(unluminous_app::services::store::Store::at(&settings));
    steady(&mut harness);
    (harness, plugin, folder)
}

/// The refusal `plugins run agent-tasks <line>` answers with.
/// @param harness - the window
/// @param line - the command's words after `plugins run agent-tasks`
fn refusal_to(harness: &mut Harness<'static, UnluminousApp>, line: &str) -> String {
    let reply = run(harness, &format!("plugins run agent-tasks {line}"));
    assert!(!reply.ok, "`{line}` was not refused: {}", reply.message);
    reply.message
}

/// Whether the provider has a board open, from `plugins view agent-tasks`.
fn board_is_open(harness: &mut Harness<'static, UnluminousApp>) -> bool {
    did(harness, "plugins view agent-tasks")["open"] == true
}

/// Remove a folder this test made.
fn tidy(folder: &Path) {
    std::fs::remove_dir_all(folder).ok();
}

#[test]
fn with_no_service_the_board_says_why_and_the_commands_that_choose_a_board_still_answer() {
    let (mut harness, _plugin, folder) =
        a_window_set_to_an_absent_service("unluminous-absent-service");
    // The provider opens, with no board: `view` answers rather than refusing.
    assert!(!board_is_open(&mut harness), "there is no board to open");
    let refused = refusal_to(&mut harness, "board");
    assert!(
        refused.contains("could not be reached") && refused.contains("No Tasks board is running"),
        "a board command says why there is no board: {refused}"
    );
    let refused = refusal_to(&mut harness, "new-task A ticket");
    assert!(refused.contains("could not be reached"), "{refused}");
    // The commands that choose a board answer.
    let source = did(&mut harness, "plugins run agent-tasks board-source");
    assert_eq!(source["boardSource"], "service");
    assert!(
        source["board"].as_str().unwrap_or_default().contains("No Tasks board is running"),
        "where the board is says why it is not open: {source}"
    );
    let url = did(&mut harness, "plugins run agent-tasks board-url");
    assert!(url["boardUrl"].is_null());
    let refused = refusal_to(&mut harness, "projects-root");
    assert!(
        refused.contains("not open") && refused.contains("No Tasks board is running"),
        "{refused}"
    );
    let refused = refusal_to(&mut harness, "sign-in http://127.0.0.1:1 jason wrong");
    assert!(
        refused.starts_with("signing in to"),
        "sign-in is tried, not refused for the board: {refused}"
    );

    // The pane draws the reason where the lanes would be.
    did(&mut harness, "plugins pane agent-tasks --show");
    steady(&mut harness);
    assert!(
        harness.get_all_by_label_contains("The board could not be opened").count() > 0,
        "the pane says the board could not be opened"
    );

    // The Settings page draws in the same state, with the board row on it.
    did(&mut harness, "modal open settings --page agent-tasks");
    steady(&mut harness);
    assert!(
        harness.get_all_by_label_contains("Board address").count() > 0,
        "the Board on another computer row is on the page"
    );
    did(&mut harness, "modal cancel");

    // Choosing the file board opens it, so the board reads.
    did(&mut harness, "plugins run agent-tasks board-source file");
    assert!(board_is_open(&mut harness), "the file board is open");
    did(&mut harness, "plugins run agent-tasks new-task On the file board");
    tidy(&folder);
}

#[test]
fn a_settings_file_changed_by_hand_opens_the_board_without_a_restart() {
    let (mut harness, plugin, folder) =
        a_window_set_to_an_absent_service("unluminous-absent-by-hand");
    assert!(!board_is_open(&mut harness));
    did(&mut harness, "plugins pane agent-tasks --show");
    // Written later than the window read it, so its modified time moves.
    std::thread::sleep(Duration::from_millis(50));
    std::fs::write(plugin.join("settings.conf"), "board-source = file\n").expect("edited by hand");
    wait_until(&mut harness, "the board opens from the edited settings file", |app| {
        app.plugin_ui.view_of("agent-tasks").is_some_and(|view| view["open"] == true)
    });
    did(&mut harness, "plugins run agent-tasks new-task Read without a restart");
    tidy(&folder);
}

#[test]
fn signing_in_with_no_service_on_this_computer_reads_the_board_it_signed_in_to() {
    let Some(service) = start_the_service() else { return };
    let key = format!("agent-tasks-board-session-test-{}", std::process::id());
    std::env::set_var("UNLUMINOUS_BOARD_SESSION_KEY", &key);
    let (mut harness, _plugin, folder) =
        a_window_set_to_an_absent_service("unluminous-absent-sign-in");
    assert!(!board_is_open(&mut harness));
    let address = format!("http://127.0.0.1:{}", service.port);
    let signed_in =
        did(&mut harness, &format!("plugins run agent-tasks sign-in {address} jason {PASSWORD}"));
    assert_eq!(signed_in["boardUrl"], address.as_str());
    assert!(board_is_open(&mut harness), "the board signed in to is open");
    did(&mut harness, "plugins run agent-tasks new-task Filed after signing in");
    let read = did(&mut harness, "plugins run agent-tasks board");
    assert!(
        read.to_string().contains("Filed after signing in"),
        "the board reads the ticket from the service: {read}"
    );
    let _ = unluminous_app::services::agent_tasks::keychain::write(&key, "");
    tidy(&folder);
}
