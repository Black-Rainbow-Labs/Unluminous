//! The board read and written through agent-tasks-service, the board the phone and the iPad show.
//!
//! `tasks/agent-tasks-through-the-service-tdd.md` is the design. [`ServiceBoard`] answers the same calls as
//! [`super::store::Store`], so [`super::board_store::BoardStore`] can hand either to the rest of Agent-Tasks.
//!
//! - **Reads come from a copy of `GET /tasks/board?view=summary`**, every ticket as a card with its counts, so the
//!   pane never waits on the network while it draws. A ticket is read in full (`GET /tasks/:id`, with its
//!   description, todos and comments) the first time something asks for it, and kept. The full board was read here
//!   until task-2879: 334 KB on the Mac's board, and 33.8 MB on the PC board, read again on every change.
//! - **A write is a request and then a fresh copy of what it changed**: the one ticket, or the whole summary for a
//!   change to lanes, sprints or epics.
//! - **The copy is read again when the service says something changed**, through `GET /tasks/events`: just the
//!   tickets the events named, and the whole summary when an event named none or the stream reconnected.
//! - **The board can be on another computer** (task-2879): [`ServiceBoard::connect_to`] takes an address such as
//!   `https://tasks.jasonmcaffee.com` and a password session, where [`ServiceBoard::connect`] takes the machine
//!   secret of a board on this computer.
//!
//! The HTTP is `tasks_client`, the crate the phone app uses, so the two apps call the service the same way.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tasks_client::{
    AgentProject, CreateTask, Credential, EventListener, FullBoard, StreamEvent, TasksClient,
    UpdateTask, UpdateTodo,
};

use super::model::{Author, Board, Comment, Epic, Sprint, SprintStatus, Status, Task, Todo};
use super::service_mapping::{self as mapping, assignee_word, author_word};
use super::store::{NewTask, TaskEdit};

/// How often the board is read again when the event stream has said nothing, in case the stream is down.
const READ_AGAIN_EVERY: Duration = Duration::from_secs(30);

/// What a desktop operation that only the desktop's own launcher and watchdog use answers in this mode.
pub const THE_SERVICE_RUNS_THE_AGENTS: &str =
    "agent-tasks-service runs the agents and the watchdog for this board";

/// What the event stream has said changed since the board was last read.
#[derive(Debug, Default)]
struct Pending {
    /// The whole board: an event named no ticket, or the stream reconnected.
    whole: bool,
    /// The tickets the events named.
    tickets: Vec<i64>,
}

/// The board kept by agent-tasks-service.
pub struct ServiceBoard {
    client: TasksClient,
    address: String,
    token: String,
    /// Whether `token` is a password session (a board on another computer) rather than the machine secret.
    session: bool,
    copy: RefCell<FullBoard>,
    /// The tickets in `copy` that were read in full. Every other ticket is a summary with no description, todos or
    /// comments.
    whole_tickets: RefCell<HashSet<i64>>,
    /// What the event stream said changed, filled on its thread and emptied by [`Self::read_again`].
    pending: Arc<Mutex<Pending>>,
    /// `GET /tasks/agent-projects`, read with the board: the projects a ticket can be in, the same list the phone has.
    projects: RefCell<Vec<AgentProject>>,
    /// `GET /tasks/agent-projects/root`, read with the projects.
    root: RefCell<Option<String>>,
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

/// Record what the event stream said changed: the tickets it named, or the whole board for `None`.
/// @param pending - the record
/// @param tickets - the tickets, or `None` for the whole board
fn note(pending: &Mutex<Pending>, tickets: Option<Vec<i64>>) {
    let mut pending = pending.lock().expect("the pending list is not poisoned");
    match tickets {
        Some(tickets) => {
            for id in tickets {
                if !pending.tickets.contains(&id) {
                    pending.tickets.push(id);
                }
            }
        }
        None => pending.whole = true,
    }
}

impl ServiceBoard {
    /// Connect to the service on this machine and read its board.
    /// @param port - the service's port on loopback
    /// @param token - the machine secret from `boards/<port>.conf`
    /// @param wake - asks the window to draw again, called when the service says the board changed
    pub fn connect(
        port: u16,
        token: &str,
        wake: Option<unluminous_terminal::Waker>,
    ) -> Result<Self, String> {
        Self::open(&format!("http://127.0.0.1:{port}"), token, false, wake)
    }

