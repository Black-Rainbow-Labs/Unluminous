//! What the menu entries, the buttons in the pane and `unluminous-cli plugins run agent-tasks …` all call.
//!
//! **One path**, which is `UnluminousApp::run_action`'s rule for the menus and `run_cli`'s for the command
//! line, kept here for a plugin: a thing done by hand and the same thing done by an agent are the same
//! call rather than two that agree today.
//!
//! [`run`] used to be one 670 line match in `mod.rs`, with no test that drove it directly — every other
//! provider's dispatch is a small function per verb group, and this was the one exception. It is split
//! by verb group here, the way `database::commands::run` already is: a thin dispatcher that tries each
//! group in turn, and a `Some`/`None` from a group says whether the verb was one of its own. Nothing
//! about what a command does changed in the split; only where its code lives did.

use serde_json::json;

use crate::services::plugin_ui::{self, Answer, UiProvider};

use super::model::{Assignee, Author, Priority, Status};
use super::store::{NewTask, TaskEdit};
use super::{agent, clock, model, split_off_a_name, AgentTasks, Field, View, SWATCHES, TAIL_LINES};

/// Every command, with the one line `plugins show agent-tasks` prints for each.
pub const LIST: &[(&str, &str)] = &[
    ("board", "The lanes, their counts and their cards. Closes whatever ticket was open."),
    ("back", "Close the ticket that is open and show the lanes."),
    ("open", "Open a ticket in the modal, with its description, its terminal and all of its fields."),
    ("close", "Close the ticket modal."),
    ("reload", "Read the board again from its file."),
    (
        "clear",
        "Delete every ticket with its todos and comments, leaving the epics and the sprints. Copies the \
         board file first and says where the copy is. Takes the word `confirm`; without it, it says what \
         it would delete and deletes nothing.",
    ),
    (
        "show",
        "Read the open ticket's description or one of its comments as markdown or as its source: \
         `show description markdown`, `show comment 12 raw`.",
    ),
    ("open-pane", "Show the board's pane."),
    ("view", "Show one of board, backlog, completed or epics."),
    ("task", "One ticket, with its todos and its comments."),
    ("new-task", "Create a ticket in New, with the rest of the line as its title."),
    ("edit-task", "Change a ticket's title."),
    ("move-task", "Move a ticket to a lane and a place in it."),
    ("delete-task", "Delete a ticket, its todos and its comments."),
    ("priority", "Set a ticket's priority to low, medium or high."),
    ("assign", "Assign a ticket to claude, codex or human."),
    ("todo-add", "Add a todo to a ticket."),
    ("todo-done", "Tick a ticket's nth todo."),
    ("todo-undone", "Untick a ticket's nth todo."),
    ("todo-remove", "Delete a ticket's nth todo."),
    ("comment", "Post a comment on a ticket."),
    ("jira-key", "Record which JIRA issue a ticket is about. Nothing is fetched."),
    ("comment-edit", "Change what a comment says. A person's own comments only."),
    ("comment-send", "Post a comment and type it into the ticket's agent."),
    ("heartbeat", "Record that a ticket's agent is working, with a lease in minutes."),
    ("start", "Launch the ticket's agent and hand the ticket over."),
    ("resume", "Bring back a retired session without changing the ticket's lane."),
    ("send", "Type a line into a ticket's agent."),
    (
        "terminal",
        "What a ticket's agent has written, with whether a line is still queued for its prompt. \
         `terminal task-1 40` for the last forty lines.",
    ),
    ("interrupt", "Send Ctrl+C to a ticket's agent."),
    ("stop", "Close a ticket's terminal."),
    ("search", "Tickets whose key, title or description holds the query."),
    ("new-epic", "Create an epic. Names itself when no name is given, which is what the menu entry does."),
    ("new-sprint", "Create a sprint and make it the active one. Names itself when no name is given."),
    ("fold", "Open or shut a ticket's todos or its terminal: `fold terminal shut`. Says both when asked with nothing."),
    ("sprint-assign", "Put a ticket in a sprint, or in the backlog: `sprint-assign task-1 backlog`."),
    ("sprint-activate", "Make one sprint the active one, which is the sprint the board shows."),
    ("sprint-complete", "Close a sprint. Anything in it that is not in Agent Done goes to the backlog."),
    ("sprint-rename", "Rename a sprint."),
    ("sprint-delete", "Delete a sprint. Its tickets go to the backlog rather than with it."),
    ("epic-rename", "Rename an epic."),
    ("epic-colour", "Recolour an epic, as `#RRGGBB`. `epic-color` is the same command."),
    ("epic-delete", "Delete an epic. Its tickets keep existing with no epic."),
    ("sync", "Not implemented on this board: says so and does nothing. There is no menu entry for it."),
    ("tick", "Read every terminal and run one watchdog tick. The window does this every two minutes."),
    ("watchdog", "The same as `tick`, named for what it is for."),
];

