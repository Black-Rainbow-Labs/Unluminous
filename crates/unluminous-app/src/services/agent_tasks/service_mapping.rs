//! The words agent-tasks-service uses for a ticket, and the desktop board's types for them.
//!
//! `tasks/agent-tasks-through-the-service-tdd.md` has the table. Two directions: what the service sends is
//! read into [`Task`], [`Todo`], [`Comment`], [`Sprint`] and [`Epic`], and what the desktop chose is written
//! back in the service's words. A word the service sends that the desktop has no name for is read as the
//! closest one the desktop has, because a ticket the pane refused to draw would be a ticket nobody could see.

use super::model::{
    Assignee, Author, Board, Comment, Epic, Lane, Priority, Source, Sprint, SprintStatus, Status,
    Task, Todo,
};

/// The service's status as a lane. `done` is a finished ticket, so it is drawn in Agent Done.
/// @param word - the service's `status`
pub fn status_of(word: &str) -> Status {
    match word {
        "done" => Status::AgentDone,
        other => Status::parse(other).unwrap_or(Status::New),
    }
}

/// The service's assignee as the desktop's. `local_ai` and `umbra` run on opencode. Any other word is a
/// person, such as `jason`.
/// @param word - the service's `assignee`
pub fn assignee_of(word: &str) -> Assignee {
    match word {
        "claude" => Assignee::Claude,
        "codex" => Assignee::Codex,
        "local_ai" | "umbra" | "opencode" => Assignee::OpenCode,
        "pi" => Assignee::Pi,
        _ => Assignee::Human,
    }
}

/// The desktop's assignee as the word the service stores.
/// @param assignee - the desktop's assignee
pub fn assignee_word(assignee: Assignee) -> &'static str {
    match assignee {
        Assignee::Claude => "claude",
        Assignee::Codex => "codex",
        Assignee::OpenCode => "local_ai",
        Assignee::Pi => "pi",
        Assignee::Human => "human",
    }
}

/// The service's comment author as the desktop's. A person's name, such as `jason`, is a person.
/// @param word - the service's `author`
pub fn author_of(word: &str) -> Author {
    match word {
        "local_ai" | "umbra" => Author::OpenCode,
        other => Author::parse(other).unwrap_or(Author::Human),
    }
}

/// The desktop's comment author as the word the service stores.
/// @param author - the desktop's author
pub fn author_word(author: Author) -> &'static str {
    match author {
        Author::OpenCode => "local_ai",
        other => other.name(),
    }
}

/// One ticket from the service as the desktop's [`Task`].
/// @param full - the ticket with every field the service sends
pub fn task_of(full: &tasks_client::FullTask) -> Task {
    let task = &full.task;
    Task {
        id: task.id,
        key: task.task_key.clone(),
        title: task.title.clone(),
        description: task.description.clone(),
        priority: Priority::parse(&task.priority).unwrap_or_default(),
        status: status_of(&task.status),
        assignee: assignee_of(&task.assignee),
        endpoint: None,
        model: task.model.clone(),
        effort: full.effort.clone(),
        epic_id: task.epic_id,
        sprint_id: task.sprint_id,
        position: task.position,
        project: full.agent_project_id.clone(),
        session_id: task.agent_session_id.clone(),
        heartbeat_at: full.heartbeat_at.clone(),
        lease_minutes: full.lease_duration_minutes,
        watchdog_strikes: full.watchdog_strikes,
        watchdog_nudges: 0,
        watchdog_nudged_at: None,
        source: Source::Local,
        jira_key: None,
        jira_url: None,
        jira_status: None,
        jira_issue_type: None,
        created_at: task.created_at.clone(),
        updated_at: full.updated_at.clone(),
        // A ticket from the summary has no todos or comments, only their counts (task-2879). A ticket read in full
        // has no counts, and its children are the count.
        todo_count: full.todo_count.unwrap_or(task.todos.len() as i64),
        todo_done_count: full
            .todo_done_count
            .unwrap_or(task.todos.iter().filter(|todo| todo.done).count() as i64),
        comment_count: full.comment_count.unwrap_or(task.comments.len() as i64),
    }
}

/// One todo from the service.
/// @param todo - the service's todo
pub fn todo_of(todo: &tasks_client::TaskTodo) -> Todo {
    Todo {
        id: todo.id,
        task_id: todo.task_id,
        text: todo.text.clone(),
        done: todo.done,
        position: todo.position,
        created_at: todo.created_at.clone(),
    }
}