    /// Connect to a board on another computer, such as the PC board at `https://tasks.jasonmcaffee.com`, with a
    /// password session from `plugins run agent-tasks sign-in` (task-2879).
    /// @param address - the board's address
    /// @param session - the session token the sign in answered
    /// @param wake - asks the window to draw again, called when the service says the board changed
    pub fn connect_to(
        address: &str,
        session: &str,
        wake: Option<unluminous_terminal::Waker>,
    ) -> Result<Self, String> {
        Self::open(address.trim().trim_end_matches('/'), session, true, wake)
    }

    /// Read the board's summary and start listening to its events.
    /// @param address - the board's address
    /// @param token - the machine secret, or a session token when `session` is set
    /// @param session - whether `token` is a session
    /// @param wake - asks the window to draw again
    fn open(
        address: &str,
        token: &str,
        session: bool,
        wake: Option<unluminous_terminal::Waker>,
    ) -> Result<Self, String> {
        let credential = match session {
            true => Credential::Session(token.to_owned()),
            false => Credential::Machine(token.to_owned()),
        };
        // Every change made here is a person's, and a board behind ai-service records it as one (task-2879).
        let client = TasksClient::new(address, credential).acting_for_person();
        let copy = client
            .summary_board()
            .map_err(said("agent-tasks-service did not answer with its board"))?;
        let changed = Arc::new(AtomicBool::new(false));
        let pending: Arc<Mutex<Pending>> = Arc::default();
        let (flag, noted) = (changed.clone(), pending.clone());
        let events = EventListener::for_client_with_tickets(&client, move |event, tickets| {
            if event == StreamEvent::Changed {
                note(&noted, tickets);
                flag.store(true, Ordering::Relaxed);
                if let Some(wake) = &wake {
                    wake();
                }
            }
        });
        let projects = RefCell::new(client.agent_projects().unwrap_or_default());
        let root = RefCell::new(client.agent_projects_root().ok().and_then(|answer| answer.root));
        Ok(Self {
            client,
            address: address.to_owned(),
            token: token.to_owned(),
            session,
            copy: RefCell::new(copy),
            whole_tickets: RefCell::default(),
            pending,
            projects,
            root,
            read_at: RefCell::new(Instant::now()),
            changed,
            _events: events,
        })
    }

