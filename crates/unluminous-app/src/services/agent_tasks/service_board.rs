//! The board read and written through agent-tasks-service, the board the phone and the iPad show.
//!
//! `tasks/agent-tasks-through-the-service-tdd.md` is the design. [`ServiceBoard`] answers the same calls as
//! [`super::store::Store`], so [`super::board_store::BoardStore`] can hand either to the rest of Agent-Tasks.
//!
//! - **Reads come from a copy of `GET /tasks/board`**, every ticket with its todos and comments nested, so the
//!   pane never waits on the network while it draws. On the MacBook Pro with 33 tickets it is 334 KB in 5 ms.
//! - **A write is a request and then a fresh copy**, so the next draw shows what the service now holds.
//! - **The copy is read again when the service says something changed**, through `GET /tasks/events`.
//!
//! The HTTP is `tasks_client`, the crate the phone app uses, so the two apps call the service the same way.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tasks_client::{CreateTask, Credential, EventListener, FullBoard, StreamEvent, TasksClient, UpdateTask, UpdateTodo};

use super::model::{Author, Board, Comment, Epic, Sprint, SprintStatus, Status, Task, Todo};
use super::service_mapping::{self as mapping, assignee_word, author_word};
use super::store::{NewTask, TaskEdit};

/// How often the board is read again when the event stream has said nothing, in case the stream is down.
const READ_AGAIN_EVERY: Duration = Duration::from_secs(30);

/// What a desktop operation that only the desktop's own launcher and watchdog use answers in this mode.
pub const THE_SERVICE_RUNS_THE_AGENTS: &str = "agent-tasks-service runs the agents and the watchdog for this board";

/// The board kept by agent-tasks-service.
pub struct ServiceBoard {
    client: TasksClient,
    address: String,
    token: String,
    copy: RefCell<FullBoard>,
    read_at: RefCell<Instant>,
    changed: Arc<AtomicBool>,
    _events: EventListener,
}

impl std::fmt::Debug for ServiceBoard {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("ServiceBoard").field("address", &self.address).finish()
    }
}

/// A client error as a sentence for the pane.
/// @param doing - what was being done, such as "the board could not be read"
fn said(doing: &str) -> impl Fn(tasks_client::ClientError) -> String + '_ {
    move |problem| format!("{doing}: {problem}")
}

impl ServiceBoard {
    /// Connect to the service on this machine and read its board.
    /// @param port - the service's port on loopback
    /// @param token - the machine secret from `boards/<port>.conf`
    /// @param wake - asks the window to draw again, called when the service says the board changed
    pub fn connect(port: u16, token: &str, wake: Option<unluminous_terminal::Waker>) -> Result<Self, String> {
        let address = format!("http://127.0.0.1:{port}");
        let client = TasksClient::new(&address, Credential::Machine(token.to_owned()));
        let copy = client.full_board().map_err(said("agent-tasks-service did not answer with its board"))?;
        let changed = Arc::new(AtomicBool::new(false));
        let flag = changed.clone();
        let events = EventListener::for_client(&client, move |event| {
            if event == StreamEvent::Changed {
                flag.store(true, Ordering::Relaxed);
                if let Some(wake) = &wake {
                    wake();
                }
            }
        });
        Ok(Self { client, address, token: token.to_owned(), copy: RefCell::new(copy), read_at: RefCell::new(Instant::now()), changed, _events: events })
    }

    /// The service's address, as the pane names the board.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The machine secret, which the ticket terminal's attach client is handed.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The client, for the agent calls `mod.rs` makes in this mode.
    pub fn client(&self) -> &TasksClient {
        &self.client
    }

    /// Read the whole board again.
    pub fn read_again(&self) -> Result<(), String> {
        let copy = self.client.full_board().map_err(said("the board could not be read from agent-tasks-service"))?;
        *self.copy.borrow_mut() = copy;
        *self.read_at.borrow_mut() = Instant::now();
        Ok(())
    }

