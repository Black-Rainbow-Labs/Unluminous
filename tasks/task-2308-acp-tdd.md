# task-2308: configuring an agent through the Agent Client Protocol

*"For Unluminous, let's add the ability to configure an agent through ACP. Research ACP, the open registry for
ACP https://agentclientprotocol.com/get-started/registry. Write a tdd, then fully implement. Build support for
it, then test out our claude code with the adapter, and ensure ours works as expected."*

## 1. What ACP is

The Agent Client Protocol (ACP, agentclientprotocol.com) is JSON-RPC 2.0 over an agent program's standard input
and output, one JSON object a line. The editor is the **client** and starts the agent as a child process. Zed
wrote it; JetBrains, Neovim plugins and others speak it now.

The parts that matter here, read from the protocol pages (`protocol/v1/initialization`, `session-setup`,
`prompt-turn`, `tool-calls`, `file-system`, `authentication`) and checked against a real agent:

| Direction | Method | What it is |
|---|---|---|
| client to agent | `initialize` | `protocolVersion: 1`, the client's capabilities and name. The answer has the agent's capabilities (`loadSession`, `sessionCapabilities.resume`, `promptCapabilities.image`) and `authMethods`. |
| client to agent | `session/new` | `cwd` (absolute) and `mcpServers`. Answers `sessionId`, and often `modes`, `models` and `configOptions`. |
| client to agent | `session/resume` | Carries on a session with no replay. Only when `sessionCapabilities.resume` is advertised. |
| client to agent | `session/load` | Carries on a session and replays it as `session/update` notifications before answering. Only when `loadSession` is advertised. |
| client to agent | `session/set_config_option` | Changes a config option, such as the one whose `category` is `model`. |
| client to agent | `session/prompt` | A list of content blocks (`text`, `image`, `resource_link`, `resource`). Answers `stopReason` (`end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`) once the whole turn is over, and usually `usage`. |
| client to agent | `session/cancel` | A notification that stops the turn. |
| agent to client | `session/update` | A notification: `agent_message_chunk`, `agent_thought_chunk`, `user_message_chunk`, `tool_call`, `tool_call_update`, `plan`, `usage_update`, `available_commands_update`, `current_mode_update`, `config_option_update`, `session_info_update`. |
| agent to client | `session/request_permission` | A request: a tool call and a list of options, each `allow_once`, `allow_always`, `reject_once` or `reject_always`. The client answers with one `optionId`, or `cancelled`. |
| agent to client | `fs/read_text_file`, `fs/write_text_file`, `terminal/*` | Only sent when the client advertised them. |

## 2. The registry

`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json` is `{ version, agents, extensions }`. On
2026-10-19 it held 42 agents. Each agent is:

```json
{
  "id": "claude-acp", "name": "Claude Agent", "version": "0.94.0",
  "description": "ACP wrapper for Anthropic's Claude",
  "repository": "https://github.com/agentclientprotocol/claude-agent-acp",
  "distribution": { "npx": { "package": "@agentclientprotocol/claude-agent-acp@0.94.0" } },
  "icon": "https://cdn.agentclientprotocol.com/registry/v1/latest/claude-acp.svg"
}
```

`distribution` has one or more of three kinds:

- `npx`: `{ package, args?, env? }`. Run as `npx -y <package> <args...>`. 25 agents, Claude, Codex, Gemini CLI,
  Copilot, Cline, Qwen Code and Pi among them.
- `uvx`: `{ package, args?, env? }`. Run as `uvx <package> <args...>`. 2 agents.
- `binary`: one entry a platform (`windows-x86_64`, `darwin-aarch64`, `linux-x86_64` and so on), each
  `{ archive, cmd, args?, env?, sha256? }`. The archive is a `.zip`, `.tar.gz` or `.tar.bz2` to unpack, and
  `cmd` is the program inside it, relative to where it was unpacked. 17 agents, goose and OpenCode among them.

`env` is configuration such as `AUGMENT_DISABLE_AUTO_UPDATE=1`, never a secret.