    /// Whether the credential is a password session, which the ticket terminal presents as a bearer token.
    pub fn uses_a_session(&self) -> bool {
        self.session
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

    /// Read again what changed: the tickets the events named, or the whole summary when an event named none, the
    /// stream reconnected, or nothing named anything, which is a caller asking for a fresh board.
    pub fn read_again(&self) -> Result<(), String> {
        let pending =
            std::mem::take(&mut *self.pending.lock().expect("the pending list is not poisoned"));
        if !pending.whole && !pending.tickets.is_empty() {
            for id in pending.tickets {
                self.read_ticket(id)?;
            }
            *self.read_at.borrow_mut() = Instant::now();
            return Ok(());
        }
        self.read_the_summary()?;
        // A ticket the same burst named may have changed in a way its summary does not show, such as an edited
        // comment, so a ticket already read in full is read in full again.
        for id in pending.tickets {
            if self.whole_tickets.borrow().contains(&id) {
                self.read_ticket(id)?;
            }
        }
        Ok(())
    }

    /// Read one ticket in full and put it in the copy, or take it out when the service no longer has it.
    /// @param id - the ticket
    fn read_ticket(&self, id: i64) -> Result<(), String> {
        let full = match self.client.full_task(id) {
            Ok(full) => full,
            Err(tasks_client::ClientError::NotFound) => {
                self.copy.borrow_mut().tasks.retain(|each| each.task.id != id);
                self.whole_tickets.borrow_mut().remove(&id);
                return Ok(());
            }
            Err(problem) => {
                return Err(said("a ticket could not be read from agent-tasks-service")(problem))
            }
        };
        let mut copy = self.copy.borrow_mut();
        match copy.tasks.iter_mut().find(|each| each.task.id == id) {
            Some(each) => *each = full,
            None => copy.tasks.push(full),
        }
        self.whole_tickets.borrow_mut().insert(id);
        Ok(())
    }

    /// The ticket with every field, read in full the first time it is asked for.
    /// @param id - the ticket
    fn whole(&self, id: i64) -> Result<(), String> {
        let known = self.copy.borrow().tasks.iter().any(|each| each.task.id == id);
        if known && !self.whole_tickets.borrow().contains(&id) {
            self.read_ticket(id)?;
        }
        Ok(())
    }

    /// Read the whole summary again, keeping each ticket already read in full that has not changed since.
    fn read_the_summary(&self) -> Result<(), String> {
        let mut fresh = self
            .client
            .summary_board()
            .map_err(said("the board could not be read from agent-tasks-service"))?;
        {
            let old = self.copy.borrow();
            let mut whole = self.whole_tickets.borrow_mut();
            for each in fresh.tasks.iter_mut() {
                // A new comment or todo does not change a ticket's updatedAt, so the counts are compared too.
                let kept = old.tasks.iter().find(|before| {
                    before.task.id == each.task.id
                        && whole.contains(&each.task.id)
                        && before.updated_at == each.updated_at
                        && each
                            .comment_count
                            .is_none_or(|count| count == before.task.comments.len() as i64)
                        && each
                            .todo_count
                            .is_none_or(|count| count == before.task.todos.len() as i64)
                        && each.todo_done_count.is_none_or(|count| {
                            count
                                == before.task.todos.iter().filter(|todo| todo.done).count() as i64
                        })
                });
                match kept {
                    Some(before) => *each = before.clone(),
                    None => {
                        whole.remove(&each.task.id);
                    }
                }
            }
        }
        *self.copy.borrow_mut() = fresh;
        // A list that fails to load keeps the one before, so a dropdown does not empty itself on one bad read.
        if let Ok(projects) = self.client.agent_projects() {
            *self.projects.borrow_mut() = projects;
        }
        if let Ok(answer) = self.client.agent_projects_root() {
            *self.root.borrow_mut() = answer.root;
        }
        *self.read_at.borrow_mut() = Instant::now();
        Ok(())
    }

    /// The projects a ticket can be in, as of the last read: every project the service lists, which includes every
    /// folder in its projects folder.
    pub fn projects(&self) -> Vec<AgentProject> {
        self.projects.borrow().clone()
    }

    /// The service's projects folder, as of the last read. Every folder in it is a project, and a ticket with no
    /// project runs in it.
    pub fn projects_root(&self) -> Result<Option<String>, String> {
        Ok(self.root.borrow().clone())
    }

    /// Change the service's projects folder for every client, then read the board and its projects again.
    /// @param root - the folder's full path on this machine
    pub fn set_projects_root(&self, root: &str) -> Result<Option<String>, String> {
        let saved = self.then_read(|client| {
            client.set_agent_projects_root(root).map_err(said("the projects folder was not saved"))
        })?;
        Ok(saved.root)
    }

    /// The id of the project a value names: a project's id, its name, or the folder it is in. `None` when no
    /// project matches.
    /// @param value - what a person or an agent typed or chose
    pub fn project_id_for(&self, value: &str) -> Option<String> {
        let value = value.trim();
        let folder = value.trim_end_matches('/');
        self.projects
            .borrow()
            .iter()
            .find(|project| {
                project.id == value
                    || project.name == value
                    || project.dir.as_deref().map(|dir| dir.trim_end_matches('/')) == Some(folder)
            })
            .map(|project| project.id.clone())
    }

    /// Whether the board should be read again: the service said it changed, or it has been a while, which reads
    /// the whole summary in case the stream missed something.
    pub fn has_changed(&self) -> bool {
        if self.changed.swap(false, Ordering::Relaxed) {
            return true;
        }
        if self.read_at.borrow().elapsed() >= READ_AGAIN_EVERY {
            note(&self.pending, None);
            return true;
        }
        false
    }

    /// Run a write and read the whole summary again, for a change to lanes, sprints or epics.
    /// @param write - the request
    fn then_read<T>(
        &self,
        write: impl FnOnce(&TasksClient) -> Result<T, String>,
    ) -> Result<T, String> {
        let answer = write(&self.client)?;
        self.read_the_summary()?;
        Ok(answer)
    }

    /// Run a write to one ticket and read that ticket again, so the next draw shows it.
    /// @param id - the ticket
    /// @param write - the request
    fn then_read_ticket<T>(
        &self,
        id: i64,
        write: impl FnOnce(&TasksClient) -> Result<T, String>,
    ) -> Result<T, String> {
        let answer = write(&self.client)?;
        self.read_ticket(id)?;
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
        let mut sprints: Vec<&tasks_client::Sprint> =
            copy.sprints.iter().filter(|sprint| sprint.status == "completed").collect();
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
        self.whole(id)?;
        Ok(self.copy.borrow().tasks.iter().find(|full| full.task.id == id).map(mapping::task_of))
    }

    /// One ticket by its key.
    /// @param key - the key, such as `task-3`
    pub fn task_by_key(&self, key: &str) -> Result<Option<Task>, String> {
        let id = self
            .copy
            .borrow()
            .tasks
            .iter()
            .find(|full| full.task.task_key.eq_ignore_ascii_case(key))
            .map(|full| full.task.id);
        match id {
            Some(id) => self.task(id),
            None => Ok(None),
        }
    }

    /// A ticket's todos, in order.
    /// @param task - the ticket
    pub fn todos(&self, task: i64) -> Result<Vec<Todo>, String> {
        self.whole(task)?;
        let copy = self.copy.borrow();
        let mut todos: Vec<Todo> = copy
            .tasks
            .iter()
            .filter(|full| full.task.id == task)
            .flat_map(|full| full.task.todos.iter().map(mapping::todo_of))
            .collect();
        todos.sort_by_key(|todo| (todo.position, todo.id));
        Ok(todos)
    }

    /// A ticket's comments, oldest first.
    /// @param task - the ticket
    pub fn comments(&self, task: i64) -> Result<Vec<Comment>, String> {
        self.whole(task)?;
        let copy = self.copy.borrow();
        let mut comments: Vec<Comment> = copy
            .tasks
            .iter()
            .filter(|full| full.task.id == task)
            .flat_map(|full| full.task.comments.iter().map(mapping::comment_of))
            .collect();
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
        let mut sprints: Vec<Sprint> =
            self.copy.borrow().sprints.iter().map(mapping::sprint_of).collect();
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
        self.copy
            .borrow()
            .tasks
            .iter()
            .find(|full| full.task.todos.iter().any(|each| each.id == todo))
            .map(|full| full.task.id)
            .ok_or_else(|| format!("todo {todo} is not on the board"))
    }

    /// The ticket a comment belongs to.
    /// @param comment - the comment
    fn task_of_comment(&self, comment: i64) -> Result<i64, String> {
        self.copy
            .borrow()
            .tasks
            .iter()
            .find(|full| full.task.comments.iter().any(|each| each.id == comment))
            .map(|full| full.task.id)
            .ok_or_else(|| format!("comment {comment} is not on the board"))
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
            // The PC: a ticket filed here runs here. The Mac is chosen on the web board or the phone (task-2931).
            machine: None,
        };
        let created =
            self.client.create_task(&body).map_err(said("the ticket could not be filed"))?;
        let rest = UpdateTask {
            status: (draft.status != Status::New).then(|| draft.status.name().to_owned()),
            effort: draft.effort.clone().map(Some),
            agent_project_id: draft.project.clone().map(Some),
            ..Default::default()
        };
        if rest != UpdateTask::default() {
            self.client
                .update_task(created.id, &rest)
                .map_err(said("the new ticket could not be finished"))?;
        }
        self.read_ticket(created.id)?;
        self.task(created.id)?
            .ok_or_else(|| format!("{} was filed but is not on the board", created.task_key))
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
        self.then_read_ticket(id, |client| {
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
    pub fn move_task(
        &self,
        id: i64,
        status: Status,
        position: i64,
        _now: &str,
    ) -> Result<(), String> {
        let (sprint, order) = mapping::lane_after_move(&self.copy.borrow(), id, status, position)?;
        self.then_read(|client| {
            client
                .reorder(&order, sprint, status.name())
                .map_err(said("the ticket could not be moved"))
        })
    }

    /// Delete a ticket with its todos and comments.
    /// @param id - the ticket
    pub fn delete_task(&self, id: i64) -> Result<(), String> {
        self.then_read_ticket(id, |client| {
            client.delete_task(id).map_err(said("the ticket could not be deleted"))
        })
    }

    /// Add a todo to the end of a ticket's list.
    /// @param task - the ticket
    /// @param text - the todo
    pub fn add_todo(&self, task: i64, text: &str, _now: &str) -> Result<Todo, String> {
        self.then_read_ticket(task, |client| {
            client
                .add_todo(task, text)
                .map(|todo| mapping::todo_of(&todo))
                .map_err(said("the todo could not be added"))
        })
    }

    /// Tick or untick a todo.
    /// @param id - the todo
    /// @param done - ticked or not
    pub fn set_todo_done(&self, id: i64, done: bool, _now: &str) -> Result<(), String> {
        let task = self.task_of_todo(id)?;
        self.then_read_ticket(task, |client| {
            client
                .update_todo(task, id, &UpdateTodo::done(done))
                .map(|_| ())
                .map_err(said("the todo could not be changed"))
        })
    }

    /// Change a todo's text.
    /// @param id - the todo
    /// @param text - the new text
    pub fn set_todo_text(&self, id: i64, text: &str) -> Result<(), String> {
        let task = self.task_of_todo(id)?;
        let body = UpdateTodo { text: Some(text.to_owned()), ..Default::default() };
        self.then_read_ticket(task, |client| {
            client
                .update_todo(task, id, &body)
                .map(|_| ())
                .map_err(said("the todo could not be changed"))
        })
    }

    /// Delete a todo.
    /// @param id - the todo
    pub fn delete_todo(&self, id: i64) -> Result<(), String> {
        let task = self.task_of_todo(id)?;
        self.then_read_ticket(task, |client| {
            client.delete_todo(task, id).map_err(said("the todo could not be deleted"))
        })
    }

    /// Post a comment.
    /// @param task - the ticket
    /// @param author - who wrote it
    /// @param body - the comment
    pub fn add_comment(
        &self,
        task: i64,
        author: Author,
        body: &str,
        _now: &str,
    ) -> Result<Comment, String> {
        self.then_read_ticket(task, |client| {
            client
                .add_comment(task, body, author_word(author))
                .map(|comment| mapping::comment_of(&comment))
                .map_err(said("the comment could not be posted"))
        })
    }

    /// Change what a comment says.
    /// @param id - the comment
    /// @param body - the new text
    pub fn edit_comment(&self, id: i64, body: &str, _now: &str) -> Result<Comment, String> {
        let task = self.task_of_comment(id)?;
        self.then_read_ticket(task, |client| {
            client
                .edit_comment(task, id, body)
                .map(|comment| mapping::comment_of(&comment))
                .map_err(said("the comment could not be changed"))
        })
    }
}

// ------------------------------------------------------------------------------ sprints and epics

impl ServiceBoard {
    /// Make an epic.
    /// @param name - its name
    /// @param color - its color as `#RRGGBB`
    pub fn create_epic(&self, name: &str, color: &str) -> Result<Epic, String> {
        self.then_read(|client| {
            client
                .create_epic(name, color)
                .map(|epic| mapping::epic_of(&epic))
                .map_err(said("the epic could not be made"))
        })
    }

    /// Rename or recolor an epic.
    /// @param id - the epic
    /// @param name - the new name, or `None`
    /// @param color - the new color, or `None`
    pub fn edit_epic(
        &self,
        id: i64,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<(), String> {
        self.then_read(|client| {
            client.update_epic(id, name, color).map_err(said("the epic could not be changed"))
        })
    }

    /// Delete an epic. Its tickets keep existing with no epic.
    /// @param id - the epic
    pub fn delete_epic(&self, id: i64, _now: &str) -> Result<(), String> {
        self.then_read(|client| {
            client.delete_epic(id).map_err(said("the epic could not be deleted"))
        })
    }

    /// Make a sprint.
    /// @param name - its name
    /// @param status - planned or active. Making one active demotes the sprint that was
    pub fn create_sprint(
        &self,
        name: &str,
        status: SprintStatus,
        _now: &str,
    ) -> Result<Sprint, String> {
        self.then_read(|client| {
            client
                .create_sprint(name, status.name())
                .map(|sprint| mapping::sprint_of(&sprint))
                .map_err(said("the sprint could not be made"))
        })
    }

    /// Put a ticket in a sprint, or in the backlog with `None`.
    /// @param id - the ticket
    /// @param sprint - the sprint
    pub fn set_sprint_of(&self, id: i64, sprint: Option<i64>, _now: &str) -> Result<(), String> {
        self.then_read_ticket(id, |client| {
            client
                .update_task(id, &UpdateTask::sprint(sprint))
                .map(|_| ())
                .map_err(said("the ticket could not be moved to that sprint"))
        })
    }

    /// Make a sprint the one the board shows.
    /// @param id - the sprint
    pub fn make_sprint_active(&self, id: i64) -> Result<(), String> {
        self.then_read(|client| {
            client
                .update_sprint(id, None, Some("active"))
                .map_err(said("the sprint could not be made active"))
        })
    }

    /// Close a sprint, and answer how many of its tickets went to the backlog.
    /// @param id - the sprint
    pub fn complete_sprint(&self, id: i64, _now: &str) -> Result<usize, String> {
        let unfinished = self
            .copy
            .borrow()
            .tasks
            .iter()
            .filter(|full| {
                full.task.sprint_id == Some(id)
                    && mapping::status_of(&full.task.status) != Status::AgentDone
            })
            .count();
        self.then_read(|client| {
            client.complete_sprint(id).map_err(said("the sprint could not be closed"))
        })?;
        Ok(unfinished)
    }

    /// Rename a sprint.
    /// @param id - the sprint
    /// @param name - the new name
    pub fn rename_sprint(&self, id: i64, name: &str) -> Result<(), String> {
        self.then_read(|client| {
            client
                .update_sprint(id, Some(name), None)
                .map_err(said("the sprint could not be renamed"))
        })
    }

    /// Delete a sprint. Its tickets go to the backlog.
    /// @param id - the sprint
    pub fn delete_sprint(&self, id: i64, _now: &str) -> Result<(), String> {
        self.then_read(|client| {
            client.delete_sprint(id).map_err(said("the sprint could not be deleted"))
        })
    }
}