    /// Whether the board should be read again: the service said it changed, or it has been a while.
    pub fn has_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed) || self.read_at.borrow().elapsed() >= READ_AGAIN_EVERY
    }

    /// Run a write and read the board again, so the next draw shows it.
    /// @param write - the request
    fn then_read<T>(&self, write: impl FnOnce(&TasksClient) -> Result<T, String>) -> Result<T, String> {
        let answer = write(&self.client)?;
        self.read_again()?;
        Ok(answer)
    }

    // ------------------------------------------------------------------------------------ reads

    /// The board the pane draws.
    pub fn board(&self) -> Result<Board, String> {
        Ok(mapping::board_of(&self.copy.borrow()))
    }

    /// Every ticket with no sprint.
    pub fn backlog(&self) -> Result<Vec<Task>, String> {
        let mut tasks = mapping::tasks_in(&self.copy.borrow(), None);
        tasks.sort_by_key(|task| (task.position, task.id));
        Ok(tasks)
    }

    /// Every ticket in a completed sprint, the newest sprint first.
    pub fn completed(&self) -> Result<Vec<Task>, String> {
        let copy = self.copy.borrow();
        let mut sprints: Vec<&tasks_client::Sprint> = copy.sprints.iter().filter(|sprint| sprint.status == "completed").collect();
        sprints.sort_by_key(|sprint| std::cmp::Reverse(sprint.position));
        Ok(sprints.into_iter().flat_map(|sprint| self.in_lane_order(&copy, sprint.id)).collect())
    }

    /// One sprint's tickets by position, then id.
    fn in_lane_order(&self, copy: &FullBoard, sprint: i64) -> Vec<Task> {
        let mut tasks = mapping::tasks_in(copy, Some(sprint));
        tasks.sort_by_key(|task| (task.position, task.id));
        tasks
    }

    /// One ticket by id.
    /// @param id - the ticket
    pub fn task(&self, id: i64) -> Result<Option<Task>, String> {
        Ok(self.copy.borrow().tasks.iter().find(|full| full.task.id == id).map(mapping::task_of))
    }

    /// One ticket by its key.
    /// @param key - the key, such as `task-3`
    pub fn task_by_key(&self, key: &str) -> Result<Option<Task>, String> {
        Ok(self.copy.borrow().tasks.iter().find(|full| full.task.task_key.eq_ignore_ascii_case(key)).map(mapping::task_of))
    }

    /// A ticket's todos, in order.
    /// @param task - the ticket
    pub fn todos(&self, task: i64) -> Result<Vec<Todo>, String> {
        let copy = self.copy.borrow();
        let mut todos: Vec<Todo> = copy.tasks.iter().filter(|full| full.task.id == task).flat_map(|full| full.task.todos.iter().map(mapping::todo_of)).collect();
        todos.sort_by_key(|todo| (todo.position, todo.id));
        Ok(todos)
    }

    /// A ticket's comments, oldest first.
    /// @param task - the ticket
    pub fn comments(&self, task: i64) -> Result<Vec<Comment>, String> {
        let copy = self.copy.borrow();
        let mut comments: Vec<Comment> = copy.tasks.iter().filter(|full| full.task.id == task).flat_map(|full| full.task.comments.iter().map(mapping::comment_of)).collect();
        comments.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        Ok(comments)
    }

    /// Every epic, in order.
    pub fn epics(&self) -> Result<Vec<Epic>, String> {
        let mut epics: Vec<Epic> = self.copy.borrow().epics.iter().map(mapping::epic_of).collect();
        epics.sort_by_key(|epic| (epic.position, epic.id));
        Ok(epics)
    }

    /// Every sprint, in order.
    pub fn sprints(&self) -> Result<Vec<Sprint>, String> {
        let mut sprints: Vec<Sprint> = self.copy.borrow().sprints.iter().map(mapping::sprint_of).collect();
        sprints.sort_by_key(|sprint| (sprint.position, sprint.id));
        Ok(sprints)
    }

    /// The sprint the board shows.
    pub fn active_sprint(&self) -> Result<Option<Sprint>, String> {
        Ok(self.copy.borrow().active_sprint.as_ref().map(mapping::sprint_of))
    }

    /// One sprint's tickets.
    /// @param sprint - the sprint
    pub fn tasks_of_sprint(&self, sprint: i64) -> Result<Vec<Task>, String> {
        Ok(self.in_lane_order(&self.copy.borrow(), sprint))
    }

    /// How many tickets each epic has, as `(epic, count)`.
    pub fn epic_counts(&self) -> Result<Vec<(i64, i64)>, String> {
        Ok(self.counts(|full| full.task.epic_id))
    }

    /// How many tickets each sprint has, as `(sprint, count)`.
    pub fn sprint_counts(&self) -> Result<Vec<(i64, i64)>, String> {
        Ok(self.counts(|full| full.task.sprint_id))
    }

    /// Count the tickets by a field that names a group.
    /// @param group - the field
    fn counts(&self, group: impl Fn(&tasks_client::FullTask) -> Option<i64>) -> Vec<(i64, i64)> {
        let mut counts = std::collections::BTreeMap::<i64, i64>::new();
        for full in &self.copy.borrow().tasks {
            if let Some(id) = group(full) {
                *counts.entry(id).or_default() += 1;
            }
        }
        counts.into_iter().collect()
    }

    /// The tickets whose key, title or description holds the text, which the service searches.
    /// @param query - the text
    pub fn search(&self, query: &str) -> Result<Vec<Task>, String> {
        let found = self.client.search(query, 200).map_err(said("the search failed"))?;
        Ok(found.iter().map(mapping::task_of).collect())
    }

    /// The ticket a todo belongs to.
    /// @param todo - the todo
    fn task_of_todo(&self, todo: i64) -> Result<i64, String> {
        self.copy.borrow().tasks.iter().find(|full| full.task.todos.iter().any(|each| each.id == todo)).map(|full| full.task.id).ok_or_else(|| format!("todo {todo} is not on the board"))
    }

    /// The ticket a comment belongs to.
    /// @param comment - the comment
    fn task_of_comment(&self, comment: i64) -> Result<i64, String> {
        self.copy.borrow().tasks.iter().find(|full| full.task.comments.iter().any(|each| each.id == comment)).map(|full| full.task.id).ok_or_else(|| format!("comment {comment} is not on the board"))
    }
}

