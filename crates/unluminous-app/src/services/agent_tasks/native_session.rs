//! Finding the conversation a Codex or OpenCode ticket's agent is having, in that agent's own records.
//!
//! `task-2285`: *"For agent tasks, it should happen when the agent task is resumed."* Claude Code takes
//! `--session-id <uuid>`, so a Claude ticket names its conversation before it starts and `--resume` hands it
//! back. Codex and OpenCode choose their own ids and offer no flag that names a new one, so until this the
//! id on such a ticket was only Unluminous's marker, and a ticket started again began a new conversation
//! while the old one was still on disk.
//!
//! Both agents keep their sessions in a SQLite file, and the rows say where each session was started and
//! when. Measured on codex-cli 0.153.4 and OpenCode 1.18.32:
//!
//! - Codex: `threads` in `~/.codex/state_<n>.sqlite` (`$CODEX_HOME` when set), with `id`, `cwd` (with a
//!   `\\?\` prefix on Windows), `created_at_ms`, `first_user_message` and `archived`.
//! - OpenCode: `session` in `~/.local/share/opencode/opencode.db` (`$XDG_DATA_HOME/opencode` when set), with
//!   `id`, `directory`, `time_created`, `parent_id` and `time_archived`.
//!
//! **Read only, and read rather than asked.** `opencode session list --format json` gives the same rows and
//! took 1.5 s, which is too long for a button press on the window's thread; the table answers in
//! milliseconds. The format belongs to another program and can change, so a missing file, table or column is
//! "not found" rather than an error, and the ticket then starts a new conversation as it did before.
//!
//! `UNLUMINOUS_CODEX_HOME` and `UNLUMINOUS_OPENCODE_DATA` are the seams a test uses, the way
//! `UNLUMINOUS_CLAUDE_HOME` is for the ticket's chat.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

/// The marker a Codex or OpenCode ticket is recorded with until its own session is found, carrying the time
/// it started so the search survives a restart: `pending-<milliseconds since the epoch>-<id>`.
pub const PENDING: &str = "pending-";

/// A marker for a ticket started now.
pub fn pending_marker(id: &str) -> String {
    format!("{PENDING}{}-{id}", now_ms())
}

/// When a marker says its ticket was started, or `None` for anything that is not a marker.
pub fn started_at(marker: &str) -> Option<u64> {
    marker.strip_prefix(PENDING)?.split('-').next()?.parse().ok()
}

/// Milliseconds since the epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

/// Codex's own folder.
pub fn codex_home() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("UNLUMINOUS_CODEX_HOME").filter(|one| !one.is_empty()) {
        return Some(PathBuf::from(home));
    }
    if let Some(home) = std::env::var_os("CODEX_HOME").filter(|one| !one.is_empty()) {
        return Some(PathBuf::from(home));
    }
    home_folder().map(|home| home.join(".codex"))
}

/// OpenCode's own data folder, which holds `opencode.db`.
pub fn opencode_data() -> Option<PathBuf> {
    if let Some(data) = std::env::var_os("UNLUMINOUS_OPENCODE_DATA").filter(|one| !one.is_empty()) {
        return Some(PathBuf::from(data));
    }
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|one| !one.is_empty()) {
        return Some(PathBuf::from(data).join("opencode"));
    }
    home_folder().map(|home| home.join(".local").join("share").join("opencode"))
}

fn home_folder() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .filter(|one| !one.is_empty())
        .map(PathBuf::from)
}

/// Codex's state file, the highest numbered `state_<n>.sqlite`, because the number is its schema version.
fn codex_state(home: &Path) -> Option<PathBuf> {
    std::fs::read_dir(home)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let number =
                name.strip_prefix("state_")?.strip_suffix(".sqlite")?.parse::<u32>().ok()?;
            Some((number, entry.path()))
        })
        .max_by_key(|(number, _)| *number)
        .map(|(_, path)| path)
}

fn open(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}

