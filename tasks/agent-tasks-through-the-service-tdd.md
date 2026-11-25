# Agent-Tasks can read and write the board through agent-tasks-service

## The problem

The phone and the iPad show the board kept by agent-tasks-service. Desktop Unluminous shows a different
board. On the MacBook Pro on 2026-11-23:

| Who | Reads | Newest ticket |
|---|---|---|
| iPhone and iPad | `tasks-api` on ports 8096 and 8196, which owns `agent-tasks-service/data/agent-tasks.rdb` | task-33 |
| Desktop Agent-Tasks pane | `~/Library/Application Support/Unluminous/plugins/agent-tasks/board.rdb` | task-11 |

Agent-Tasks has no HTTP client. It opens its own Inillucent file through `services::agent_tasks::store::Store`.
The `database` setting in the plugin's `settings.conf` can name another file, but it cannot be pointed at the
service's file for two reasons:

- The schemas differ. The service has `task_epic`, `pty_session`, `board_user`, `board_device` and more, and
  its `task` table has columns the desktop does not know, such as `agent_project_id` and `worktree_path`.
  `Store::open` runs its own migration on whatever file it opens, so it would change the service's tables.
- The service keeps one thread that owns the file (`agent-tasks-service/docs/architecture-overview.md`,
  "The one thread that owns the file"). A second process with its own buffer pool on the same file is the
  case the engine is built to refuse.

So the desktop has to be a client of the service, the way the phone is.

## What changes

1. **A setting chooses where the board comes from.** `board-source` in the plugin's `settings.conf` is
   `file` (the default, and what every board does today) or `service`. `Settings -> Agent-Tasks` has a card
   for it, and `plugins run agent-tasks board-source [file|service]` is the command line half. A change
   reopens the board at once.
2. **In `service` mode the board is read and written through agent-tasks-service**, found the way the Pair a
   Phone dialog finds it: `services::pairing::find_board`, which reads `boards/<port>.conf` in the settings
   folder (or `UNLUMINOUS_BOARD_CONF`) and checks `/health`. The request carries the machine secret from that
   file as `x-skip-token`, on loopback.
3. **The desktop and the phone use the same client crate.** `tasks-client` in the `unluminous-mobile`
   repository is the board over HTTP: models, commands and the live event stream, with no user interface.
   Its own header says it was written so the Agent-Tasks plugin could share it. The desktop depends on it by
   git, pinned to a commit, so a change to the service's API is made once in `tasks-client` and both apps
   take it. Anything the desktop needs that `tasks-client` lacks is added to `tasks-client`, never written a
   second time in Unluminous.
4. **The agents run in the service's terminal daemon in `service` mode.** Start, Resume, Send, Pause
   (interrupt) and the watchdog are the service's. The ticket's terminal in the desktop modal shows the
   service's terminal through `GET /tasks/:id/agent-terminal/ws`, the same relay the phone uses.

## The design

### The seam: `BoardStore`

Every Agent-Tasks command reaches the board through `AgentTasks::store()`, and the board has about 45
operations, each a named function on `Store`. A new enum takes `Store`'s place in `AgentTasks`:

```rust
pub enum BoardStore {
    File(Store),
    Service(ServiceBoard),
}
```

`BoardStore` has every method `Store` has, with the same signature, and sends each call to whichever board is
open. So `mod.rs`, `commands.rs`, `supervisor.rs` and the components keep calling `self.store()?.board()`
and do not know which board answered. `Store` itself does not change.

### `ServiceBoard`

`services/agent_tasks/service_board.rs`. It holds a `tasks_client::TasksClient` and a copy of the last
`GET /tasks/board`, which has every ticket with its todos and comments nested. That request is 334 KB and
answers in about 5 ms on the MacBook Pro with 33 tickets.

- **Reads come from the copy**, so the pane never waits on the network while it draws. `board`, `backlog`,
  `completed`, `task`, `task_by_key`, `todos`, `comments`, `epics`, `sprints`, `active_sprint`,
  `epic_counts`, `sprint_counts`, `tasks_of_sprint` and `owner_of` are answered from it.
- **Writes are a request and then a fresh copy.** Each write calls the client, then reads the board again, so
  the next draw shows what the service now holds.
- **`search` is `GET /tasks/search`**, because the service searches its own text.
- **The copy is read again when the service says something changed.** `tasks_client::EventListener` reads
  `GET /tasks/events` on its own thread and sets a flag. `AgentTasks::notice_another_window`, which already
  runs once a second to see changes another window made to `board.rdb`, reads the board again when that flag
  is set, and every 30 seconds when the stream is down.

How the service's words map to the desktop's:

| Service | Desktop |
|---|---|
| status `new`, `qa_failed`, `in_progress`, `agent_done` | the same lanes |
| status `done` | `AgentDone`. The desktop has four lanes, and `done` is a ticket that is finished |
| assignee `claude`, `codex` | `Claude`, `Codex` |
| assignee `local_ai`, `umbra` | `OpenCode`. The service runs `local_ai` tickets with opencode |
| assignee `pi` | `Pi` |
| any other assignee, such as `jason` or `human` | `Human` |
| `agentProjectId` | `project`. In `service` mode it is a project id from `GET /tasks/agent-projects`, not a folder |
| `agentSessionId`, `heartbeatAt`, `leaseDurationMinutes`, `watchdogStrikes` | the same fields |
| `taskKey` | `key` |
| sprint and epic | the same fields |