/// Run one.
///
/// A thin dispatcher over the verb groups below: each answers `None` for a verb that is not its own, so
/// the next group is tried, and `Some(result)` once one of them recognises it. `search` is the one verb
/// with nowhere else to live, and stays here.
pub fn run(tasks: &mut AgentTasks, command: &str, arguments: &[String]) -> Result<Answer, String> {
    if let Some(answer) = board_and_pane(tasks, command, arguments) {
        return answer;
    }
    if let Some(answer) = ticket_display(tasks, command, arguments) {
        return answer;
    }
    if let Some(answer) = task_commands(tasks, command, arguments) {
        return answer;
    }
    if let Some(answer) = todo_commands(tasks, command, arguments) {
        return answer;
    }
    if let Some(answer) = comment_commands(tasks, command, arguments) {
        return answer;
    }
    if let Some(answer) = agent_commands(tasks, command, arguments) {
        return answer;
    }
    if let Some(answer) = sprint_and_epic_commands(tasks, command, arguments) {
        return answer;
    }
    if command == "search" {
        let query = arguments.join(" ");
        tasks.search(&query)?;
        return Ok(Answer::nothing().with(json!({
            "query": query,
            "found": tasks.results.iter().map(|task| task.key.clone()).collect::<Vec<String>>(),
        })));
    }
    Err(tasks.refuse(command))
}

/// The board itself, and what is not about one particular ticket: opening and closing the pane, the two
/// clocks (`reload`, `tick`/`watchdog`), and `clear`, `sync`.
fn board_and_pane(
    tasks: &mut AgentTasks,
    command: &str,
    arguments: &[String],
) -> Option<Result<Answer, String>> {
    let argument = |index: usize| plugin_ui::argument(arguments, index).unwrap_or_default();
    Some(match command {
        "board" => (|| {
            // `board` means the lanes, so it closes whatever ticket was open. Without that, creating a
            // ticket and then asking for the board would answer with the ticket that was just opened,
            // and the pane would still be showing it.
            tasks.close_detail();
            tasks.view = View::Board;
            tasks.refresh()?;
            Ok(Answer::nothing().with(UiProvider::view(tasks)))
        })(),
        "back" => {
            tasks.modal_open = false;
            tasks.close_detail();
            Ok(Answer::said("showing the lanes"))
        }
        "reload" => (|| {
            tasks.refresh()?;
            Ok(Answer::said("the board was read again"))
        })(),
        "clear" => clear(tasks, argument(0)),
        // One command that is about the window rather than about the board. The provider cannot open a
        // tab itself — only the window can — so it answers and `UnluminousApp` acts, which is the rule every
        // request in `plugin_ui::Request` follows.
        //
        // `open-pane` was the other one and it is gone with the pane: `task-28` asked for the board to be
        // a tab and nothing else, and the manifest no longer contributes a pane for it to show.
        // The window is what shows a pane; the provider's part is to accept the command, because
        // `run_plugin_command` only acts on one the provider answered. `task-1848` moved the board from
        // a tab to a pane, and this line said `open-tab` — so its own menu entry ran, was refused, and
        // the pane never appeared.
        "open-pane" => Ok(Answer::said("showing the board")),
        "view" => (|| {
            let view = View::parse(argument(0)).ok_or_else(|| {
                format!(
                    "there is no `{}` view: this board shows {}",
                    argument(0),
                    View::ALL.iter().map(|view| view.name()).collect::<Vec<&str>>().join(", ")
                )
            })?;
            // Through `set_view`, so the command and the button in the rail reach the same code: the
            // listing a view shows is read when the view is chosen, and a `view` command that set the
            // field on its own left the Backlog and the Epics views drawing nothing at all.
            tasks.set_view(view);
            Ok(Answer::said(format!("showing {}", view.label())))
        })(),
        // Named so that `plugins show` lists it and an agent is told plainly rather than being left to
        // find out by trying. There is no menu entry for it, because a control that cannot apply is
        // absent.
        "sync" => Err(
            "this board does not sync with JIRA. That is the one part of the survey the plugin does not \
             do: reading JIRA is HTTP and Unluminous has no HTTP client, so it is its own piece of work. The \
             board's own tickets are all there is here."
                .to_owned(),
        ),
        // The window's own clock, every two minutes. It is a command so that it goes down the one
        // path a change goes down, and so an agent can ask for a tick by hand rather than waiting.
        "tick" | "watchdog" => (|| {
            let now = clock::now();
            // Every terminal is read first, even the ones whose board is not showing, or an agent
            // that printed while the pane was put away would count as silent and be nudged.
            tasks.pump();
            let acted = tasks.watchdog_tick(&now)?;
            Ok(Answer::said(format!("{} card(s) acted on", acted.len())).with(json!({
                "acted": acted
                    .iter()
                    .map(|(key, decision)| json!({"task": key, "decision": format!("{decision:?}")}))
                    .collect::<Vec<serde_json::Value>>(),
            })))
        })(),
        _ => return None,
    })
}

