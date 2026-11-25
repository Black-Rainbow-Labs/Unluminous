//! Agent-Tasks reading and writing a real agent-tasks-service (`tasks/agent-tasks-through-the-service-tdd.md`).
//!
//! Each test starts a `tasks-api` on a free port with a database of its own, and is skipped with a message when
//! no `tasks-api` is built: `AGENT_TASKS_BIN`, or `agent-tasks-service/target/release/tasks-api` beside this
//! checkout. The commands are the ones `plugins run agent-tasks` sends, so these cover the command line and the
//! buttons that call the same code.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tasks_client::{CreateTask, Credential, TasksClient};

use super::{AgentTasks, BoardSource};
use crate::services::pairing;
use crate::services::plugin_ui::{Context, UiProvider};

/// A `tasks-api` that is stopped and whose files are removed when it is dropped.
struct Service {
    process: Child,
    client: TasksClient,
    conf: PathBuf,
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
    let sibling = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../agent-tasks-service/target/release/tasks-api");
    sibling.exists().then_some(sibling)
}

/// A port nothing is listening on.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").expect("a free port").local_addr().expect("its address").port()
}

/// Start a service, or say why the test is skipped and answer `None`.
fn start_the_service() -> Option<Service> {
    let Some(binary) = the_service_binary() else {
        eprintln!("SKIPPED: no tasks-api. Build agent-tasks-service with scripts/setup.sh, or set AGENT_TASKS_BIN.");
        return None;
    };
    let port = free_port();
    let home = std::env::temp_dir().join(format!("unluminous-service-board-{port}-{}", std::process::id()));
    let database = std::env::temp_dir().join(format!("unluminous-service-board-{port}-{}.rdb", std::process::id()));
    let token = format!("test-token-{port}-0123456789abcdef");
    for folder in ["dev/alpha", "dev/beta", "other/gamma"] {
        std::fs::create_dir_all(home.join(folder)).expect("a project folder");
    }
    let process = Command::new(binary)
        .env("AGENT_TASKS_PORT", port.to_string())
        .env("AGENT_TASKS_TOKEN", &token)
        .env("AGENT_TASKS_DB", &database)
        .env("AGENT_TASKS_DEVICE_PORT", free_port().to_string())
        .env("AGENT_TASKS_UNLUMINOUS_HOME", &home)
        // A home of its own, so the service does not read this machine's Claude Code transcripts while the test
        // waits on it.
        .env("HOME", &home)
        .env("AGENT_TASKS_TAILSCALE", "/nonexistent/tailscale")
        .env("AGENT_TASKS_WORKTREES", "0")
        .env("AGENT_PTY_URL", "ws://127.0.0.1:1")
        // A projects folder of its own, `<home>/dev`, holding the folders `alpha` and `beta`.
        .env("AGENT_PROJECTS_ROOT", home.join("dev"))
        .env("AGENT_PROJECTS_ROOT_FILE", home.join("projects-root.txt"))
        .env_remove("AGENT_PROJECTS_FILE")
        .env_remove("AGENT_PROJECTS_JSON")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("tasks-api starts");
    let client = TasksClient::new(&format!("http://127.0.0.1:{port}"), Credential::Machine(token));
    let service = Service { process, client, conf: home.join("boards").join(format!("{port}.conf")), database, home };
    // The first `GET /health` after the service starts took 1.1 to 1.3 seconds on the MacBook Pro, and four
    // services starting at once took longer than the second and a half `find_board` gives it. Later ones take
    // under two milliseconds, so the wait is for `find_board` itself to accept the service.
    let named = pairing::Source { folder: None, file: Some(service.conf.clone()) };
    wait_until(|| pairing::find_board(&named).is_ok(), "tasks-api answers and writes its conf file");
    Some(service)
}

