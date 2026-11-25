//! The board Agent-Tasks has open: its own file, or agent-tasks-service.
//!
//! Every command reaches the board through `AgentTasks::store()`, and this is what that answers. It has every
//! method [`Store`] has, with the same signature, and sends each call to whichever board is open, so the rest of
//! Agent-Tasks does not know which one answered. `board-source` in the plugin's `settings.conf` chooses.
//! `tasks/agent-tasks-through-the-service-tdd.md` is the design.

use std::path::{Path, PathBuf};

use super::model::{Assignee, Author, Board, Comment, Epic, Sprint, SprintStatus, Status, Task, Todo};
use super::service_board::{ServiceBoard, THE_SERVICE_RUNS_THE_AGENTS};
use super::store::{Candidate, NewTask, Schedule, Store, TaskEdit};

/// The board that is open.
#[derive(Debug)]
pub enum BoardStore {
    /// The plugin's own Inillucent file.
    File(Store),
    /// The board agent-tasks-service keeps, which the phone and the iPad show.
    Service(ServiceBoard),
}

/// Methods both boards answer the same way: the call is passed to whichever is open.
macro_rules! either {
    ($( $(#[$doc:meta])* fn $name:ident(&self $(, $arg:ident: $ty:ty)*) -> $answer:ty; )*) => {
        $( $(#[$doc])* pub fn $name(&self $(, $arg: $ty)*) -> $answer {
            match self {
                BoardStore::File(store) => store.$name($($arg),*),
                BoardStore::Service(board) => board.$name($($arg),*),
            }
        } )*
    };
}

/// Methods only the plugin's own launcher and watchdog use. agent-tasks-service runs the agents and the
/// watchdog for its board, so on that board each one answers why it does nothing.
macro_rules! file_only {
    ($( $(#[$doc:meta])* fn $name:ident(&self $(, $arg:ident: $ty:ty)*) -> Result<$ok:ty, String>; )*) => {
        $( $(#[$doc])* pub fn $name(&self $(, $arg: $ty)*) -> Result<$ok, String> {
            match self {
                BoardStore::File(store) => store.$name($($arg),*),
                BoardStore::Service(_) => Err(THE_SERVICE_RUNS_THE_AGENTS.to_owned()),
            }
        } )*
    };
}

impl BoardStore {
    either! {
        /// The board the pane draws: the active sprint in four lanes, and the epics.
        fn board(&self) -> Result<Board, String>;
        /// Every ticket with no sprint.
        fn backlog(&self) -> Result<Vec<Task>, String>;
        /// Every ticket in a completed sprint.
        fn completed(&self) -> Result<Vec<Task>, String>;
        /// One ticket by id.
        fn task(&self, id: i64) -> Result<Option<Task>, String>;
        /// One ticket by its key.
        fn task_by_key(&self, key: &str) -> Result<Option<Task>, String>;
        /// A ticket's todos.
        fn todos(&self, task: i64) -> Result<Vec<Todo>, String>;
        /// A ticket's comments.
        fn comments(&self, task: i64) -> Result<Vec<Comment>, String>;
        /// Every epic.
        fn epics(&self) -> Result<Vec<Epic>, String>;
        /// Every sprint.
        fn sprints(&self) -> Result<Vec<Sprint>, String>;
        /// The sprint the board shows.
        fn active_sprint(&self) -> Result<Option<Sprint>, String>;
        /// File a ticket.
        fn create_task(&self, draft: NewTask, now: &str) -> Result<Task, String>;
        /// Change a ticket's fields.
        fn edit_task(&self, id: i64, edit: &TaskEdit, now: &str) -> Result<(), String>;
        /// Move a ticket to a lane and a place in it.
        fn move_task(&self, id: i64, status: Status, position: i64, now: &str) -> Result<(), String>;
        /// Delete a ticket with its todos and comments.
        fn delete_task(&self, id: i64) -> Result<(), String>;
        /// Add a todo.
        fn add_todo(&self, task: i64, text: &str, now: &str) -> Result<Todo, String>;
        /// Tick or untick a todo.
        fn set_todo_done(&self, id: i64, done: bool, now: &str) -> Result<(), String>;
        /// Change a todo's text.
        fn set_todo_text(&self, id: i64, text: &str) -> Result<(), String>;
        /// Delete a todo.
        fn delete_todo(&self, id: i64) -> Result<(), String>;
        /// Post a comment.
        fn add_comment(&self, task: i64, author: Author, body: &str, now: &str) -> Result<Comment, String>;
        /// Change what a comment says.
        fn edit_comment(&self, id: i64, body: &str, now: &str) -> Result<Comment, String>;
        /// Make an epic.
        fn create_epic(&self, name: &str, color: &str) -> Result<Epic, String>;
        /// Rename or recolor an epic.
        fn edit_epic(&self, id: i64, name: Option<&str>, color: Option<&str>) -> Result<(), String>;
        /// Delete an epic.
        fn delete_epic(&self, id: i64, now: &str) -> Result<(), String>;
        /// Make a sprint.
        fn create_sprint(&self, name: &str, status: SprintStatus, now: &str) -> Result<Sprint, String>;
        /// One sprint's tickets.
        fn tasks_of_sprint(&self, sprint: i64) -> Result<Vec<Task>, String>;
        /// Put a ticket in a sprint or the backlog.
        fn set_sprint_of(&self, id: i64, sprint: Option<i64>, now: &str) -> Result<(), String>;
        /// Make a sprint the active one.
        fn make_sprint_active(&self, id: i64) -> Result<(), String>;
        /// Close a sprint.
        fn complete_sprint(&self, id: i64, now: &str) -> Result<usize, String>;
        /// Rename a sprint.
        fn rename_sprint(&self, id: i64, name: &str) -> Result<(), String>;
        /// Delete a sprint.
        fn delete_sprint(&self, id: i64, now: &str) -> Result<(), String>;
        /// How many tickets each epic has.
        fn epic_counts(&self) -> Result<Vec<(i64, i64)>, String>;
        /// How many tickets each sprint has.
        fn sprint_counts(&self) -> Result<Vec<(i64, i64)>, String>;
        /// The tickets whose key, title or description holds the text.
        fn search(&self, query: &str) -> Result<Vec<Task>, String>;
    }

    file_only! {
        /// Take an unclaimed ticket for an agent this window started.
        fn claim(&self, id: i64, session: &str, kind: Assignee, owner: &str, now: &str) -> Result<bool, String>;
        /// Give a claim back.
        fn release(&self, id: i64, now: &str) -> Result<(), String>;
        /// Record the session a ticket's agent is having.
        fn set_session(&self, id: i64, session: &str, now: &str) -> Result<(), String>;
        /// Record that a ticket's agent is working.
        fn heartbeat(&self, id: i64, minutes: Option<i64>, now: &str) -> Result<(), String>;
        /// Count a watchdog strike.
        fn strike(&self, id: i64) -> Result<i64, String>;
        /// Count a watchdog nudge.
        fn nudge(&self, id: i64, now: &str) -> Result<i64, String>;
        /// Hand a stalled ticket back to New.
        fn reclaim(&self, id: i64, now: &str) -> Result<(), String>;
        /// Delete every ticket, leaving the epics and sprints.
        fn clear_the_tickets(&self) -> Result<(i64, i64, i64), String>;
        /// Copy the board's file.
        fn copy_the_file(&self, to: &Path) -> Result<PathBuf, String>;
    }

    /// Which window took a ticket. The service's board has no windows, so it answers none.
    /// @param id - the ticket
    pub fn owner_of(&self, id: i64) -> Result<Option<String>, String> {
        match self {
            BoardStore::File(store) => store.owner_of(id),
            BoardStore::Service(_) => Ok(None),
        }
    }

    /// The tickets the desktop's watchdog looks at. None on the service's board, whose own watchdog runs.
    /// @param now - the time now
    /// @param default_lease_minutes - the lease a ticket that names none has
    pub fn watchdog_candidates(&self, now: &str, default_lease_minutes: i64) -> Result<Vec<Candidate>, String> {
        match self {
            BoardStore::File(store) => store.watchdog_candidates(now, default_lease_minutes),
            BoardStore::Service(_) => Ok(Vec::new()),
        }
    }

    /// The scheduled runs in the board's file. None on the service's board.
    pub fn schedules(&self) -> Result<Vec<Schedule>, String> {
        match self {
            BoardStore::File(store) => store.schedules(),
            BoardStore::Service(_) => Ok(Vec::new()),
        }
    }

    /// Where the board is: its file, or the service's address.
    pub fn path(&self) -> &Path {
        match self {
            BoardStore::File(store) => store.path(),
            BoardStore::Service(board) => Path::new(board.address()),
        }
    }

    /// The service's board, when that is the one open.
    pub fn service(&self) -> Option<&ServiceBoard> {
        match self {
            BoardStore::File(_) => None,
            BoardStore::Service(board) => Some(board),
        }
    }
}