/// `task-28`: "Clear out existing tasks. They were cloned and are out of date."
///
/// A command rather than a button. Emptying a board is not something to put one press away from
/// somebody's hand, and the ticket asks for it once. Two safeguards, because this is the one thing on
/// this board that destroys work: the file is copied first, and the word `confirm` is required.
fn clear(tasks: &mut AgentTasks, said: &str) -> Result<Answer, String> {
    let store = tasks.store()?;
    let board = store.board()?;
    let (tickets, todos, comments) = (
        board.total(),
        board
            .lanes
            .iter()
            .map(|lane| lane.tasks.iter().map(|card| card.todo_count).sum::<i64>())
            .sum::<i64>(),
        board
            .lanes
            .iter()
            .map(|lane| lane.tasks.iter().map(|card| card.comment_count).sum::<i64>())
            .sum::<i64>(),
    );
    if said != "confirm" {
        return Ok(Answer::said(format!(
            "this would delete {tickets} tickets with their todos and comments, and leave the epics \
             and the sprints. Nothing has been deleted: run `clear confirm` to do it."
        ))
        .with(json!({
            "would_delete": { "tickets": tickets, "todos": todos, "comments": comments },
            "deleted": false,
        })));
    }
    // Copied first, and named for when it was made, so two clears do not overwrite one another. A
    // board in memory has no file to copy first, which is a test's window rather than anybody's board
    // — so the copy is skipped rather than the clear being refused.
    let stamp = clock::now().replace([':', '.'], "-");
    let copy = tasks
        .configuration
        .database_path(tasks.folder.as_deref())
        .map(|file| file.with_file_name(format!("board-before-clear-{stamp}.sqlite3")));
    let copied = match &copy {
        Some(copy) => Some(tasks.store()?.copy_the_file(copy)?),
        None => None,
    };
    let (tickets, todos, comments) = tasks.store()?.clear_the_tickets()?;
    tasks.close_detail();
    tasks.refresh()?;
    tasks.message = match &copied {
        Some(copied) => format!(
            "{tickets} tickets deleted, with {todos} todos and {comments} comments. The board as it \
             was is in {}",
            copied.display()
        ),
        None => format!(
            "{tickets} tickets deleted, with {todos} todos and {comments} comments. This board is in \
             memory, so there is no copy of it as it was."
        ),
    };
    Ok(Answer::said(tasks.message.clone()).with(json!({
        "deleted": true,
        "tickets": tickets,
        "todos": todos,
        "comments": comments,
        "backup": copied.as_ref().map(|copied| copied.display().to_string()),
    })))
}

