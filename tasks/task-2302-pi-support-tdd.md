# task-2302: Pi support

Pi is the coding agent at `@earendil-works/pi-coding-agent` (it was `@mariozechner/pi-coding-agent` until
0.73, which npm now marks deprecated). It is a command line program called `pi`, like `claude`, `codex` and
`opencode`, with read, bash, edit and write tools, sessions on disk, and a JSON event stream. This ticket adds
it everywhere Unluminous runs an agent: the Agent-Chat pane, the AI Configuration list, the Agent-Tasks board,
the MCP installer, `unluminous-cli setup`, the documentation and unluminous.com.

## 1. What was measured on this machine

Pi 1.3.0, installed with `npm install -g @earendil-works/pi-coding-agent`. npm puts `pi`, `pi.cmd` and
`pi.ps1` in `C:\nvm4w\nodejs`, so on Windows the program Unluminous starts is `pi.cmd`, a batch file.

**It was configured against the local Qwen** with `~/.pi/agent/models.json`:

```json
{
  "providers": {
    "local-qwen": {
      "baseUrl": "http://localhost:8080/v1",
      "api": "openai-completions",
      "apiKey": "local",
      "models": [{
        "id": "ninfer-qwen3.8-27b-coletti-v9", "name": "Qwen 3.8 27B (local)", "reasoning": true,
        "contextWindow": 262144, "maxTokens": 16000,
        "compat": { "supportsDeveloperRole": false, "thinkingFormat": "qwen-chat-template", "supportsStore": false }
      }]
    }
  }
}
```

and `~/.pi/agent/settings.json` naming `local-qwen` and that model as the defaults. Without
`supportsDeveloperRole: false` the server refused every request with `Jinja Exception: Unexpected message
role`, because Pi sends a reasoning model's system prompt as a `developer` message. A person with a Claude or
ChatGPT subscription can add it with `/login` in Pi itself; that is interactive and was not done here.

What Unluminous depends on:

| Question | Answer, measured |
|---|---|
| A turn with no terminal | `pi --mode json`, with the prompt on standard input. It reads standard input to its end and exits when the turn settles. |
| The stream | JSON lines: a `session` header carrying the session `id`, then `agent_start`, `turn_start`, `message_start`, `message_update` (with `assistantMessageEvent` of `text_delta`, `thinking_delta`, `toolcall_start`, `toolcall_delta`, `toolcall_end`), `message_end`, `tool_execution_start`, `tool_execution_update`, `tool_execution_end`, `turn_end`, `agent_end` and `agent_settled`. Words arrive as deltas. |
| A failure | The assistant `message_end` has `stopReason: "error"` and `errorMessage`, and the process still exits 0. |
| Continuing a conversation | `--session <id>`. An id it does not have: exit 1 and `No session found matching '<id>'` on standard error, before any model is called. |
| Naming a new session | `--session-id <id>` creates the session with that id when there is none and resumes it when there is. It printed `Warning: No project session found with id ...; creating a new session with that id.` and the second turn remembered a word from the first. |
| Where a session is | `~/.pi/agent/sessions/--<folder with / \ : as ->--/<time>_<id>.jsonl`, `PI_CODING_AGENT_DIR` moving `~/.pi/agent`. |
| Permissions | None. Pi never asks before a tool call. What it may do is decided by which tools it is given: `--tools read,grep,find,ls`. |
| Project trust | A folder with `.pi/settings.json`, `.pi/extensions` and the like is asked about once in the terminal interface; `--approve` answers yes for one run. JSON mode never asks and skips them. |
| MCP | `~/.pi/agent/mcp.json`, an `mcpServers` object in the same shape Claude Code reads. |

## 2. Agent-Chat: a sixth wire, `pi-cli`

`unluminous_chat::Wire::PiCli`, named `pi-cli`, a program called `pi`. It is a program and an agent, like
the other three command line wires, so it needs no key and no URL and Unluminous offers it no tools.

The command line (`agent::command_line`):

- `--mode json`, and the prompt on standard input. It has to be: `pi.cmd` is a batch file and Rust refuses
  to give a batch file an argument with a line break in it, which is the reason the other three do the same.
- `--model <model>` when the row names one, in Pi's own `provider/id` form. Empty is Pi's own default.
- `--session <id>` when the conversation has one. Not `--session-id`, because that creates an empty session
  for an id it does not know, and then a restored conversation would carry on with an agent that had seen
  none of it and nothing would say so. `--session` refuses, so `restore`'s retry as a replay works.
- The permission (`chat.permission`) is the tool list, because that is all Pi has:
  `read` is `--tools read,grep,find,ls`, `edit` adds `edit,write`, and `full` gives no `--tools`, which is
  Pi's default set with `bash`. Like Claude Code this is the agent's own policy, not the operating system's.
- A picture is an `@<path>` argument, which is how Pi attaches a file. A path has no line break, so the batch
  file takes it.

The decoder (`Decoder::pi`):

- `session` sets the session and is `Reply::Session`. `Reply::Started` takes the model from the first
  assistant `message_start` (`message.model`).
- `message_update` with `text_delta` is `Reply::Text` and `thinking_delta` is `Reply::Thinking`.
- `toolcall_end` is `Reply::ToolCall` with the call's `id`, `name` and `arguments` written as JSON.
- `tool_execution_end` is `Reply::ToolAnswer`, its `result.content` text, failed when `isError`.
- An assistant `message_end` with `stopReason` `error` is `Reply::Failed(errorMessage)`; otherwise its
  `usage` (`input + cacheRead + cacheWrite`, `output`) is added to the turn's and its input is the context
  used. `aborted` is a failure too.