Writing back, `OpenCode` is `local_ai` and `Human` is `human`.

Operations that only the desktop's own watchdog and launcher use (`claim`, `release`, `set_session`,
`heartbeat`, `strike`, `nudge`, `reclaim`, `watchdog_candidates`, `lease_expired`, `schedules`) answer
"the service owns this" or an empty list, because in `service` mode the service runs the agents and the
watchdog. `clear_the_tickets` and `copy_the_file` are refused: they act on a file the desktop does not have.

### Agents in `service` mode

| The desktop's action | In `service` mode |
|---|---|
| `start`, `resume` | `POST /tasks/:id/agent-terminal/start`. The service reattaches a live session rather than starting a second agent |
| `send`, `comment-send` | `POST /tasks/:id/agent-terminal/message`, which starts a stopped agent first |
| `interrupt` (the pause button) | `POST /tasks/:id/agent-terminal/interrupt` |
| `escape` | `message` with the Escape character |
| `stop` | closes the desktop's view of the terminal, and leaves the agent running in the service |
| the watchdog tick | does nothing. The service's watchdog runs |
| `terminal`, `choice` | read from the desktop's view of the terminal, as today |

### The terminal

The ticket modal draws a `unluminous_terminal::Session`, a pseudoterminal running a program. In `service`
mode that program is Unluminous itself, started as

```
unluminous --board-terminal <task id>
```

with the board's address and secret in the environment (`UNLUMINOUS_BOARD_URL`, `UNLUMINOUS_BOARD_TOKEN`).
`main` reads `--board-terminal` before anything else and runs `services::agent_tasks::board_terminal::run`,
which opens a window for nothing. It:

1. puts its terminal in raw mode,
2. opens `ws://127.0.0.1:<port>/tasks/<id>/agent-terminal/ws` with `x-skip-token`,
3. writes `replay` and `data` frames to its output, and sends what is typed as `input` frames,
4. sends a `resize` frame when the pseudoterminal changes size,
5. exits when the socket closes or an `exit` frame arrives, saying why.

So the desktop's terminal emulator, scrollback, choice detection and Chat tab work on the service's terminal
with no change, and closing the desktop's view never stops the agent.

The frames are the daemon's protocol, documented in `agent-tasks-service/docs/pty-daemon.md`. Their encoding
lives in `tasks_client::terminal_frames`, so the phone's terminal and the desktop's read and write the same
frames from one module.

### `tasks-client` changes

All additive, so the phone app builds unchanged:

- `Task` gains `effort`, `agent_project_id`, `heartbeat_at`, `lease_duration_minutes`, `watchdog_strikes`,
  `updated_at` and `status_changed_at`, each `#[serde(default)]`.
- `CreateTask` and `UpdateTask` gain `effort` and `agent_project_id`.
- New calls: `task_by_key`, `search`, `edit_comment`, `sprints`, `create_sprint`, `update_sprint`,
  `complete_sprint`, `delete_sprint`, `epics`, `create_epic`, `update_epic`, `delete_epic`,
  `interrupt_terminal`, `agent_projects`.
- `terminal_frames`: `Frame` (`replay`, `data`, `exit`, `error`), `parse_frame`, `input_frame`,
  `resize_frame`.
- TLS becomes the default feature `rustls`. The phone keeps it. The desktop turns default features off,
  because it only talks to loopback over plain HTTP and Unluminous uses native TLS to keep its binary small
  (the comment on `ureq` in Unluminous's `Cargo.toml`).

### The setting

`Configuration` gains `board_source: BoardSource` (`File` or `Service`), read from and written to
`board-source` in the plugin's `settings.conf`. A file with no `board-source` line is `File`, so every board
today opens exactly as it did.

When `service` is chosen and no service answers, the board does not open and the pane says why, in the words
`find_board` already uses ("The Tasks board service is not running on this computer. Start it with ..."). It
does not fall back to the file, because a person who chose the service and saw the old board would not
know which board they were looking at.

## Tests

- `tasks-client`: the new calls against a real `tasks-api`, in the crate's existing harness.
- `service_board.rs`: the field mapping in both directions, the five statuses and every assignee.
- An integration test in Unluminous that starts a `tasks-api` from `agent-tasks-service` on a free port with
  a temporary database, writes a conf file for it, opens Agent-Tasks in `service` mode, and checks that a
  ticket made with `new-task` is on the service's board and that a ticket made on the service shows on the
  desktop board after an event. It is `#[ignore]` when no `tasks-api` binary is found.
- By hand on the MacBook Pro: `board-source service` shows task-33 and the rest of the service's board, a
  comment posted on the iPad shows on the desktop, and task-33's terminal shows in the ticket modal.

## What is not in this change

- The phone's terminal does not yet use the paired route, so it fails on a paired iPad. That fix is in
  `unluminous-mobile` (`crates/terminal/src/link.rs`) and is filed as its own ticket. It can use
  `terminal_frames` once it lands.
- The supervisor still runs in the desktop. In `service` mode it drives the service's board through the same
  `plugins run agent-tasks` commands.
- There is no import of a desktop board into the service.