/// Opening, closing and reading one ticket, and the two sections that fold.
fn ticket_display(
    tasks: &mut AgentTasks,
    command: &str,
    arguments: &[String],
) -> Option<Result<Answer, String>> {
    let argument = |index: usize| plugin_ui::argument(arguments, index).unwrap_or_default();
    Some(match command {
        // Opening and closing the modal, so everything a person can do to it an agent can do too.
        "open" => (|| {
            let task = tasks.by_key(argument(0))?;
            tasks.open_the_modal(task.id)?;
            Ok(Answer::said(format!("{} is open", task.key)).with(tasks.detail_json()))
        })(),
        "close" => {
            tasks.modal_open = false;
            tasks.detail.is_new = false;
            Ok(Answer::said("the ticket is closed"))
        }
        // `task-28`: the same change the two buttons on a description and on each comment make, reached the
        // same way by an agent — one function behind both, which is Unluminous's own rule about a control.
        "show" => show(tasks, argument(0), argument(1), argument(2)),
        // **Reading a ticket does not open it.** It used to call `open_detail`, and that is what
        // `task-1771` reports as a side panel opening on its own: an agent working a ticket asks for
        // it by key many times over, and every one of those turned the board into a ticket somebody
        // else was looking at. Reading is `task`; showing is `open`, which is the modal. The two were
        // one command and should never have been.
        "task" => (|| {
            let task = tasks.by_key(argument(0))?;
            Ok(Answer::nothing().with(tasks.read_a_ticket(task.id)?))
        })(),
        // Both of these are on the plugin's `New` submenu, and a menu entry passes no arguments — so with a
        // required name they were two controls that could only fail. They name themselves instead, from what
        // is already there, and the name can be changed afterwards like any other.
        // The two sections of a ticket that fold. `task-1771` gave them a control - the disclosure over
        // each - and everything a person can do an agent can do too, so they have a command as well.
        // Without an argument it says which way each of them is.
        "fold" => (|| {
            let which = argument(0);
            let how = argument(1);
            let shut = match how {
                "" => None,
                "shut" | "closed" | "off" => Some(true),
                "open" | "on" => Some(false),
                other => {
                    return Err(format!("`{other}` is neither `open` nor `shut`"));
                }
            };
            match which {
                "" => {}
                "todos" => tasks.todos_shut = shut.unwrap_or(tasks.todos_shut),
                "terminal" => tasks.terminal_shut = shut.unwrap_or(tasks.terminal_shut),
                other => {
                    return Err(format!(
                        "`{other}` is not a section that folds: a ticket has `todos` and `terminal`"
                    ));
                }
            }
            Ok(Answer::said(format!(
                "todos are {}, the terminal is {}",
                match tasks.todos_shut {
                    true => "shut",
                    false => "open",
                },
                match tasks.terminal_shut {
                    true => "shut",
                    false => "open",
                }
            ))
            .with(json!({"todos": !tasks.todos_shut, "terminal": !tasks.terminal_shut})))
        })(),
        _ => return None,
    })
}

/// `task-28`: the same change the two buttons on a description and on each comment make, reached the
/// same way by an agent — one function behind both, which is Unluminous's own rule about a control.
fn show(tasks: &mut AgentTasks, what: &str, first: &str, second: &str) -> Result<Answer, String> {
    let how = |value: &str| match value {
        "markdown" | "rendered" => Ok(true),
        "raw" | "source" => Ok(false),
        other => Err(format!("`{other}` is not a way to read this: say `markdown` or `raw`")),
    };
    match what {
        "description" => {
            let rendered = how(first)?;
            tasks.show_the_description_rendered(rendered);
            Ok(Answer::said(match rendered {
                true => "the description is shown as markdown",
                false => "the description is shown as its source",
            }))
        }
        "comment" => {
            let id: i64 = first.parse().map_err(|_| format!("`{first}` is not a comment id"))?;
            if !tasks.detail.comments.iter().any(|comment| comment.id == id) {
                return Err(format!(
                    "the ticket that is open has no comment {id}: its comments are {}",
                    match tasks.detail.comments.is_empty() {
                        true => "none".to_owned(),
                        false => tasks
                            .detail
                            .comments
                            .iter()
                            .map(|comment| comment.id.to_string())
                            .collect::<Vec<String>>()
                            .join(", "),
                    }
                ));
            }
            let rendered = how(second)?;
            tasks.show_the_comment_raw(id, !rendered);
            Ok(Answer::said(match rendered {
                true => format!("comment {id} is shown as markdown"),
                false => format!("comment {id} is shown as its source"),
            }))
        }
        other => Err(format!(
            "`{other}` cannot be shown one way or the other: say `description` or `comment <id>`"
        )),
    }
}