/// A folder as text two programs can agree on: no `\\?\`, forward separators, no trailing separator, and
/// case folded on Windows, where the file system does not care.
pub fn same_folder_text(folder: &str) -> String {
    let plain = folder.strip_prefix(r"\\?\").unwrap_or(folder).replace('\\', "/");
    let trimmed = plain.trim_end_matches('/').to_owned();
    match cfg!(windows) {
        true => trimmed.to_lowercase(),
        false => trimmed,
    }
}

/// The Codex thread a ticket's agent started: the newest one in `folder` whose first message names `key`,
/// not archived, and made at or after `since_ms` when that is known.
///
/// Every line Unluminous types into a ticket's agent names the ticket (`agent::handoff`), which is what makes
/// the first message a reliable mark when several tickets share one folder.
pub fn codex_thread(
    home: &Path,
    folder: &Path,
    key: &str,
    since_ms: Option<u64>,
) -> Option<String> {
    let connection = open(&codex_state(home)?)?;
    let mut statement = connection
        .prepare(
            "SELECT id, cwd, first_user_message FROM threads \
             WHERE archived = 0 AND COALESCE(created_at_ms, created_at * 1000) >= ?1 \
             ORDER BY COALESCE(created_at_ms, created_at * 1000) DESC LIMIT 200",
        )
        .ok()?;
    let wanted = same_folder_text(&folder.to_string_lossy());
    let rows: Vec<(String, String, String)> = statement
        .query_map([since_ms.unwrap_or(0) as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .ok()?
        .flatten()
        .collect();
    rows.into_iter()
        .find(|(_, cwd, first)| same_folder_text(cwd) == wanted && names_the_ticket(first, key))
        .map(|(id, _, _)| id)
}

/// Whether a message names `key` as a whole word, so `task-1` is not found inside `task-12`.
fn names_the_ticket(text: &str, key: &str) -> bool {
    text.match_indices(key).any(|(at, _)| {
        let after = text[at + key.len()..].chars().next();
        !after.is_some_and(|one| one.is_ascii_digit())
    })
}

/// Whether Codex has a thread with this id, so a recorded id is only handed to `codex resume` when Codex
/// will answer to it.
pub fn codex_knows(home: &Path, id: &str) -> bool {
    let Some(connection) = codex_state(home).and_then(|path| open(&path)) else {
        return false;
    };
    connection
        .query_row("SELECT 1 FROM threads WHERE id = ?1 AND archived = 0", [id], |_| Ok(()))
        .is_ok()
}

/// The OpenCode session a ticket's agent started: the **earliest** top level session in `folder` made at
/// or after `since_ms`, not archived.
///
/// The earliest, because the agent makes its session as it starts, so any later session in that folder
/// belongs to something started afterwards. OpenCode's first message is not in this table, so the time is
/// the mark, and an older marker with no time cannot be searched for.
pub fn opencode_session(data: &Path, folder: &Path, since_ms: u64) -> Option<String> {
    let connection = open(&data.join("opencode.db"))?;
    let mut statement = connection
        .prepare(
            "SELECT id, directory FROM session \
             WHERE (parent_id IS NULL OR parent_id = '') AND time_archived IS NULL AND time_created >= ?1 \
             ORDER BY time_created ASC LIMIT 200",
        )
        .ok()?;
    let wanted = same_folder_text(&folder.to_string_lossy());
    let rows: Vec<(String, String)> = statement
        .query_map([since_ms as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok()?
        .flatten()
        .collect();
    rows.into_iter().find(|(_, directory)| same_folder_text(directory) == wanted).map(|(id, _)| id)
}

/// Whether OpenCode has a session with this id.
pub fn opencode_knows(data: &Path, id: &str) -> bool {
    let Some(connection) = open(&data.join("opencode.db")) else {
        return false;
    };
    connection
        .query_row(
            "SELECT 1 FROM session WHERE id = ?1 AND time_archived IS NULL",
            [id],
            |_| Ok(()),
        )
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("unluminous-native-session-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a folder");
        folder
    }

    /// A Codex state file with the columns this module reads, and the rows given.
    fn a_codex_home(name: &str, rows: &[(&str, &str, u64, &str, i64)]) -> PathBuf {
        let home = a_folder(name);
        // An older schema beside it, which must not be the one read.
        Connection::open(home.join("state_4.sqlite"))
            .expect("old")
            .execute_batch("CREATE TABLE x (y)")
            .ok();
        let connection = Connection::open(home.join("state_5.sqlite")).expect("a database");
        connection
            .execute_batch(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, cwd TEXT NOT NULL, created_at INTEGER NOT NULL, \
                 created_at_ms INTEGER, first_user_message TEXT NOT NULL DEFAULT '', archived INTEGER NOT NULL DEFAULT 0)",
            )
            .expect("a table");
        for (id, cwd, at, first, archived) in rows {
            connection
                .execute(
                    "INSERT INTO threads (id, cwd, created_at, created_at_ms, first_user_message, archived) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![id, cwd, (*at / 1000) as i64, *at as i64, first, archived],
                )
                .expect("a row");
        }
        home
    }

    #[test]
    fn the_codex_thread_is_the_newest_in_the_folder_that_names_the_ticket() {
        let home = a_codex_home(
            "codex",
            &[
                ("t-old", r"\\?\C:\work\repo", 1_000, "Work task-7 please", 0),
                ("t-new", r"\\?\C:\work\repo", 3_000, "Work task-7 please, again", 0),
                ("t-other-ticket", r"\\?\C:\work\repo", 4_000, "Work task-71", 0),
                ("t-other-folder", r"\\?\C:\work\elsewhere", 5_000, "Work task-7", 0),
                ("t-archived", r"\\?\C:\work\repo", 6_000, "Work task-7", 1),
            ],
        );
        let folder = Path::new(r"C:\work\repo");
        assert_eq!(codex_thread(&home, folder, "task-7", None).as_deref(), Some("t-new"));
        assert_eq!(
            codex_thread(&home, folder, "task-7", Some(3_500)),
            None,
            "made before the ticket started"
        );
        assert_eq!(codex_thread(&home, folder, "task-7", Some(2_000)).as_deref(), Some("t-new"));
        assert!(codex_knows(&home, "t-old"));
        assert!(!codex_knows(&home, "t-archived"));
        assert!(!codex_knows(&home, "never"));
    }

    #[test]
    fn a_codex_home_with_no_state_file_or_no_table_finds_nothing() {
        let empty = a_folder("codex-empty");
        assert_eq!(codex_thread(&empty, Path::new("C:/x"), "task-1", None), None);
        Connection::open(empty.join("state_9.sqlite"))
            .expect("db")
            .execute_batch("CREATE TABLE y (z)")
            .ok();
        assert_eq!(codex_thread(&empty, Path::new("C:/x"), "task-1", None), None);
        assert!(!codex_knows(&empty, "anything"));
    }

    #[test]
    fn the_opencode_session_is_the_earliest_top_level_one_in_the_folder_since_the_ticket_started() {
        let data = a_folder("opencode");
        let connection = Connection::open(data.join("opencode.db")).expect("a database");
        connection
            .execute_batch(
                "CREATE TABLE session (id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT NOT NULL, \
                 time_created INTEGER NOT NULL, time_archived INTEGER)",
            )
            .expect("a table");
        for (id, parent, directory, at, archived) in [
            ("ses_before", None, "C:/work/repo", 900, None),
            ("ses_ours", None, "C:/work/repo", 1_100, None),
            ("ses_child", Some("ses_ours"), "C:/work/repo", 1_050, None),
            ("ses_later", None, "C:/work/repo", 2_000, None),
            ("ses_elsewhere", None, "C:/work/other", 1_000, None),
            ("ses_archived", None, "C:/work/repo", 1_001, Some(5)),
        ] {
            connection
                .execute(
                    "INSERT INTO session (id, parent_id, directory, time_created, time_archived) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![id, parent, directory, at, archived],
                )
                .expect("a row");
        }
        assert_eq!(
            opencode_session(&data, Path::new("C:/work/repo/"), 1_000).as_deref(),
            Some("ses_ours")
        );
        assert!(opencode_knows(&data, "ses_later"));
        assert!(!opencode_knows(&data, "ses_archived"));
        assert_eq!(
            opencode_session(&a_folder("opencode-empty"), Path::new("C:/work/repo"), 0),
            None
        );
    }

    #[test]
    fn a_marker_carries_the_time_its_ticket_started() {
        let marker = pending_marker("0f9a-1234");
        assert!(marker.starts_with(PENDING));
        let at = started_at(&marker).expect("a time");
        assert!(at.abs_diff(now_ms()) < 60_000);
        assert_eq!(started_at("0f9a-1234"), None);
        assert_eq!(started_at("ses_abc"), None);
    }

    #[test]
    fn a_folder_is_compared_without_its_prefix_separators_or_trailing_slash() {
        assert_eq!(same_folder_text(r"\\?\C:\work\repo\"), same_folder_text("C:/work/repo"));
        assert!(names_the_ticket("Work task-7.", "task-7"));
        assert!(!names_the_ticket("Work task-71.", "task-7"));
    }

    /// Run an agent once in `folder` with `prompt` on standard input, and answer with what it printed.
    fn run_once(name: &str, arguments: &[&str], folder: &Path, prompt: &str) -> String {
        use std::io::Write;
        let environment = crate::services::agent_chat::AgentChat::the_environment();
        let program = unluminous_chat::provider::program(name, &environment).expect("installed");
        let mut child = std::process::Command::new(program)
            .args(arguments)
            .current_dir(folder)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("started");
        child.stdin.take().expect("stdin").write_all(prompt.as_bytes()).expect("written");
        let output = child.wait_with_output().expect("finished");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    // The live tests. Each runs a real agent and costs a few hundred tokens, so they are ignored and were run
    // by hand once, for `task-2285`:
    //
    //     cargo test -p unluminous-app --lib live_restore -- --ignored --nocapture --test-threads=1

    #[test]
    #[ignore = "runs Codex; run by hand"]
    fn live_restore_codex_ticket_session_is_found_in_codexs_own_records() {
        let folder = a_folder("live-codex-ticket");
        let key = format!("task-9{}", std::process::id() % 100_000);
        let since = now_ms();
        let printed = run_once(
            "codex",
            &["exec", "--json", "--skip-git-repo-check", "-"],
            &folder,
            &format!("This is ticket {key}. Reply with OK and nothing else."),
        );
        let started = printed
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find_map(|value| value["thread_id"].as_str().map(str::to_owned))
            .expect("codex said which thread it started");
        let home = codex_home().expect("codex's folder");
        let found = codex_thread(&home, &folder, &key, Some(since));
        println!("codex started {started}, and the ticket's lookup found {found:?}");
        assert_eq!(found.as_deref(), Some(started.as_str()));
        assert!(codex_knows(&home, &started));
    }

    #[test]
    #[ignore = "runs OpenCode; run by hand"]
    fn live_restore_opencode_ticket_session_is_found_in_opencodes_own_records() {
        let folder = a_folder("live-opencode-ticket");
        let since = now_ms();
        let printed = run_once(
            "opencode",
            &["run", "--format", "json", "--dir", &folder.display().to_string()],
            &folder,
            "This is ticket task-1. Reply with OK and nothing else.",
        );
        let started = printed
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find_map(|value| value["sessionID"].as_str().map(str::to_owned))
            .expect("opencode said which session it started");
        let data = opencode_data().expect("opencode's folder");
        let found = opencode_session(&data, &folder, since);
        println!("opencode started {started}, and the ticket's lookup found {found:?}");
        assert_eq!(found.as_deref(), Some(started.as_str()));
        assert!(opencode_knows(&data, &started));
    }
}