/// One comment from the service.
/// @param comment - the service's comment
pub fn comment_of(comment: &tasks_client::TaskComment) -> Comment {
    Comment {
        id: comment.id,
        task_id: comment.task_id,
        author: author_of(&comment.author),
        body: comment.body.clone(),
        created_at: comment.created_at.clone(),
    }
}

/// One sprint from the service.
/// @param sprint - the service's sprint
pub fn sprint_of(sprint: &tasks_client::Sprint) -> Sprint {
    Sprint {
        id: sprint.id,
        name: sprint.name.clone(),
        status: SprintStatus::parse(&sprint.status).unwrap_or_default(),
        position: sprint.position,
        created_at: String::new(),
    }
}

/// One epic from the service.
/// @param epic - the service's epic
pub fn epic_of(epic: &tasks_client::TaskEpic) -> Epic {
    Epic {
        id: epic.id,
        name: epic.name.clone(),
        color: epic.color.clone(),
        position: epic.position,
    }
}

/// The tickets in one sprint, or with no sprint when `sprint` is `None`, in the order the board draws them:
/// by lane, then position, then id. The order `Store::board` reads them in.
/// @param board - the whole board
/// @param sprint - the sprint, or `None` for the tickets with no sprint
pub fn tasks_in(board: &tasks_client::FullBoard, sprint: Option<i64>) -> Vec<Task> {
    let mut tasks: Vec<Task> =
        board.tasks.iter().filter(|full| full.task.sprint_id == sprint).map(task_of).collect();
    tasks.sort_by_key(|task| (task.status, task.position, task.id));
    tasks
}

/// The board the pane draws: the active sprint's tickets in four lanes, and the epics.
/// @param board - the whole board from the service
pub fn board_of(board: &tasks_client::FullBoard) -> Board {
    let sprint = board.active_sprint.as_ref().map(sprint_of);
    let tasks = tasks_in(board, sprint.as_ref().map(|sprint| sprint.id));
    let lanes = Status::ALL
        .into_iter()
        .map(|status| Lane {
            status,
            tasks: tasks.iter().filter(|task| task.status == status).cloned().collect(),
        })
        .collect();
    Board { sprint, lanes, epics: board.epics.iter().map(epic_of).collect() }
}