/// Creating, editing, moving and deleting a ticket, and its priority and its assignee.
fn task_commands(
    tasks: &mut AgentTasks,
    command: &str,
    arguments: &[String],
) -> Option<Result<Answer, String>> {
    let argument = |index: usize| plugin_ui::argument(arguments, index).unwrap_or_default();
    let rest = |index: usize| plugin_ui::rest(arguments, index);
    let now = clock::now();
    Some(match command {
        "new-task" => (|| {
            let draft = NewTask {
                title: arguments.join(" "),
                assignee: tasks.configuration.agent,
                sprint_id: tasks.board.sprint.as_ref().map(|sprint| sprint.id),
                project: tasks
                    .configuration
                    .project
                    .as_ref()
                    .map(|folder| folder.display().to_string()),
                ..NewTask::default()
            };
            let task = tasks.store()?.create_task(draft, &now)?;
            tasks.refresh()?;
            // Opened as a new one, which is what puts the six fields and the description in front of
            // somebody: a ticket that cannot be given an assignee, a model and a project is a ticket that
            // cannot be started.
            tasks.open_a_new_ticket(task.id)?;
            Ok(Answer::said(format!("{} created", task.key))
                .with(json!({"task": task.key, "id": task.id})))
        })(),
        "edit-task" => (|| {
            let task = tasks.by_key(argument(0))?;
            let edit = TaskEdit {
                title: (!argument(1).is_empty()).then(|| rest(1)),
                ..TaskEdit::default()
            };
            tasks.store()?.edit_task(task.id, &edit, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("{} changed", task.key)))
        })(),
        "move-task" => (|| {
            let task = tasks.by_key(argument(0))?;
            let status = Status::parse(argument(1)).ok_or_else(|| {
                format!(
                    "there is no `{}` lane: the board has {}",
                    argument(1),
                    Status::ALL
                        .iter()
                        .map(|status| status.name())
                        .collect::<Vec<&str>>()
                        .join(", ")
                )
            })?;
            let position = argument(2).parse::<i64>().unwrap_or(i64::MAX);
            // Through `move_card` rather than straight at the store, so the command line and a drag do the
            // same thing — including the one extra thing a move does, which is hand a ticket sent back from
            // Agent Done to its agent.
            tasks.move_card(task.id, status, position)?;
            let said = match tasks.message.is_empty() {
                true => format!("{} moved to {}", task.key, status.label()),
                // What the resume said, which is the more interesting half when there is one.
                false => format!("{} moved to {}. {}", task.key, status.label(), tasks.message),
            };
            Ok(Answer::said(said))
        })(),
        "delete-task" => (|| {
            let task = tasks.by_key(argument(0))?;
            tasks.store()?.delete_task(task.id)?;
            if tasks.detail.task.as_ref().map(|open| open.id) == Some(task.id) {
                tasks.close_detail();
            }
            tasks.refresh()?;
            Ok(Answer::said(format!("{} deleted", task.key)))
        })(),
        "priority" => (|| {
            let task = tasks.by_key(argument(0))?;
            let priority = Priority::parse(argument(1)).ok_or_else(|| {
                format!("there is no `{}` priority: low, medium or high", argument(1))
            })?;
            let edit = TaskEdit { priority: Some(priority), ..TaskEdit::default() };
            tasks.store()?.edit_task(task.id, &edit, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("{} is {}", task.key, priority.name())))
        })(),
        "assign" => (|| {
            let task = tasks.by_key(argument(0))?;
            let assignee = Assignee::parse(argument(1)).ok_or_else(|| {
                format!("there is no `{}` assignee: claude, codex or human", argument(1))
            })?;
            let edit = TaskEdit { assignee: Some(assignee), ..TaskEdit::default() };
            tasks.store()?.edit_task(task.id, &edit, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("{} is {}'s", task.key, assignee.name())))
        })(),
        _ => return None,
    })
}

/// A ticket's todos: adding one, ticking it, and taking it away.
fn todo_commands(
    tasks: &mut AgentTasks,
    command: &str,
    arguments: &[String],
) -> Option<Result<Answer, String>> {
    let argument = |index: usize| plugin_ui::argument(arguments, index).unwrap_or_default();
    let rest = |index: usize| plugin_ui::rest(arguments, index);
    let now = clock::now();
    Some(match command {
        "todo-add" => (|| {
            let task = tasks.by_key(argument(0))?;
            let text = rest(1);
            if text.trim().is_empty() {
                return Err("a todo with no text would be a row nobody can act on".to_owned());
            }
            let todo = tasks.store()?.add_todo(task.id, &text, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("todo added to {}", task.key))
                .with(json!({"id": todo.id, "text": todo.text})))
        })(),
        "todo-done" | "todo-undone" => (|| {
            let task = tasks.by_key(argument(0))?;
            let which = argument(1).parse::<usize>().unwrap_or(0);
            let todos = tasks.store()?.todos(task.id)?;
            let todo = todos
                .get(which.saturating_sub(1))
                .ok_or_else(|| format!("{} has no todo {which}", task.key))?;
            tasks.store()?.set_todo_done(todo.id, command == "todo-done", &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("{}: {}", task.key, todo.text)))
        })(),
        "todo-remove" => (|| {
            let task = tasks.by_key(argument(0))?;
            let which = argument(1).parse::<usize>().unwrap_or(0);
            let todos = tasks.store()?.todos(task.id)?;
            let todo = todos
                .get(which.saturating_sub(1))
                .ok_or_else(|| format!("{} has no todo {which}", task.key))?;
            tasks.store()?.delete_todo(todo.id)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("todo removed from {}", task.key)))
        })(),
        _ => return None,
    })
}

