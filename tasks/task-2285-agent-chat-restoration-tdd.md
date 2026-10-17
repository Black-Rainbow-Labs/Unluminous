# task-2285: an agent picks its conversation back up after a restart

## The report

> When unluminous restarts the agent chat conversation and messages are there, but the agent isn't
> aware of them unless I explicitly ask it to fetch them. This should be automatic once I send the
> first message after restart. For agent tasks, it should happen when the agent task is resumed. The
> goal is to restore conversation, but only when needed, so we don't waste tokens. Ideally we'd resume
> session using the native resume conversation flags.

## What is actually wrong

### The chat pane

The chat pane already resumes natively within one run of the window. `unluminous_chat::agent::command_line`
passes `--resume <id>` to Claude Code, `exec resume <id>` to Codex and `--session <id>` to OpenCode
whenever `Conversation::session` is set, and `AgentChat::what_to_ask_the_agent` sends only the newest
message when it is.

`Conversation::session` is never written to disk. `services::agent_chat::store::to_json` writes the name,
the provider, the usage, the context and the messages, and `from_json` reads the same fields back. The
session id is missing from both. So after a restart the conversation on the screen has every message and
an empty `session`, and the next question is sent as a **first** question: Unluminous's preamble and the
newest message, with no `--resume`. The agent starts a new session that has never seen anything above it.
That is exactly the report.

Two smaller faults sit beside it:

- **The session belongs to one agent, and the conversation does not say which.** `send` writes
  `chat.provider = provider.name` on every turn. A conversation started with Claude Code and continued
  after switching the model chooser to Codex hands Codex Claude's session id, and Codex refuses it.
- **A refused resume is a dead end.** If the agent no longer has the session (Claude Code keeps sessions
  per project folder, so a window opened on another folder cannot resume it; a session file can be
  deleted; a conversation can be older than this fix), the turn fails with the agent's refusal and the
  conversation is stuck: every later turn sends the same id and is refused again.

### Tickets

`services::agent_tasks::agent::can_resume` is true for Claude only. A Claude ticket is started with
`--session-id <uuid>` that Unluminous chose and resumed with `--resume <uuid>`, so resuming a Claude ticket
already restores its conversation natively. Codex and OpenCode choose their own session ids and have no
flag that names a new one, so Unluminous records a marker of its own on the ticket and `Resume session` is
refused. Pressing `Start Work again` begins a new conversation. The agent that worked the ticket yesterday
is gone even though Codex and OpenCode both still hold that conversation on disk.

## What the three agents offer, measured on this machine

Versions: Claude Code 2.1.302, codex-cli 0.153.4, OpenCode 1.18.32. Each was asked to resume a session id
that does not exist, which costs nothing because all three refuse before any model is called.

| Agent | Native resume, non-interactive | Native resume, terminal | What it says for a session it does not have |
|---|---|---|---|
| Claude Code | `claude -p --resume <id>` | `claude --resume <id>` | a `result` line with `is_error: true`, `num_turns: 0` and `errors: ["No conversation found with session ID: <id>"]`, and exit 0 |
| Codex | `codex exec resume <id>` | `codex resume <id>` | `Error: thread/resume: thread/resume failed: no rollout found for thread id <id>` on standard error |
| OpenCode | `opencode run --session <id>` | `opencode --session <id>` | `Error: Session not found` on standard error |

The published documentation agrees: Claude Code's headless page documents `--resume` and the
`session_id` in its JSON output, Codex documents `codex exec resume <SESSION_ID>`, and OpenCode documents
`-s, --session` for `run` and for its terminal interface.

Where each agent keeps its sessions, which is what a ticket needs:

- **Codex** keeps a `threads` table in `~/.codex/state_<n>.sqlite` (`$CODEX_HOME` when set). Each row has
  `id`, `cwd` (written with a `\\?\` prefix on Windows), `created_at_ms`, `first_user_message` and
  `archived`.
- **OpenCode** keeps a `session` table in `~/.local/share/opencode/opencode.db` (`$XDG_DATA_HOME/opencode`
  when set). Each row has `id` (`ses_…`), `directory`, `time_created`, `parent_id` and `title`.
- `opencode session list --format json` gives the same rows through a public command, and takes 1.5 s on
  this machine. Reading the table takes milliseconds. A ticket's `Resume session` is pressed on the
  window's own thread, so the table is read and the command is not used.

## Design

### 1. The session id is written down with the conversation, with the agent it belongs to

`Conversation` gains `session_provider: String`, the provider whose agent issued `session`.
`Session::reply` sets it from `chat.provider` when `Reply::Session` arrives, which is the moment the id
is learned. The store writes `"session": { "id": …, "provider": … }` and reads it back. A file with no
`session` key, which is every file written before this change, reads as an empty session.

### 2. The newest question resumes natively when it can, and replays only when it cannot

`AgentChat::what_to_ask_the_agent` decides how the turn starts, and `services::agent_chat::restore` holds
that decision as a value so it can be tested with no agent:

```
enum Restore {
    /// The agent has this conversation: `--resume`, and only the newest message is sent.
    Resume(String),
    /// Nothing came before this question: Unluminous's preamble and the question.
    Fresh,
    /// Messages came before, and no session of this agent holds them: the preamble, the earlier
    /// conversation written out, and the question.
    Replay,
}
```

- **`Resume`** when the conversation has a session and `session_provider` is the provider being asked.
  This is the case after every restart once the session has been written down, and it costs nothing
  beyond what the agent's own resume costs.
- **`Fresh`** when there is no usable session and nothing came before the newest question.
- **`Replay`** when there is no usable session and earlier messages exist: a conversation written before
  this change, a conversation continued with a different agent, or a resume the agent refused. The
  earlier messages are written out once, in front of the newest question, inside a fenced block that says
  what it is. After that turn the agent has a session of its own again and every later turn is `Resume`.
  So the replay is paid once, and only when there is no other way.

The replay is bounded by `REPLAY_LIMIT` characters (24,000, about 6,000 tokens). When the conversation
is longer, the oldest messages are left out and the block says how many were left out. Each message is
its role and its words. A tool call is its name only, and a picture is its name, because neither can be
sent again as text and both are in the agent's own record if it has one. A compaction note is skipped.

An HTTP endpoint sends the whole transcript on every turn already (`unluminous_chat::wire::request`), so
none of this applies to one.

### 3. A refused resume is retried once with the replay

`unluminous_chat::agent::refused_to_resume(words)` is true for the three refusals in the table above.
`AgentChat::take_the_replies` watches for a `Reply::Failed` during a turn that started as `Resume`. If the
failure is a refusal and nothing has been said in the answer yet, the reply is not applied. The session is
cleared, the turn is dispatched again, and because the session is now empty it goes as `Replay`. A second
refusal in the same turn is applied as an ordinary failure, so a turn can never loop.

Claude Code puts its refusal in `errors` and leaves `result` empty, so the decoder took the refusal for
"the agent stopped with an error." and the pane showed that instead of the reason. The decoder now
reports `errors` when `result` is empty. That is needed to recognise the refusal, and it also shows the
person the real reason for any other error of that kind.

`plugins run agent-chat state` answers `restore`: `resume`, `fresh`, `replay` or `retried` for the turn
in flight or the last one. It is how an agent and a test can see which way a turn went without reading
the agent's command line.

### 4. A Codex or OpenCode ticket resumes its own conversation

`services::agent_tasks::native_session` reads the two agents' tables read only:

- `codex_thread(home, folder, key, since_ms)`: the newest thread whose `cwd` is the ticket's folder
  (compared after `unluminous_terminal::paths::plain`, case folded on Windows), whose `first_user_message`
  names the ticket key (every handoff and every resume line Unluminous types names it), not archived, and
  created at or after `since_ms` when that is known.
- `codex_knows(home, id)`: whether a thread with that id exists.
- `opencode_session(data, folder, since_ms)`: the earliest top level session (`parent_id` empty) in the
  ticket's folder created at or after `since_ms`, not archived. The earliest, because the ticket's agent
  makes its session as it starts and anything later in that folder is somebody else's.

`UNLUMINOUS_CODEX_HOME` and `UNLUMINOUS_OPENCODE_DATA` are the seams, the way `UNLUMINOUS_CLAUDE_HOME` is
for the ticket chat. A missing file, a missing table or a missing column is "not found", never an error,
because the format belongs to another program and can change.

A ticket started on Codex or OpenCode is recorded with the marker `pending-<epoch ms>-<id>`, so the time
it started survives a restart. `AgentTasks::native_session_for(task)` turns the ticket's recorded id into
one the agent answers to:

| Agent | Recorded id | Answer |
|---|---|---|
| Claude | any | the id itself, as today |
| Codex | a thread Codex knows | the id |
| Codex | `pending-…` or an older marker | `codex_thread` from that time (any time for an older marker, because the key match is enough) |
| OpenCode | `ses_…` | the id |
| OpenCode | `pending-…` | `opencode_session` from that time |

When an answer is found it is written to the ticket with `set_session`, so the lookup happens once.
`resume`, `start` (on a ticket that already has a session) and `send` all go through it. `start` with no
answer begins a new conversation as it does today. `resume` with no answer refuses with a sentence that
says what was looked for. `send` falls back to `start`, as it already does when there is no session.

`agent::launch` resumes OpenCode with `opencode --auto --session <id>`, and Codex with
`codex resume <id>` as it already does.

`agent::can_resume(agent, session)` replaces `can_resume(agent)`. It looks at the recorded id only, so the
card and the ticket modal can ask it every frame without opening a database: true for Claude with any
id, for Codex with any id that is not an older marker, and for OpenCode with a `ses_` or `pending-` id.
A ticket whose native session cannot be found behind a `pending-` marker shows `Resume session`, and the
press says the session was not found and offers `Start Work again`.

The agent is told it was resumed, as Claude is today (`agent::resumed`), so it re-reads the ticket. That
is a short line, and it is the only thing a ticket's resume sends that the agent does not already hold.

## Tests

The always-run tests use no agent and no network:

- `store`: a conversation with a session and its provider round trips, and a file with no `session` key
  opens with an empty one.
- `restore`: the three decisions; a provider change discards the session; the replay keeps the newest
  messages, says how many it left out, names tools and pictures, skips compaction notes, and leaves the
  newest question out of the block.
- `agent`: `refused_to_resume` is true for the three recorded refusals and false for ordinary failures;
  Claude's error `result` reports `errors`.
- `AgentChat`, driven with a scripted client: a reopened conversation's next turn carries `--resume` and
  only the newest message; a refused resume is retried once as a replay and a second refusal fails the
  turn.
- `native_session`: a Codex `state_5.sqlite` and an OpenCode `opencode.db` built in a temporary folder
  with the columns this code reads, covering folder matching with and without `\\?\`, the key match, the
  time bound, archived rows, the earliest OpenCode session, and a database with no such table.
- `agent`: `can_resume` for every agent and every kind of id; OpenCode's resume command line.

The live tests run the real agents and cost a few hundred tokens each, so they are `#[ignore]` and are run
by hand once, for this ticket:

```
cargo test -p unluminous-app --lib live_restore -- --ignored --nocapture --test-threads=1
```

For each of Claude Code, Codex and OpenCode, one test asks the agent to remember a word, writes the
conversation to a store, reads it back into a new pane as a restarted window would, asks for the word, and
checks the answer contains it and the turn was `Resume`. A second test for Claude Code gives the reopened
conversation a session id the agent does not have and checks the turn is retried as `Replay` and still
answers with the word. For tickets, one test per Codex and OpenCode starts the agent non-interactively in a
temporary folder with a first message naming a ticket key, then checks `native_session` finds that session
in the agent's real database. These are not run by the release, because the release runs only the default
tests.

## Left out

- **Forking** (`--fork-session`, `codex fork`, `opencode --fork`). Restoring a conversation means
  carrying it on.
- **A replay summarised by a model.** A summary costs a model call to save tokens on a turn that happens
  once per conversation at most.
- **Terminal nodes on the canvas running Codex.** `services::realm::launch` has the same limitation as
  tickets had, and the same lookup would answer it; it is a separate change to a separate surface.
