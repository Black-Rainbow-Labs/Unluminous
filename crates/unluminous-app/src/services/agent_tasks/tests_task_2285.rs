//! `task-2285`: resuming a Codex or OpenCode ticket hands back the conversation that agent was having, found
//! in its own records, rather than starting a new one.
//!
//! The two agents' databases are built in a temporary folder with the columns `native_session` reads, and
//! named through `UNLUMINOUS_CODEX_HOME` and `UNLUMINOUS_OPENCODE_DATA`. Nothing is started.

use super::*;
use std::path::Path;

fn a_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir()
        .join(format!("unluminous-ticket-restore-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a folder");
    folder
}

/// A board working in `project`, and a ticket of `agent`'s recorded with `session`.
fn a_ticket(project: &Path, agent: Assignee, session: &str) -> (AgentTasks, Task) {
    let mut board = AgentTasks::new();
    board.project = Some(project.to_path_buf());
    let mut task = Task::for_tests("task-7");
    task.assignee = agent;
    task.session_id = Some(session.to_owned());
    (board, task)
}

#[test]
fn a_codex_and_an_opencode_ticket_are_resumed_with_the_session_their_agent_chose() {
    let project = a_folder("project");
    let started = native_session::now_ms() - 10_000;
    let marker =
        format!("{}{started}-0f9a1b2c-1234-4abc-8def-0123456789ab", native_session::PENDING);

    let codex = a_folder("codex");
    let connection = rusqlite::Connection::open(codex.join("state_5.sqlite")).expect("a database");
    connection
        .execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, cwd TEXT NOT NULL, created_at INTEGER NOT NULL, \
             created_at_ms INTEGER, first_user_message TEXT NOT NULL DEFAULT '', archived INTEGER NOT NULL DEFAULT 0)",
        )
        .expect("a table");
    connection
        .execute(
            "INSERT INTO threads (id, cwd, created_at, created_at_ms, first_user_message) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                "01a148bc-804b-7202-9df1-ce01eb22c2b8",
                format!(r"\\?\{}", project.display()),
                (started / 1000 + 2) as i64,
                (started + 2_000) as i64,
                "You are working ticket task-7 on this board."
            ],
        )
        .expect("a row");

    let opencode = a_folder("opencode");
    let connection = rusqlite::Connection::open(opencode.join("opencode.db")).expect("a database");
    connection
        .execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT NOT NULL, \
             time_created INTEGER NOT NULL, time_archived INTEGER)",
        )
        .expect("a table");
    connection
        .execute(
            "INSERT INTO session (id, directory, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params!["ses_ours", project.display().to_string(), (started + 1_000) as i64],
        )
        .expect("a row");

    // Only this test names these variables, so setting them for the process cannot reach another test.
    std::env::set_var("UNLUMINOUS_CODEX_HOME", &codex);
    std::env::set_var("UNLUMINOUS_OPENCODE_DATA", &opencode);

    let (mut board, task) = a_ticket(&project, Assignee::Codex, &marker);
    let found = board.native_session_for(&task, Assignee::Codex);
    assert_eq!(found.as_deref(), Some("01a148bc-804b-7202-9df1-ce01eb22c2b8"));
    let plan = board.plan_for(&task, Assignee::Codex, None, found.expect("found"), true);
    let line = agent::launch(&plan).expect("a launch").arguments;
    assert_eq!(line[..2], ["resume", "01a148bc-804b-7202-9df1-ce01eb22c2b8"], "{line:?}");

    // An id Codex already knows is used as it is, and a marker from before `task-2285` is still found by the
    // ticket key in the first message.
    let (mut board, task) =
        a_ticket(&project, Assignee::Codex, "01a148bc-804b-7202-9df1-ce01eb22c2b8");
    assert_eq!(
        board.native_session_for(&task, Assignee::Codex).as_deref(),
        Some("01a148bc-804b-7202-9df1-ce01eb22c2b8")
    );
    let (mut board, task) =
        a_ticket(&project, Assignee::Codex, "0f9a1b2c-1234-4abc-8def-0123456789ab");
    assert!(board.native_session_for(&task, Assignee::Codex).is_some());

    let (mut board, task) = a_ticket(&project, Assignee::OpenCode, &marker);
    let found = board.native_session_for(&task, Assignee::OpenCode);
    assert_eq!(found.as_deref(), Some("ses_ours"));
    let plan = board.plan_for(&task, Assignee::OpenCode, None, found.expect("found"), true);
    let line = agent::launch(&plan).expect("a launch").arguments;
    assert!(line.windows(2).any(|pair| pair == ["--session", "ses_ours"]), "{line:?}");

    // A ticket started after every session in the folder has none to resume, and says so.
    let later = format!("{}{}-x", native_session::PENDING, started + 60_000);
    let (mut board, task) = a_ticket(&project, Assignee::OpenCode, &later);
    assert_eq!(board.native_session_for(&task, Assignee::OpenCode), None);
    let (mut board, task) = a_ticket(&a_folder("elsewhere"), Assignee::Codex, &marker);
    assert_eq!(board.native_session_for(&task, Assignee::Codex), None, "another folder's ticket");
    drop(board);
    let refused = agent::why_it_cannot_resume(Assignee::OpenCode, &task.key);
    assert!(refused.contains("Start Work again"), "{refused}");

    std::env::remove_var("UNLUMINOUS_CODEX_HOME");
    std::env::remove_var("UNLUMINOUS_OPENCODE_DATA");
}