/// Comments on a ticket, and the JIRA key it names.
fn comment_commands(
    tasks: &mut AgentTasks,
    command: &str,
    arguments: &[String],
) -> Option<Result<Answer, String>> {
    let argument = |index: usize| plugin_ui::argument(arguments, index).unwrap_or_default();
    let rest = |index: usize| plugin_ui::rest(arguments, index);
    let now = clock::now();
    Some(match command {
        "comment" => (|| {
            let task = tasks.by_key(argument(0))?;
            // **Who is commenting, when the caller is not a person.** A comment carries an author and
            // the command line cannot tell who ran it, so it said `human` for everything — including a
            // comment an agent posted about its own work, which is the one case where the author is the
            // whole point of the column. `--as claude` is how the handoff line tells it. Only in first
            // position and only when it names a real author, so `comment task-1 --as we understood it`
            // is still a comment about understanding rather than a refusal.
            let named = (argument(1) == "--as").then(|| Author::parse(argument(2))).flatten();
            let (author, body) = match named {
                Some(author) => (author, rest(3)),
                None => (Author::Human, rest(1)),
            };
            if body.trim().is_empty() {
                return Err("a comment with no body says nothing".to_owned());
            }
            tasks.store()?.add_comment(task.id, author, &body, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("commented on {} as {}", task.key, author.name())))
        })(),
        "jira-key" => (|| {
            let task = tasks.by_key(argument(0))?;
            tasks.edit_field(task.id, Field::JiraKey(rest(1)))?;
            Ok(Answer::said(format!("{} names its JIRA issue", task.key)))
        })(),
        "comment-edit" => (|| {
            let id: i64 = argument(0)
                .parse()
                .map_err(|_| format!("`{}` is not a comment id", argument(0)))?;
            let body = rest(1);
            if body.trim().is_empty() {
                return Err("a comment with no body says nothing".to_owned());
            }
            let changed = tasks.store()?.edit_comment(id, &body, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("comment {} changed", changed.id)))
        })(),
        "comment-send" => (|| {
            let task = tasks.by_key(argument(0))?;
            let body = rest(1);
            if body.trim().is_empty() {
                return Err("a comment with no body says nothing".to_owned());
            }
            // Sent first, recorded second. The other order left a comment on the board that looked sent
            // when the agent could not be reached, and a comment that looks sent is worse than one that
            // was not written: somebody waits for an answer to a question nobody was asked.
            let sent = tasks.send(argument(0), &agent::comment_handoff(&task.key, &body));
            match sent {
                Ok(answer) => {
                    tasks.store()?.add_comment(task.id, Author::Human, &body, &now)?;
                    tasks.refresh()?;
                    Ok(answer)
                }
                Err(problem) => Err(format!(
                    "{problem} The comment was not posted either, so nothing on the board says it was \
                     sent. Use `comment` to post it without sending."
                )),
            }
        })(),
        _ => return None,
    })
}

/// A ticket's agent: launching it, sending it a line, and reading its terminal.
fn agent_commands(
    tasks: &mut AgentTasks,
    command: &str,
    arguments: &[String],
) -> Option<Result<Answer, String>> {
    let argument = |index: usize| plugin_ui::argument(arguments, index).unwrap_or_default();
    let rest = |index: usize| plugin_ui::rest(arguments, index);
    let now = clock::now();
    Some(match command {
        "heartbeat" => (|| {
            let task = tasks.by_key(argument(0))?;
            let minutes = argument(1).parse::<i64>().ok();
            tasks.store()?.heartbeat(task.id, minutes, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("{} heard from", task.key)))
        })(),
        // **What a person can see in the ticket, read back as data.** The modal draws the agent's
        // terminal and nothing could read it, so the only way to find out why a started ticket was
        // doing nothing was to take a screenshot of the window and look at the picture. That is the
        // parity rule broken in the one place it was most needed: an agent watching a ticket, and a
        // person diagnosing one from a terminal, both have to be able to read what the agent wrote.
        // It also says whether the handoff is still waiting, which is the state that turns "nothing
        // is happening" into a sentence.
        "terminal" => (|| {
            let task = tasks.by_key(argument(0))?;
            let tail = argument(1).parse::<usize>().ok();
            let Some(terminal) = tasks.terminal_for(task.id) else {
                return Ok(Answer::said(format!("{} has no terminal", task.key))
                    .with(json!({"task": task.key, "terminal": false})));
            };
            let text = terminal.session.written_text(tail.or(Some(TAIL_LINES)));
            let waiting = terminal.waiting();
            Ok(Answer::said(text.clone()).with(json!({
                "task": task.key,
                "terminal": true,
                "running": terminal.session.is_running(),
                "session": terminal.session_id,
                // The two that say why nothing is happening: a line still queued, and whether the
                // prompt is thought ready for it.
                "queued": waiting,
                "prompt_ready": terminal.prompt_is_ready(),
                "text": text,
            })))
        })(),
        "start" => tasks.start(argument(0)),
        "resume" => tasks.resume(argument(0)),
        "send" => {
            let line = rest(1);
            tasks.send(argument(0), &line)
        }
        "interrupt" => (|| {
            let task = tasks.by_key(argument(0))?;
            let terminal = tasks
                .terminal_for_mut(task.id)
                .ok_or_else(|| format!("{} has no terminal", task.key))?;
            terminal.session.interrupt();
            Ok(Answer::said(format!("interrupted {}", task.key)))
        })(),
        "stop" => (|| {
            let task = tasks.by_key(argument(0))?;
            let had = tasks.terminals.iter().any(|terminal| terminal.task_id == task.id);
            tasks.terminals.retain(|terminal| terminal.task_id != task.id);
            match had {
                true => Ok(Answer::said(format!("{}'s terminal was closed", task.key))),
                false => Err(format!("{} has no terminal", task.key)),
            }
        })(),
        _ => return None,
    })
}