/// The ids of one lane after a ticket is moved into it at `position`, which is what `POST /tasks/reorder` is
/// sent. Copies `Store::move_task`: a ticket arriving in Agent Done from another lane goes to the top.
/// @param board - the whole board
/// @param id - the ticket being moved
/// @param status - the lane it is moved to
/// @param position - where in the lane, counted from 0
pub fn lane_after_move(
    board: &tasks_client::FullBoard,
    id: i64,
    status: Status,
    position: i64,
) -> Result<(Option<i64>, Vec<i64>), String> {
    let moved = board
        .tasks
        .iter()
        .find(|full| full.task.id == id)
        .ok_or_else(|| format!("ticket {id} is not on the board"))?;
    let sprint = moved.task.sprint_id;
    let mut lane: Vec<&tasks_client::FullTask> = board
        .tasks
        .iter()
        .filter(|full| {
            full.task.id != id
                && full.task.sprint_id == sprint
                && status_of(&full.task.status) == status
        })
        .collect();
    lane.sort_by_key(|full| (full.task.position, full.task.id));
    let mut order: Vec<i64> = lane.iter().map(|full| full.task.id).collect();
    let arriving_done =
        status == Status::AgentDone && status_of(&moved.task.status) != Status::AgentDone;
    let at = match arriving_done {
        true => 0,
        false => (position.max(0) as usize).min(order.len()),
    };
    order.insert(at, id);
    Ok((sprint, order))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_status_the_service_sends_has_a_lane() {
        assert_eq!(status_of("new"), Status::New);
        assert_eq!(status_of("qa_failed"), Status::QaFailed);
        assert_eq!(status_of("in_progress"), Status::InProgress);
        assert_eq!(status_of("agent_done"), Status::AgentDone);
        assert_eq!(
            status_of("done"),
            Status::AgentDone,
            "a finished ticket is drawn in Agent Done"
        );
    }

    #[test]
    fn every_assignee_the_service_sends_is_read_and_written_back() {
        assert_eq!(assignee_of("claude"), Assignee::Claude);
        assert_eq!(assignee_of("codex"), Assignee::Codex);
        assert_eq!(assignee_of("local_ai"), Assignee::OpenCode);
        assert_eq!(assignee_of("umbra"), Assignee::OpenCode);
        assert_eq!(assignee_of("pi"), Assignee::Pi);
        assert_eq!(assignee_of("jason"), Assignee::Human, "a person's name is a person");
        for assignee in Assignee::ALL {
            assert_eq!(
                assignee_of(assignee_word(assignee)),
                assignee,
                "{} survives the round trip",
                assignee.name()
            );
        }
        assert_eq!(author_of("jason"), Author::Human);
        assert_eq!(author_of("local_ai"), Author::OpenCode);
        for author in Author::ALL {
            assert_eq!(
                author_of(author_word(author)),
                author,
                "{} survives the round trip",
                author.name()
            );
        }
    }

    /// A board with three tickets in New and one in Agent Done, all in sprint 1.
    fn board() -> tasks_client::FullBoard {
        let json = serde_json::json!({
            "activeSprint": {"id": 1, "name": "Sprint 1", "status": "active", "position": 0},
            "sprints": [{"id": 1, "name": "Sprint 1", "status": "active", "position": 0}],
            "epics": [{"id": 3, "name": "infra", "color": "#2FB67C", "position": 0}],
            "tasks": [
                {"id": 1, "taskKey": "task-1", "title": "a", "status": "new", "position": 0, "sprintId": 1, "assignee": "claude", "agentProjectId": "unluminous-mac", "effort": "high", "todos": [{"id": 9, "taskId": 1, "text": "t", "done": true, "position": 0}], "comments": [{"id": 4, "taskId": 1, "author": "jason", "body": "hi"}]},
                {"id": 2, "taskKey": "task-2", "title": "b", "status": "new", "position": 1, "sprintId": 1, "assignee": "local_ai"},
                {"id": 3, "taskKey": "task-3", "title": "c", "status": "new", "position": 2, "sprintId": 1, "assignee": "claude"},
                {"id": 4, "taskKey": "task-4", "title": "d", "status": "done", "position": 0, "sprintId": 1, "assignee": "claude"},
                {"id": 5, "taskKey": "task-5", "title": "e", "status": "new", "position": 0, "assignee": "claude"}
            ]
        });
        serde_json::from_value(json).expect("the board parses")
    }

    #[test]
    fn a_service_ticket_reads_as_a_desktop_ticket() {
        let board = board();
        let task = task_of(&board.tasks[0]);
        assert_eq!(task.key, "task-1");
        assert_eq!(
            task.project.as_deref(),
            Some("unluminous-mac"),
            "the project is the service's project id"
        );
        assert_eq!(task.effort.as_deref(), Some("high"));
        assert_eq!((task.todo_count, task.todo_done_count, task.comment_count), (1, 1, 1));
        assert_eq!(comment_of(&board.tasks[0].task.comments[0]).author, Author::Human);
    }

    #[test]
    fn the_board_draws_the_active_sprint_in_four_lanes() {
        let drawn = board_of(&board());
        assert_eq!(drawn.sprint.as_ref().map(|sprint| sprint.name.as_str()), Some("Sprint 1"));
        let keys = |status: Status| {
            drawn
                .lane(status)
                .unwrap()
                .tasks
                .iter()
                .map(|task| task.key.clone())
                .collect::<Vec<String>>()
        };
        assert_eq!(
            keys(Status::New),
            ["task-1", "task-2", "task-3"],
            "the backlog ticket is not on the board"
        );
        assert_eq!(keys(Status::AgentDone), ["task-4"]);
        assert_eq!(drawn.epics.len(), 1);
    }

    #[test]
    fn a_move_inside_a_lane_lands_at_the_index_asked_for() {
        assert_eq!(lane_after_move(&board(), 3, Status::New, 0).unwrap(), (Some(1), vec![3, 1, 2]));
        assert_eq!(
            lane_after_move(&board(), 1, Status::New, 9).unwrap(),
            (Some(1), vec![2, 3, 1]),
            "past the end is the end"
        );
    }

    #[test]
    fn a_ticket_arriving_in_agent_done_goes_to_the_top() {
        assert_eq!(
            lane_after_move(&board(), 2, Status::AgentDone, 5).unwrap(),
            (Some(1), vec![2, 4])
        );
    }
}