/// Wait up to ten seconds for something to be true.
/// @param done - the condition
/// @param what - what is being waited for, for the failure message
fn wait_until(mut done: impl FnMut() -> bool, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "gave up waiting: {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// An Agent-Tasks with no settings folder, switched to the service.
/// @param service - the service it reads
fn a_board_on(service: &Service) -> AgentTasks {
    let mut tasks = AgentTasks::default();
    tasks.open(&Context::default()).expect("a board in memory opens");
    tasks.find_the_service_through(service.conf.clone());
    run(&mut tasks, "board-source", &["service"]);
    assert_eq!(tasks.board_source(), BoardSource::Service);
    tasks
}

/// Run a command the way `plugins run agent-tasks` does, and answer its message.
/// @param tasks - the board
/// @param command - the command
/// @param arguments - its words
fn run(tasks: &mut AgentTasks, command: &str, arguments: &[&str]) -> String {
    let words: Vec<String> = arguments.iter().map(|word| (*word).to_owned()).collect();
    match UiProvider::command(tasks, command, &words) {
        Ok(answer) => answer.message,
        Err(problem) => panic!("`{command} {}` was refused: {problem}", arguments.join(" ")),
    }
}

/// The titles on the service's board, read by the service's own client.
/// @param service - the service
fn titles_on(service: &Service) -> Vec<String> {
    service.client.full_board().expect("the service's board").tasks.into_iter().map(|full| full.task.title).collect()
}

#[test]
fn a_ticket_made_in_the_pane_is_on_the_service_and_its_changes_reach_it() {
    let Some(service) = start_the_service() else { return };
    let mut tasks = a_board_on(&service);
    let key = run(&mut tasks, "new-task", &["Draw", "the", "lanes"]);
    assert!(titles_on(&service).contains(&"Draw the lanes".to_owned()), "the service has the ticket, made as {key}");

    let card = tasks.board.tasks().find(|task| task.title == "Draw the lanes").expect("the pane shows it").clone();
    run(&mut tasks, "comment", &[&card.key, "Started", "on", "it"]);
    run(&mut tasks, "todo-add", &[&card.key, "Read", "the", "design"]);
    run(&mut tasks, "todo-done", &[&card.key, "1"]);
    run(&mut tasks, "priority", &[&card.key, "high"]);
    run(&mut tasks, "move-task", &[&card.key, "qa_failed"]);

    let full = service.client.task_by_key(&card.key).expect("the service has the ticket");
    assert_eq!(full.task.status, "qa_failed");
    assert_eq!(full.task.priority, "high");
    assert_eq!(full.task.comments.iter().map(|comment| comment.body.as_str()).collect::<Vec<&str>>(), ["Started on it"]);
    assert!(full.task.todos.iter().all(|todo| todo.done) && full.task.todos.len() == 1, "the todo is there and ticked");
}

#[test]
fn a_ticket_made_on_the_service_shows_in_the_pane_after_the_service_says_so() {
    let Some(service) = start_the_service() else { return };
    let mut tasks = a_board_on(&service);
    service.client.create_task(&CreateTask { title: "Filed from the phone".into(), ..Default::default() }).expect("the phone files a ticket");
    wait_until(
        || {
            tasks.notice_the_service();
            tasks.board.tasks().any(|task| task.title == "Filed from the phone")
        },
        "the pane shows the ticket the service made",
    );
}

#[test]
fn sprints_and_epics_made_in_the_pane_are_the_services() {
    let Some(service) = start_the_service() else { return };
    let mut tasks = a_board_on(&service);
    run(&mut tasks, "new-sprint", &["Sprint", "A"]);
    run(&mut tasks, "new-epic", &["infra"]);
    let board = service.client.full_board().expect("the service's board");
    assert_eq!(board.active_sprint.map(|sprint| sprint.name), Some("Sprint A".to_owned()), "a new sprint is the active one");
    assert!(board.epics.iter().any(|epic| epic.name == "infra"));
    run(&mut tasks, "new-task", &["In", "the", "sprint"]);
    assert_eq!(tasks.board.total(), 1, "the pane shows the active sprint's ticket");
}

#[test]
fn choosing_the_file_again_shows_the_file_board() {
    let Some(service) = start_the_service() else { return };
    let mut tasks = a_board_on(&service);
    run(&mut tasks, "new-task", &["Only", "on", "the", "service"]);
    run(&mut tasks, "board-source", &["file"]);
    assert_eq!(tasks.board_source(), BoardSource::File);
    assert_eq!(tasks.board.total(), 0, "a test window's file board is in memory and empty");
    assert!(titles_on(&service).contains(&"Only on the service".to_owned()), "the service keeps its ticket");
}

#[test]
fn a_service_that_is_not_running_is_said_rather_than_replaced_by_the_file() {
    let mut tasks = AgentTasks::default();
    tasks.open(&Context::default()).expect("a board in memory opens");
    let missing = std::env::temp_dir().join(format!("unluminous-no-such-board-{}.conf", std::process::id()));
    tasks.find_the_service_through(missing);
    let refused = UiProvider::command(&mut tasks, "board-source", &["service".to_owned()]).expect_err("there is no service to read");
    assert!(refused.contains("agent-tasks-service"), "the refusal names the service: {refused}");
    assert!(tasks.store.is_none(), "the file board is not shown in its place");
}

/// Run a command the way `plugins run agent-tasks` does, and answer its refusal.
/// @param tasks - the board
/// @param command - the command
/// @param arguments - its words
fn refused(tasks: &mut AgentTasks, command: &str, arguments: &[&str]) -> String {
    let words: Vec<String> = arguments.iter().map(|word| (*word).to_owned()).collect();
    match UiProvider::command(tasks, command, &words) {
        Ok(answer) => panic!("`{command} {}` was not refused: {}", arguments.join(" "), answer.message),
        Err(problem) => problem,
    }
}

#[test]
fn the_project_field_offers_the_services_projects_and_no_project() {
    let Some(service) = start_the_service() else { return };
    let mut tasks = a_board_on(&service);
    let ids: Vec<String> = tasks.project_choices(None).into_iter().map(|(id, _)| id).collect();
    assert!(ids.contains(&"alpha".to_owned()) && ids.contains(&"beta".to_owned()), "every folder in the projects folder: {ids:?}");
    assert_eq!(tasks.no_project_words(), "No project");

    run(&mut tasks, "new-task", &["Pick", "a", "project"]);
    let card = tasks.board.tasks().find(|task| task.title == "Pick a project").expect("the pane shows it").clone();
    assert_eq!(card.project, None, "a new ticket has no project");
    let folder = service.home.join("dev/beta").display().to_string();
    run(&mut tasks, "project", &[&card.key, &folder]);
    let on_the_service = |service: &Service| service.client.full_board().unwrap().tasks.into_iter().find(|full| full.task.title == "Pick a project").unwrap().agent_project_id;
    assert_eq!(on_the_service(&service).as_deref(), Some("beta"), "a folder is written as the service's project id");
    run(&mut tasks, "project", &[&card.key, "alpha"]);
    assert_eq!(on_the_service(&service).as_deref(), Some("alpha"));
    let problem = refused(&mut tasks, "project", &[&card.key, "/nowhere/at/all"]);
    assert!(problem.contains("alpha"), "a refusal names the projects: {problem}");
    run(&mut tasks, "project", &[&card.key, "none"]);
    assert_eq!(on_the_service(&service), None, "none leaves the ticket with no project");
}

#[test]
fn the_projects_folder_is_read_and_changed_through_the_service() {
    let Some(service) = start_the_service() else { return };
    let mut tasks = a_board_on(&service);
    let said = run(&mut tasks, "projects-root", &[]);
    assert!(said.ends_with(&service.home.join("dev").display().to_string()), "{said}");
    let other = service.home.join("other").display().to_string();
    run(&mut tasks, "projects-root", &[&other]);
    assert_eq!(service.client.agent_projects_root().unwrap().root.as_deref(), Some(other.as_str()), "the service has the new folder");
    let ids: Vec<String> = tasks.project_choices(None).into_iter().map(|(id, _)| id).collect();
    assert!(ids.contains(&"gamma".to_owned()) && !ids.contains(&"alpha".to_owned()), "the list follows the folder at once: {ids:?}");
    let problem = refused(&mut tasks, "projects-root", &["/nonexistent/dev"]);
    assert!(problem.contains("not a folder"), "{problem}");
    let listed = run(&mut tasks, "projects", &[]);
    assert!(listed.contains(&other), "{listed}");
}