/// `task-1771` asks that the Backlog and the Epics views do what the page they are modelled on does —
/// rearrange by dragging, make a sprint, complete one, rename and recolour an epic — and Unluminous's rule
/// is that everything a person can do reaches the same code from the command line. These are that half.
/// A sprint or an epic is named by its **name**, because that is what is on the screen; its id is
/// accepted too, for a board with two sprints called the same thing.
fn sprint_and_epic_commands(
    tasks: &mut AgentTasks,
    command: &str,
    arguments: &[String],
) -> Option<Result<Answer, String>> {
    let argument = |index: usize| plugin_ui::argument(arguments, index).unwrap_or_default();
    let rest = |index: usize| plugin_ui::rest(arguments, index);
    let now = clock::now();
    Some(match command {
        "sprint-assign" => (|| {
            let task = tasks.by_key(argument(0))?;
            let said = rest(1);
            let said = said.trim();
            let sprint = match said.eq_ignore_ascii_case("backlog") || said.is_empty() {
                true => None,
                false => Some(tasks.sprint_named(said)?.id),
            };
            tasks.store()?.set_sprint_of(task.id, sprint, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(match sprint {
                Some(_) => format!("{} is in {}", task.key, said),
                None => format!("{} is in the backlog", task.key),
            }))
        })(),
        "sprint-activate" => (|| {
            let sprint = tasks.sprint_named(&rest(0))?;
            tasks.store()?.make_sprint_active(sprint.id)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("{} is the active sprint", sprint.name)))
        })(),
        "sprint-complete" => (|| {
            let sprint = tasks.sprint_named(&rest(0))?;
            let moved = tasks.store()?.complete_sprint(sprint.id, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(match moved {
                0 => format!("{} is completed", sprint.name),
                1 => {
                    format!("{} is completed. 1 unfinished ticket went to the backlog", sprint.name)
                }
                many => format!(
                    "{} is completed. {many} unfinished tickets went to the backlog",
                    sprint.name
                ),
            }))
        })(),
        "sprint-rename" => rename_sprint(tasks, arguments),
        "sprint-delete" => (|| {
            let sprint = tasks.sprint_named(&rest(0))?;
            tasks.store()?.delete_sprint(sprint.id, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("{} is gone. Its tickets are in the backlog", sprint.name)))
        })(),
        "epic-rename" => rename_epic(tasks, arguments),
        // The colour is the **last** word and the epic is everything before it, so an epic whose name
        // has a space in it can be recoloured without knowing its id.
        "epic-colour" | "epic-color" => recolour_epic(tasks, arguments),
        "epic-delete" => (|| {
            let epic = tasks.epic_named(&rest(0))?;
            // **`general` is the one epic that stays**, which is the browser board's own rule and what
            // the Epics view draws — it offers no Delete on that card. Enforced here rather than only
            // there, because a rule a control hides and a command allows is not a rule. Found by the
            // `task-1771` review, which pointed out that renaming it first would have exposed the
            // control anyway.
            if epic.name.eq_ignore_ascii_case("general") {
                return Err(
                    "`general` is the epic every ticket falls back to, so it cannot be deleted"
                        .to_owned(),
                );
            }
            tasks.store()?.delete_epic(epic.id, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!(
                "{} is gone. Its tickets keep existing with no epic",
                epic.name
            )))
        })(),
        "new-epic" => (|| {
            let name = match arguments.join(" ").trim() {
                "" => format!("Epic {}", tasks.store()?.epics()?.len() + 1),
                said => said.to_owned(),
            };
            // **No colour.** A plugin does not choose one, which is the rule the palette being closed exists
            // for, so an epic is created with none and the store's own default is what draws until somebody
            // sets one. `#2F6BFF` here was the plugin picking a colour.
            let epic = tasks.store()?.create_epic(&name, "")?;
            tasks.refresh()?;
            Ok(Answer::said(format!("{} created", epic.name)))
        })(),
        "new-sprint" => (|| {
            let name = match arguments.join(" ").trim() {
                "" => format!("Sprint {}", tasks.store()?.sprints()?.len() + 1),
                said => said.to_owned(),
            };
            let sprint = tasks.store()?.create_sprint(&name, model::SprintStatus::Active, &now)?;
            tasks.refresh()?;
            Ok(Answer::said(format!("{} is the active sprint", sprint.name)))
        })(),
        _ => return None,
    })
}