The registry only lists agents that support authentication. An agent signs in with its own account, as `claude`
and `codex` already do.

## 3. Measured against Claude's adapter

`_agent_output/task-2308-acp/record.cjs` speaks ACP from Node and writes both directions to a file. Three runs
against `npx -y @agentclientprotocol/claude-agent-acp@0.94.0`, kept in
`crates/unluminous-chat/tests/streams/acp-*.jsonl`:

1. **A read.** `initialize` answered `loadSession: true`, `sessionCapabilities.resume`, `promptCapabilities.image`
   and no `authMethods`: it uses the account `claude` is already signed in with. `session/new` answered within
   about a second with five modes (`default`, `acceptEdits`, `plan`, `auto`, `bypassPermissions`) and a `model`
   config option. A `Read` tool call arrived as `tool_call` with `rawInput: {}` and the title `Read File`, then two
   `tool_call_update`s filling in `rawInput` and the title `Read Cargo.toml (1 - 5)`, then one with
   `status: completed` and the result. No permission was asked for a read. `usage_update` reports `used` and
   `size` (1,000,000 for Opus 5.5), which is the context ring. The answer to `session/prompt` carried `usage`.
2. **A write in a second process, through `session/resume`.** The session from run 1 was carried on with no
   replay. The `Write` call sent its content as a `diff` block, then `session/request_permission` with three
   options (`allow-once`, `allow-with-updates`, `reject`). Answering `allow-once` wrote the file.
3. **An unknown session.** `session/resume` answered `-32002 Resource not found: <id>`, and a prompt sent after it
   answered `Session not found`.

Two things are not in the protocol pages and were only found by running it: the adapter sends `_auth/status_update`
notifications that are not part of ACP, and `available_commands_update` carries every skill on the machine, 35 KB
here. Both must be ignored without being parsed into anything.

## 4. Design

### 4.1 An eighth wire, `acp`

`Wire::Acp` joins the seven in `unluminous_chat::provider`. It is a program (`is_a_program`), so everything
that already branches on that question treats it as an agent that holds its own account and brings its own tools,
and it needs no change: no key, no URL, no tool catalogue, a session carried on by id, `chat.permission` applied.

An ACP agent is not one program, so `Provider` gains two fields:

- `args: Vec<String>`, written to `ai-endpoints.conf` as `provider.<n>.args`, one line, split the way a shell
  splits double quoted words. For the shipped wires it stays empty, because their arguments are built by
  `agent::command_line` and a row must not be able to change what they mean.
- `env: Vec<(String, String)>`, written as `provider.<n>.env = A=1; B=2`. Only what the registry names. It is
  added to the environment the agent is started with, after the person's own.

`settings set ai.provider.<n>.args ...` and `ai.provider.<n>.env` reach both, so the agent's half is the existing
`settings` command.

### 4.2 One process a turn, carried on with `session/resume`

The other agent wires start a process for a turn and let it end. ACP is designed for a process that lives as long
as the editor, but run 2 above shows a session carries on across processes with `session/resume`, so the turn
keeps the shape `agent::run` already has: start, initialize, open the session, prompt, read until the answer,
end. That keeps `Running`, the generation, stopping by killing the child and the channel exactly as they are, and
nothing outlives a window.

The cost is the start: about a second for the adapter to answer `session/new` after `npx` has found it in its
cache. A process kept between turns is the follow up if that second is noticed (§8).

Which call opens the session:

| The conversation has | The agent advertises | Call |
|---|---|---|
| no session | anything | `session/new` |
| a session | `sessionCapabilities.resume` | `session/resume` |
| a session | `loadSession` only | `session/load`, and every `session/update` before its answer is the replay, which is dropped |
| a session | neither | the turn fails with `Session not found`, so `refused_to_resume` asks again as a replay |

A refusal of `session/resume` or `session/load` is reported as `Session not found: <the agent's words>` for the
same reason, which is what `services::agent_chat::restore` already retries once.

### 4.3 The state machine is pure and the transport is thin

`unluminous_chat::acp` has two halves.