// ---------------------------------------------------------------------------------------- writes

impl ServiceBoard {
    /// File a ticket, then set what `POST /tasks` does not take.
    /// @param draft - the ticket's fields
    pub fn create_task(&self, draft: NewTask, _now: &str) -> Result<Task, String> {
        let body = CreateTask {
            title: draft.title.clone(),
            description: Some(draft.description.clone()).filter(|text| !text.is_empty()),
            priority: Some(draft.priority.name().to_owned()),
            assignee: Some(assignee_word(draft.assignee).to_owned()),
            model: draft.model.clone(),
            epic_id: draft.epic_id,
            sprint_id: draft.sprint_id,
            agent_project_id: None,
        };
        let created = self.client.create_task(&body).map_err(said("the ticket could not be filed"))?;
        let rest = UpdateTask {
            status: (draft.status != Status::New).then(|| draft.status.name().to_owned()),
            effort: draft.effort.clone().map(Some),
            agent_project_id: draft.project.clone().map(Some),
            ..Default::default()
        };
        if rest != UpdateTask::default() {
            self.client.update_task(created.id, &rest).map_err(said("the new ticket could not be finished"))?;
        }
        self.read_again()?;
        self.task(created.id)?.ok_or_else(|| format!("{} was filed but is not on the board", created.task_key))
    }

    /// Change a ticket's fields. `endpoint` and `jira_key` are the desktop's own and are not sent.
    /// @param id - the ticket
    /// @param edit - what changed
    pub fn edit_task(&self, id: i64, edit: &TaskEdit, _now: &str) -> Result<(), String> {
        let patch = UpdateTask {
            title: edit.title.clone(),
            description: edit.description.clone(),
            priority: edit.priority.map(|priority| priority.name().to_owned()),
            assignee: edit.assignee.map(|assignee| assignee_word(assignee).to_owned()),
            model: edit.model.clone().flatten(),
            epic_id: edit.epic_id,
            effort: edit.effort.clone(),
            agent_project_id: edit.project.clone(),
            ..Default::default()
        };
        self.then_read(|client| {
            if patch != UpdateTask::default() {
                client.update_task(id, &patch).map_err(said("the ticket could not be changed"))?;
            }
            if matches!(edit.model, Some(None)) {
                client.clear_model(id).map_err(said("the model could not be cleared"))?;
            }
            Ok(())
        })
    }

    /// Move a ticket to a lane and a place in it.
    /// @param id - the ticket
    /// @param status - the lane
    /// @param position - the place, counted from 0
    pub fn move_task(&self, id: i64, status: Status, position: i64, _now: &str) -> Result<(), String> {
        let (sprint, order) = mapping::lane_after_move(&self.copy.borrow(), id, status, position)?;
        self.then_read(|client| client.reorder(&order, sprint, status.name()).map_err(said("the ticket could not be moved")))
    }

    /// Delete a ticket with its todos and comments.
    /// @param id - the ticket
    pub fn delete_task(&self, id: i64) -> Result<(), String> {
        self.then_read(|client| client.delete_task(id).map_err(said("the ticket could not be deleted")))
    }

    /// Add a todo to the end of a ticket's list.
    /// @param task - the ticket
    /// @param text - the todo
    pub fn add_todo(&self, task: i64, text: &str, _now: &str) -> Result<Todo, String> {
        self.then_read(|client| client.add_todo(task, text).map(|todo| mapping::todo_of(&todo)).map_err(said("the todo could not be added")))
    }

    /// Tick or untick a todo.
    /// @param id - the todo
    /// @param done - ticked or not
    pub fn set_todo_done(&self, id: i64, done: bool, _now: &str) -> Result<(), String> {
        let task = self.task_of_todo(id)?;
        self.then_read(|client| client.update_todo(task, id, &UpdateTodo::done(done)).map(|_| ()).map_err(said("the todo could not be changed")))
    }