/// `sprint-rename <sprint> <name>`, where `<sprint>` is the longest prefix of the line that names one
/// — see `split_off_a_name`'s own doc comment for why the longest prefix is the right guess and what an
/// id in front of it settles outright.
fn rename_sprint(tasks: &mut AgentTasks, arguments: &[String]) -> Result<Answer, String> {
    let sprints = tasks.store()?.sprints()?;
    let (sprint, name) = split_off_a_name(arguments, |said| {
        sprints.iter().position(|sprint| sprint.name.eq_ignore_ascii_case(said)).or_else(|| {
            said.parse::<i64>()
                .ok()
                .and_then(|id| sprints.iter().position(|sprint| sprint.id == id))
        })
    })
    .map(|(at, name)| (sprints[at].clone(), name))
    .map_or_else(
        || {
            Err(format!(
                "there is no sprint named at the start of `{}`: this board has {}",
                arguments.join(" "),
                sprints.iter().map(|s| s.name.clone()).collect::<Vec<String>>().join(", ")
            ))
        },
        Ok,
    )?;
    if name.trim().is_empty() {
        return Err("a sprint needs a name: `sprint-rename <sprint> <name>`".to_owned());
    }
    tasks.store()?.rename_sprint(sprint.id, name.trim())?;
    tasks.refresh()?;
    Ok(Answer::said(format!("{} is now {}", sprint.name, name.trim())))
}

/// `epic-rename <epic> <name>`, the same shape as [`rename_sprint`] over the epics rather than the
/// sprints.
fn rename_epic(tasks: &mut AgentTasks, arguments: &[String]) -> Result<Answer, String> {
    let epics = tasks.store()?.epics()?;
    let (epic, name) = split_off_a_name(arguments, |said| {
        epics.iter().position(|epic| epic.name.eq_ignore_ascii_case(said)).or_else(|| {
            said.parse::<i64>().ok().and_then(|id| epics.iter().position(|epic| epic.id == id))
        })
    })
    .map(|(at, name)| (epics[at].clone(), name))
    .map_or_else(
        || {
            Err(format!(
                "there is no epic named at the start of `{}`: this board has {}",
                arguments.join(" "),
                epics.iter().map(|e| e.name.clone()).collect::<Vec<String>>().join(", ")
            ))
        },
        Ok,
    )?;
    if name.trim().is_empty() {
        return Err("an epic needs a name: `epic-rename <epic> <name>`".to_owned());
    }
    tasks.store()?.edit_epic(epic.id, Some(name.trim()), None)?;
    tasks.refresh()?;
    Ok(Answer::said(format!("{} is now {}", epic.name, name.trim())))
}

/// `epic-colour <epic> #RRGGBB`. The colour is the **last** word and the epic is everything before it,
/// so an epic whose name has a space in it can be recoloured without knowing its id.
fn recolour_epic(tasks: &mut AgentTasks, arguments: &[String]) -> Result<Answer, String> {
    if arguments.len() < 2 {
        return Err("say which epic and what colour: `epic-colour Unluminous #8B6BFF`".to_owned());
    }
    let colour = arguments.last().map(String::as_str).unwrap_or("");
    let named = arguments
        .get(..arguments.len().saturating_sub(1))
        .map(|said| said.join(" "))
        .unwrap_or_default();
    let epic = tasks.epic_named(&named)?;
    // **Any `#RRGGBB` the board's own colour reader accepts.** The seven in `SWATCHES` are what the
    // Epics view offers, because seven swatches are what the browser board offers; they are not a set
    // the column is restricted to, and a refusal that named them as though they were said something
    // the store does not enforce. The refusal names them as the ones on offer.
    if crate::services::plugins::colour(colour).is_none() {
        return Err(format!(
            "`{colour}` is not a colour: say `#RRGGBB`. The Epics view offers {}",
            SWATCHES.join(", ")
        ));
    }
    tasks.store()?.edit_epic(epic.id, None, Some(colour))?;
    tasks.refresh()?;
    Ok(Answer::said(format!("{} is {colour}", epic.name)))
}