`acp::Turn` is the protocol: `start()` answers the first messages to send, and `message(value)` takes one message
from the agent and answers the messages to send back, the `Reply` values for the pane, and whether the turn is
over. It holds no process and no thread, so the recorded streams in `tests/streams/acp-*.jsonl` are replayed
through it in `tests/agent_streams.rs`, including what it would have sent.

`acp::run` is the transport: `agent::start` (the spawning code `agent::run` already had, split out so both use
it), then a loop reading lines, handing them to `Turn` and writing what it answers. Standard error is kept for a
refusal, as for the other agents.

What each update becomes:

| ACP | `Reply` |
|---|---|
| `session/new` or `session/resume` answered | `Session(id)`, then `Started { model }` with the name of the model config option's current value, or `models.currentModelId` |
| `agent_message_chunk` text | `Text` |
| `agent_thought_chunk` text | `Thinking` |
| `tool_call`, `tool_call_update` | `ToolCall` once, as soon as `rawInput` is not empty or the status is no longer `pending`, named by the title. `ToolAnswer` once, when the status is `completed` or `failed`: the text blocks of `content`, a `diff` block written as a short diff, or `rawOutput` when there is no content |
| `plan` | `Thinking`, the entries as a checklist, only when the plan changed |
| `usage_update` | remembered, and sent as `Context { used, window: size }` at the end of the turn |
| the answer to `session/prompt` | `Usage` from its `usage` (input with both caches, output), `Finished { reason: stopReason }`, then `Context` |
| a JSON-RPC error | `Failed`, with the message and `data.details` or `data.message` |
| anything else | nothing |

### 4.4 What the agent is allowed to do

`chat.permission` is answered on every `session/request_permission`, because a turn in the pane has nobody to
ask, which is the same reason it is a setting for the other agents:

- `full`: the first `allow_once` option, else the first `allow_always`.
- `edit`: allowed when the tool call's `kind` is `read`, `edit`, `search`, `think`, `fetch` or `move`; anything
  else (`execute`, `delete`, `other`) is refused.
- `read`: allowed only for `read`, `search`, `think` and `fetch`.

A refusal picks a `reject_once` option and says so in the thinking (`Refused: Write probe.txt, because
chat.permission is read.`), so a turn that could not do something does not look like a turn that chose not to.
An agent that offers no option of the kind wanted is answered `cancelled`. Stop kills the agent's process, as it
does every other agent's, so no permission request is left waiting.

Like Claude Code's own permission mode, this is the agent's policy carried out by the agent, not an operating
system sandbox. The page says so.

### 4.5 What Unluminous offers the agent

`clientCapabilities` is `fs.readTextFile: false`, `fs.writeTextFile: false` and `terminal: false`. An agent then
uses its own tools on the files, which is the rule every agent wire keeps: Unluminous runs none of an agent's tool
calls. Any request Unluminous did not advertise is answered with the JSON-RPC error `-32601`, so the agent is never
left waiting. `mcpServers` is empty: an agent connected with `mcp install` already has Unluminous's tools through
its own configuration.

A picture goes as an `image` block when `promptCapabilities.image` is advertised, base64 with its media type, and
as its path in the words otherwise.

A row with a model named sends `session/set_config_option` for the option whose `category` is `model` before the
prompt, matching the model by value or by name. A model the agent does not offer fails the turn with the list of
the ones it does.

### 4.6 The registry in Unluminous

`unluminous_chat::acp_registry` reads the registry (`parse`), decides how one agent runs here (`launch_for`), and
installs a binary agent (`install`). `Settings -> AI Configuration` gets a second card, `ACP Registry`:

- Nothing is fetched until `Load registry` is pressed, which is the rule that nothing is fetched that was not asked
  for. The fetch runs on a thread.
- Each agent is a row: its name, version, how it is distributed and its description, with `Add`. An agent already
  in the list says `Added`. An agent with no distribution for this platform says so instead of offering `Add`.
- `Add` makes an `acp` endpoint named by the registry id. `npx` becomes `command = npx`,
  `args = -y <package> <args>`; `uvx` becomes `command = uvx`, `args = <package> <args>`. A program that is not
  installed is still added, and its card says why it cannot answer.