- `agent_settled` ends the turn: `Reply::Usage`, `Reply::Context { used, window: None }` and
  `Reply::Finished`. Pi does not say how large its context is in the stream, so the ring draws no fraction,
  which is what every shape but Claude Code's already does.
- `compaction_end` with a result is `Reply::Compacted { before: tokensBefore }`.
- `auto_retry_start` is shown as thinking, because the turn goes on.

`refused_to_resume` learns `no session found matching`. A real run of each kind is recorded in
`crates/unluminous-chat/tests/streams/pi-cli*.jsonl` and replayed by `tests/agent_streams.rs`, as the other
agents' are.

The default endpoint list gains a row `pi` (wire `pi-cli`, command `pi`, no model). Like the others it is
dropped from the list on a machine with no `pi` on it, and comes back when Pi is installed.

## 3. Agent-Tasks: `Assignee::Pi`

- `Assignee::Pi` and `Author::Pi`, named `pi`, added to `AGENTS` and to every list a ticket's agent is chosen
  from. `Assignee::from_wire(PiCli)` is `Pi`, so a ticket given the `pi` endpoint is a Pi ticket.
- **The board's schema becomes 4.** `task.assignee` and `task_comment.author` have check constraints that
  name the values they may hold, and a check constraint cannot be changed in place. Schema 3's rebuild of
  those two tables for `opencode` is made general: a table whose check leaves out any value this version
  writes is rebuilt from `SCHEMA` with every row, every id and `sqlite_sequence` kept, after the files are
  copied into `board-before-schema-4/`. A board already at 3 is rebuilt once.
- **A Pi ticket names its own session, the way a Claude ticket does.** Started and resumed alike:
  `pi --approve --session-id <uuid>`, with Unluminous's id, because that flag makes the session when Pi has
  none and carries it on when it has. So `can_resume` is true for any id, and `native_session` is not needed.
  A ticket whose session file was lost starts a fresh conversation rather than failing, and the resumed line
  tells the agent to read the ticket again, which is what it needs either way. `--approve` is there because a
  terminal nobody is watching cannot answer Pi's project trust question, which is why Claude is given
  `--dangerously-skip-permissions`. The model is `--model`; the effort is `--thinking`, whose levels include
  all five the board offers.
- **The Chat tab reads Pi's session file.** `unluminous_chat::transcript` gains a Pi reading beside Claude
  Code's: `Transcript::for_pi()`, and `find_pi_transcript(home, folder, session)`, which looks in the folder
  Pi names after the ticket's folder for the file ending `_<id>.jsonl`. A `message` line of role `user` is a
  prompt and opens the turn; `assistant` is its text, thinking and `toolCall` blocks gathered into one
  message, and a `stopReason` other than `toolUse` closes the turn; `toolResult` answers the call with its
  `toolCallId`; `compaction` is the note. `UNLUMINOUS_PI_HOME`, then `PI_CODING_AGENT_DIR`, then
  `~/.pi/agent` is Pi's folder. A Pi ticket opens on the Chat tab, and the side pane's ticket view reads it
  the same way.
- Sending from the chat types into the terminal, as for Claude, and Stop is Escape, which is how Pi's
  interface stops a turn.
- `ready_after(Pi)` is Codex's three seconds, because Pi draws its banner first.
- **A new ticket is given the agent the board's settings name** (asked for on the ticket while it was being
  worked): `+ Add Task` and `new-task` both take `AgentTasks::default_assignee`, which is the endpoint in
  `Settings -> Agent-Tasks`, and the agent kind there when no endpoint is named. With the `pi` endpoint chosen,
  a new ticket is a Pi ticket. `a_new_ticket_is_given_the_agent_the_boards_settings_name` is the test.

## 4. Settings, setup and MCP

- `Settings -> AI Configuration` offers `pi-cli` as a wire when Pi is installed, and names Pi in its
  sentence about agents that need no key. Its search words gain `Pi`.
- `install::Client::Pi` writes the `unluminous` entry into `~/.pi/agent/mcp.json` (`PI_CODING_AGENT_DIR`
  moving it), keeping the rest of the file, and removes it. `mcp install pi`, `mcp install all`, `mcp config
  pi`, the MCP Settings page and `unluminous-cli setup` all reach it, and `setup` finds Pi by `pi` on the path.
- The Set Up Unluminous dialog from task-2301 lists the agents `setup` reports, so Pi appears there once that
  ticket is on `main`.

## 5. Documentation and the site

`unluminous-cli/docs/commands.md` and `mcp.md` name Pi wherever they list the agents, `CLAUDE.md` says what
this ticket changed, and unluminous.com says Pi is supported beside Claude Code, Codex and OpenCode, in the
agent section and the instructions an agent follows to set Unluminous up.

## 6. How it is checked

- Unit tests for the command line in each permission and on resume, the decoder over the recorded streams,
  the refusal, the transcript reader over a recorded session file, the schema 3 to 4 rebuild keeping every
  row and id, the MCP file edit, and the launch line.
- A release window: a real Agent-Chat conversation with the `pi` endpoint (a tool call, a second turn that
  remembers the first, a restart that carries on), and a real ticket assigned to Pi started on the board,
  read on its Chat tab, answered from the chat, and moved to `agent_done` by Pi through `unluminous-cli`.