    /// Change a todo's text.
    /// @param id - the todo
    /// @param text - the new text
    pub fn set_todo_text(&self, id: i64, text: &str) -> Result<(), String> {
        let task = self.task_of_todo(id)?;
        let body = UpdateTodo { text: Some(text.to_owned()), ..Default::default() };
        self.then_read(|client| client.update_todo(task, id, &body).map(|_| ()).map_err(said("the todo could not be changed")))
    }

    /// Delete a todo.
    /// @param id - the todo
    pub fn delete_todo(&self, id: i64) -> Result<(), String> {
        let task = self.task_of_todo(id)?;
        self.then_read(|client| client.delete_todo(task, id).map_err(said("the todo could not be deleted")))
    }

    /// Post a comment.
    /// @param task - the ticket
    /// @param author - who wrote it
    /// @param body - the comment
    pub fn add_comment(&self, task: i64, author: Author, body: &str, _now: &str) -> Result<Comment, String> {
        self.then_read(|client| client.add_comment(task, body, author_word(author)).map(|comment| mapping::comment_of(&comment)).map_err(said("the comment could not be posted")))
    }

    /// Change what a comment says.
    /// @param id - the comment
    /// @param body - the new text
    pub fn edit_comment(&self, id: i64, body: &str, _now: &str) -> Result<Comment, String> {
        let task = self.task_of_comment(id)?;
        self.then_read(|client| client.edit_comment(task, id, body).map(|comment| mapping::comment_of(&comment)).map_err(said("the comment could not be changed")))
    }
}

// ------------------------------------------------------------------------------ sprints and epics

impl ServiceBoard {
    /// Make an epic.
    /// @param name - its name
    /// @param color - its color as `#RRGGBB`
    pub fn create_epic(&self, name: &str, color: &str) -> Result<Epic, String> {
        self.then_read(|client| client.create_epic(name, color).map(|epic| mapping::epic_of(&epic)).map_err(said("the epic could not be made")))
    }

    /// Rename or recolor an epic.
    /// @param id - the epic
    /// @param name - the new name, or `None`
    /// @param color - the new color, or `None`
    pub fn edit_epic(&self, id: i64, name: Option<&str>, color: Option<&str>) -> Result<(), String> {
        self.then_read(|client| client.update_epic(id, name, color).map_err(said("the epic could not be changed")))
    }

    /// Delete an epic. Its tickets keep existing with no epic.
    /// @param id - the epic
    pub fn delete_epic(&self, id: i64, _now: &str) -> Result<(), String> {
        self.then_read(|client| client.delete_epic(id).map_err(said("the epic could not be deleted")))
    }

    /// Make a sprint.
    /// @param name - its name
    /// @param status - planned or active. Making one active demotes the sprint that was
    pub fn create_sprint(&self, name: &str, status: SprintStatus, _now: &str) -> Result<Sprint, String> {
        self.then_read(|client| client.create_sprint(name, status.name()).map(|sprint| mapping::sprint_of(&sprint)).map_err(said("the sprint could not be made")))
    }

    /// Put a ticket in a sprint, or in the backlog with `None`.
    /// @param id - the ticket
    /// @param sprint - the sprint
    pub fn set_sprint_of(&self, id: i64, sprint: Option<i64>, _now: &str) -> Result<(), String> {
        self.then_read(|client| client.update_task(id, &UpdateTask::sprint(sprint)).map(|_| ()).map_err(said("the ticket could not be moved to that sprint")))
    }

    /// Make a sprint the one the board shows.
    /// @param id - the sprint
    pub fn make_sprint_active(&self, id: i64) -> Result<(), String> {
        self.then_read(|client| client.update_sprint(id, None, Some("active")).map_err(said("the sprint could not be made active")))
    }

    /// Close a sprint, and answer how many of its tickets went to the backlog.
    /// @param id - the sprint
    pub fn complete_sprint(&self, id: i64, _now: &str) -> Result<usize, String> {
        let unfinished = self.copy.borrow().tasks.iter().filter(|full| full.task.sprint_id == Some(id) && mapping::status_of(&full.task.status) != Status::AgentDone).count();
        self.then_read(|client| client.complete_sprint(id).map_err(said("the sprint could not be closed")))?;
        Ok(unfinished)
    }

    /// Rename a sprint.
    /// @param id - the sprint
    /// @param name - the new name
    pub fn rename_sprint(&self, id: i64, name: &str) -> Result<(), String> {
        self.then_read(|client| client.update_sprint(id, Some(name), None).map_err(said("the sprint could not be renamed")))
    }

    /// Delete a sprint. Its tickets go to the backlog.
    /// @param id - the sprint
    pub fn delete_sprint(&self, id: i64, _now: &str) -> Result<(), String> {
        self.then_read(|client| client.delete_sprint(id).map_err(said("the sprint could not be deleted")))
    }
}