- `binary` downloads the archive for this platform into `<settings folder>/acp-agents/<id>/<version>/` on the same
  thread, checks its SHA-256 when the registry gives one and deletes it when it does not match, unpacks it with
  `tar` (`%SystemRoot%\System32\tar.exe` on Windows, which reads a zip), and makes `command` the absolute path of
  `cmd` inside it. A folder that already holds that version is not downloaded again.

The agent's half is two commands in the `settings` area, held until the registry has arrived:

- `settings acp-registry [filter] [--refresh]` lists the agents, as the card does.
- `settings acp-add <id> [--name <endpoint>]` adds one, installing it first when it is a binary, and answers the
  endpoint it made.

An endpoint card for an `acp` row draws `Program`, `Arguments` and `Model`.

### 4.7 What does not change

- Agent-Tasks. A ticket's agent runs in a terminal with its own interface, and an ACP agent has none. An `acp`
  endpoint is not offered as a ticket's endpoint: `Assignee::of_wire` answers `None` for it, so
  `ai_endpoints::agents` leaves it out.
- The Set Up dialog and `unluminous-cli setup`. Adding an ACP agent is a choice made on the page.
- The shipped endpoint list. No ACP row ships; `claude` keeps `claude-cli`.

## 5. Tests

- `acp::tests` and `tests/agent_streams.rs`: the three recorded streams replayed through `Turn`, asserting the
  replies (text, the read tool's call and answer, usage, context `239750 / 1000000`, `end_turn`), what `Turn`
  sends (initialize, `session/resume` for a conversation with a session, `allow-once` for the write at `full`,
  `reject` at `read`), and the unknown session failing with words `refused_to_resume` knows.
- `session/load` drops its replay; an unadvertised request gets `-32601`; a model the row names is asked for
  through its config option and an unknown one fails with the list.
- `acp_registry`: the registry of 2026-10-19 (`tests/streams/acp-registry.json`) parses all 42; `launch_for`
  for npx, uvx and a binary on each platform; a missing platform; the SHA-256 check refusing a wrong file.
- `ai_endpoints`: `args` and `env` round trip through the file and through `settings set`.
- `provider`: the split and join of `args` keep quoted words.
- A live test, `#[ignore]`, runs a real turn through `acp::run` against the Claude adapter, then a second turn on
  the session it gave.
- The window: drive a real window, add `claude-acp` from the registry with `settings acp-add`, choose it in
  Agent-Chat, ask a question that reads a file and one that writes one, and read the conversation back with
  `plugins run agent-chat messages`.

## 5.1 What the tests and the live runs found

Measured on 2026-10-19, Windows, Claude's adapter 0.94.0 through `npx`:

- `acp_live` (`cargo test -p unluminous-chat --test acp_live -- --ignored --nocapture`): the first reply of a turn
  (the session id) arrives about **1.8 s** after the process starts. A turn that reads a file took 17.7 s, and
  the second turn, a new process carrying the session on with `session/resume`, took 4.6 s and remembered the
  word it had read.
- A release window with its own settings folder: `settings acp-registry` read the live registry (42 agents),
  `settings acp-add claude-acp` added the endpoint, and Agent Chat on it answered a read with a tool block and
  the right word, carried the session on (`restore: resume`), refused a write at `read` with the reason in the
  thinking and no file written, and made the write at `full`.
- `settings acp-add goose` downloaded the Windows zip, checked its SHA-256 and unpacked it in about 3 s. A turn
  with no goose provider configured failed with goose's own words: `Internal error: Failed to resolve provider:
  Configuration value not found: GOOSE_PROVIDER`.

**One fault in Agent Chat itself, fixed here.** `settings set plugins.agent-chat.permission read` wrote the file,
and the open pane went on answering with the permission it read when it opened, so the first live run at `read`
let the agent write. `AgentChat::follow_the_configuration_file` reads the file again when its modified time has
moved, before a turn is sent and before a command is answered, which is what `AgentTasks` already does for the
board. `a_permission_set_while_the_pane_is_open_is_the_one_the_next_turn_uses` is the test.

## 6. Files

- `crates/unluminous-chat/src/acp.rs` (new): `Turn`, `run`.
- `crates/unluminous-chat/src/acp_registry.rs` (new): the registry, the launch line, the binary install.
- `crates/unluminous-chat/src/provider.rs`: `Wire::Acp`, `args`, `env`, `split_arguments`, `join_arguments`.
- `crates/unluminous-chat/src/agent.rs`: `start` split out of `run`; `run` sends `Wire::Acp` to `acp::run`.
- `crates/unluminous-app/src/services/ai_endpoints.rs`: the two fields, the registry state.
- `crates/unluminous-app/src/components/ai_page.rs`: the `Arguments` field and the registry card.
- `crates/unluminous-app/src/app/cli*.rs`, `unluminous-cli/src/catalogue.rs`, `unluminous-cli/docs/commands.md`:
  the two commands.
- `CLAUDE.md`: a section on ACP.

## 7. Risks

- **An agent that never answers `initialize`**, such as a program that is not an ACP agent. The turn waits until
  Stop, as a `claude` that hangs does. The card's readiness line only says the program exists.
- **`npx` downloading a package on the first turn** can take a minute. The turn waits. `Add` does not prefetch.
- **The registry changes shape.** Unknown fields are ignored and an agent with no distribution Unluminous
  understands is listed without `Add`.

## 7.1 What Jason found in 1.3.0, and what changed

Reported on the ticket after the release, from a Mac:

- **Claude's adapter failed with `npm error code ETARGET`, `No matching version found for
  @agentclientprotocol/claude-agent-acp@0.94.0`.** The public registry has 0.94.0 (it is the `latest` tag), so
  that Mac's npm reads a mirror that lags. `acp::run` now runs a turn that fails that way again with the
  package's version taken off, and says so in the thinking. It only ever drops a version npx could not find.
- **The page could not be scrolled in places, and the Find field took no typing.** Neither reproduced on Windows,
  where synthetic wheel and key events reached every part of the page on both the old and the new layout. The
  cause found is macOS only: a browser or Designer page under the Settings dialog is cropped to nothing with a
  layer mask, and AppKit hit tests a view by its frame, so the invisible page still received the wheel and, once
  clicked, the keyboard. `browser::move_to` now hides a page with nothing left to show on macOS.
- **The registry is cards**, three or four a row, with `Add` on the action's blue (`settings_kit::primary`).
  An added agent shows `Update` when its endpoint runs something other than the registry's current version, and
  a trash button that removes the endpoint and deletes a binary agent's download (`AcpRegistry::update` and
  `remove`, and `settings acp-update` and `acp-remove`).
- **The registry is read when the page is shown**, the `Added claude-acp` line and its status mark are gone,
  and the card's help is one sentence.
- **Endpoints are two a row**, one a row when the dialog is narrower than two cards of 330 points, and their
  `Remove` is a trash button named `Remove <name>`.
- **A narrow window closed when the page opened.** With the dialog squeezed by a 690 point window, a field in a
  half width card was handed a negative width, and `rux` panics on a surface with a negative size. Every width on
  an endpoint card is at least zero, and `settings_kit::text_well` draws no surface into an empty rectangle.
- **Each card asked whether npx and uvx are installed, every frame**, which walks `PATH`. `AcpRegistry::runners`
  keeps the answer for five seconds.

## 8. Left for later

- On macOS, a page partly covered (a menu over it, a node in front of it) is masked but still takes the pointer
  under the cover. The full answer is a hit test on the view that agrees with the mask.

- Keeping one agent process between turns, with `session/cancel` for Stop. Measure the second per turn first.
- Asking the person about a permission in the pane, with the agent's own options as buttons. The ticket's Chat
  tab already draws a terminal's menu that way (`screen_choice`).
- Advertising `fs/read_text_file` so an agent reads a tab's unsaved text.
- Showing an ACP agent's modes and slash commands in the pane.
- The registry's icons.
