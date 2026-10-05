# task-2193: Unluminous issues

The ticket is eleven reports about Agent-Tasks and Agent-Chat. This records what each one was, what
changed, and how it was checked.

## 1. An agent's ticket filled the window and could not be closed

`new-task` opened the ticket it made in the modal, whoever made it. An agent driving the board through
`unluminous-cli plugins run agent-tasks new-task ...` therefore put a modal over the person's window, the
modal asked for 90% of the window, and nothing the agent could do closed it.

- `new-task` with a title creates the ticket and answers its key. Nothing opens. The answer carries
  `"opened": false`.
- `new-task` with no title is what `+ Add Task` sends, and only that opens the empty ticket in the modal,
  because an empty ticket is one somebody is about to fill in.
- The modal asks for 90% of the window and never more than 1180 by 820 points.

## 2. The ticket terminal lost the arrow keys

Outside a modal, `app::hold_the_keyboard` owns egui's keyboard focus and claims `Tab` and the arrows, so
egui never moves its focus. Inside a modal it steps aside, so an arrow key in the ticket's terminal reached
the agent and also moved egui's focus to the next widget in the dialog. When that widget was a text box,
`text_box_has_the_keyboard` answered yes, and the terminal ignored every key after it.

`terminal_panel::hold_the_keys_inside_a_modal` makes the grid hold egui's focus while it has the keys inside
a modal, and claims `Tab`, `Escape` and the four arrows through an `EventFilter`.

## 3. The tool limit was too low

`chat.tool_limit` defaults to 100 rounds, and up to 500 may be chosen. A settings file written before this
version holding 8 or 30 (an earlier default the code wrote there) is moved to 100 once.

## 4. Tool calls filled the conversation

The tool calls between two things said are one row, however many rounds they took. One call is still a
block of its own. Two or more are a row naming how many calls there were and which commands, shut until
somebody opens it. A message that is nothing but tool calls is not a row of its own.
`components::agent_chat::rows_of` builds the rows and `message::run_height` and `message::run_show` draw a
run.

## 5. An agent could not set a ticket's description

`edit-task` sets the title only, and nothing set the description, so an agent that was asked to write one
put the whole body into the title, then opened the board's SQLite file through the Database plugin and
wrote the column directly.

- `describe <key> <text>` replaces a ticket's description. Line breaks inside the text are kept.
- `task <key>` already answered with the description, as a top level `description` field. Its summary now
  says so, because the agent did not know.

The data source the agent added was on the machine it ran on. This Windows machine has no such source.
Nothing reads `board.sqlite3` after the move in section 6, so a data source pointing at it shows an old copy.

## 6. The board is an Inillucent file

`services::agent_tasks::store` keeps every query it had and runs them through `services::agent_tasks::db`,
which answers in the shape `rusqlite` did, on `inillucent-driver` 2.0.2.

Measured on a copy of this machine's board before writing any code: every query the store runs is answered
by the engine, including the correlated counts, `julianday`, `ON CONFLICT DO UPDATE`, `pragma_table_info`,
`ALTER TABLE ADD COLUMN`, `BEGIN` and `ROLLBACK`, and the check constraints. Two processes can open the same
file and each sees the other's writes. Three things differ, and each is handled in the store:

| | SQLite | Inillucent | What the store does |
|---|---|---|---|
| Foreign keys | `ON DELETE CASCADE` enforced | not enforced | `delete_task` and `clear_the_tickets` delete the todos and comments themselves |
| Database in memory | `:memory:` | none | `Store::in_memory` is a file in a folder of its own under the temporary folder, removed when the store is dropped |
| File format | SQLite | its own | a `board.sqlite3` is imported into `board.rdb` beside it the first time it is opened |

The import writes to `board.rdb-importing`, checkpoints, and renames the file and its log into place, so a
start that stops half way leaves nothing at the path the next start opens. The SQLite file is never written
to and is left where it is. Ids, keys, the check constraints and the autoincrement counters come across, and
a test checks each.

## 7. The ticket modal

The labels were drawn from the editor's font size and the fields were not, so on a machine whose editor is
set large a field's name was twice the size of the words in it. The description was a `TextEdit` put at a
rectangle with nothing clipping it, so a long one was drawn over the todos and the terminal.

- Everything in the body is a `rux` component or is drawn with a `Look` fixed at 14 points
  (`Look::at_a_fixed_size`), which is `rux`'s own body size. The editor's font no longer reaches the dialog.
- The start button, the seven dropdowns, the JIRA field, the delete button and the footer's buttons are
  `rux` `Button`, `Select` and `TextInput`. The todos, the terminal and the comments are `rux` `SubGroup`
  sections. The description and the terminal sit in `rux` wells.
- The description scrolls inside its own well (`description::in_a_well`).
- `rux` moved to `9e8f31a`, which lets a `TextInput` keep its id when the field moves, which a dialog that can
  be dragged needs.

## 8. Cmd+V did not paste into the description

On macOS `Cut`, `Copy`, `Paste` and `Select All` are key equivalents of the menu bar, so AppKit hands the chord
to the menu and the window never sees a key press. Those menu entries edited the editing area's document
whatever had the keyboard, so a paste into a ticket's description, a comment or the chat composer went into
the file behind the dialog. `UnluminousApp::give_the_clipboard_entry_to_a_text_box` sends the edit to the text
box that holds the keyboard, through the mechanism a field's own right click menu uses.

## 9. The chat pane

- The Stream switch is gone and the answer always streams. Its button was a play triangle nobody could read.
  A settings file that says `stream = false` is read and the line ignored.
- The row of tokens in and out under the composer is gone. `plugins view agent-chat` still reports usage.
- The prompt's first line sits where a one line prompt has it and every line after it goes below, so the
  field grows downwards. The line after a final line break is counted, so the well grows the moment
  `Shift+Enter` is pressed.
- Queuing a message while an answer is arriving shipped in `task-2060` and is unchanged.
