# task-2261: the ticket's agent as a chat, a folding fields card, icon buttons on comments, and todos that get ticked

## What was asked

> It's become tedious to interact with the agent through comments and terminal in the task modal of agent
> tasks. Let's add a tabbed view of the terminal section, which has the first/default tab be an Agent Chat
> component, and the second tab be the terminal. The agent chat should be fully wired up and show all the
> responses from the agent, while the terminal shows all the details in terminal view. Is that possible? If
> not, let's just use the Agent Chat and not use the terminal view any more. The goal is a better UX, ideally
> with a fallback to the terminal when needed.
>
> The agent chat shown in the task should be the same exact one from the agent chat side panel, use the same
> settings, etc.
>
> Also, agents aren't updating their todos as they go [...] Make sure the agent updates the todos as it goes.
>
> Also, the right side panel should be collapsable so i can maximize usage of the modal for description etc.
>
> Also, the edit and send to terminal buttons above each comment should be icon buttons (pencil and paper
> airplane). They can be small like the preview toggle for markdown above the comment.

Four pieces of work. The first is the large one.

## 1. The ticket's agent as a chat

### It is possible, and it is the same agent in both tabs

A ticket's agent is `claude` running interactively in a pseudoterminal (`services::agent_tasks::terminal`),
started with `--session-id <uuid>`, and the uuid is on the ticket's row. Claude Code writes every turn of that
conversation to its own transcript, one JSON object a line:

```
~/.claude/projects/<the working folder with every character that is not a letter or a digit as "-">/<session id>.jsonl
```

Measured on this machine against the transcript of `task-2260`'s own agent: 911 attachment lines, 182
`assistant` lines holding a `tool_use`, 182 `user` lines holding the matching `tool_result`, 66 `assistant`
lines holding text, 87 holding thinking, 9 `user` lines whose content is a plain string (the prompts), and a
`system` line with subtype `turn_duration` at the end of each turn. Each `assistant` line holds **one** content
block, and the blocks of one model message share `message.id`.

So the chat tab does not run a second agent. It **reads the transcript the agent in the terminal is already
writing**, and what is typed into the chat is typed into that terminal. The two tabs are two views of one
process and one conversation: the chat shows what the agent said and which tools it ran with their output,
folded the way the Agent-Chat pane folds them; the terminal shows everything else Claude Code draws.

Three alternatives were weighed and refused:

- **Running the ticket's agent through the chat's own transport** (`claude --print --output-format
  stream-json --resume <id>`, a process a turn). The watchdog, `interrupt`, `terminal`, the supervisor, the
  handoff queue and `Resume session` are all built on the interactive terminal, and a turn run with `--print`
  cannot be watched in a terminal at the same time. It would have meant choosing between the two tabs rather
  than having both.
- **Reading the terminal's screen and guessing which lines are the agent's words.** A character grid has no
  record of where one message ends, and Claude Code redraws it.
- **A second process tailing the transcript.** The file is appended to, so a read from the last offset each
  time the file grows is a few kilobytes a turn on the window's thread, and there is nothing to coordinate.

### The pieces

**`unluminous_chat::transcript`** is the reading, with no window and no file system: `Transcript::line`
takes one line and a `Conversation` and applies it.

| A transcript line | What it becomes |
|---|---|
| `user`, content a string, not `isMeta` | a `User` message. A slash command's `<command-name>` and `<command-args>` are written back as `/name args`. `[Request interrupted by user]` ends the turn and is not drawn. |
| `user`, content an array of `text` blocks, not `isMeta` | a `User` message of the text |
| `user`, a `tool_result` block | the answer of the call with that id, marked failed when `is_error` |
| `assistant` | appended to the message with the same `message.id`, or a new `Assistant` message: `text` to its words, `thinking` to its thinking, `tool_use` to a `ToolCall` whose arguments are the input as JSON |
| `system`, `compact_boundary` | the compacted note the pane already draws |
| `system`, `turn_duration` | the turn is over |
| `isSidechain` | skipped: a sub-agent's own work is in a file of its own |
| anything else | skipped |

It also answers whether a turn is open (a prompt arrived and no `turn_duration`, `end_turn` or interruption has
followed), the model the agent named, and how much context the last message started with. Its tests are
fixtures cut from real transcripts, fed whole and fed split at every line.

`transcript::claude_project_folder(folder)` is the folder name Claude Code uses, and
`transcript::claude_transcript(home, folder, session)` is where the file is. When it is not there the board
looks for `<session id>.jsonl` in every folder under `projects`, because Claude Code shortens a very long name
with a hash and the rule for that is not written down anywhere.

**`AgentChat` gains a mirror mode** (`AgentChat::mirroring`). The pane is the same function
(`components::agent_chat::pane`), the same message drawing, the same tool runs, the same composer, the same
Chat settings dialog and the same `rux` theme, which is what *"the same exact one"* asks for. It reads the same
Agent-Chat configuration file, so its look follows the same settings. What differs:

- The header has no history or new conversation buttons, because the conversation is the ticket's and there
  is only one. It shows the agent's name and the ticket's key.
- `Send` does not start a turn. It puts the words on `Mirror::outgoing` and shows them as a queued message until
  the transcript records them.
- `Stop` puts an interrupt on `Mirror::outgoing`.
- A dropped or pasted picture is refused with a sentence, because a picture cannot be typed into a terminal.
- The Chat settings dialog shows which agent is answering in place of the model switch, and the context ring
  as before.
- An empty conversation draws one line saying the agent has not said anything yet, in place of the welcome
  screen, which lists the side panel's own conversations.

**`services::agent_tasks::ticket_chat::TicketChat`** belongs to the board: one for the ticket in the modal,
made when the modal opens a ticket that has a session, and replaced when the ticket or its session changes. It
holds the `AgentChat`, the `Transcript`, the file's path, how far it has been read and the part line after
that. `AgentTasks::follow_the_ticket_chat`, called from `catch_up` once a frame:

1. looks at the file's length at most every 200 ms and reads what was added,
2. marks the session busy while a turn is open and the agent's terminal is running,
3. takes `Mirror::outgoing`: words go through `AgentTasks::send`, which resumes a retired session first, and
   an interrupt is `Escape` typed into the terminal. `Escape` and not `Ctrl+C`, because Claude Code takes
   `Ctrl+C` twice in a row as a request to exit, and `Escape` is its own key for interrupting a turn.

### The modal

The section that was `Agent terminal` is `Agent`, with a small pill switch on its heading row: `Chat` and
`Terminal`. Chat is first and is the default for a Claude ticket. For a Codex or OpenCode ticket the default is
Terminal and the Chat tab says, in one line, that this agent keeps its conversation where Unluminous does not
read it, because neither names its own transcript by the id on the ticket (`agent::can_resume` already records
the same fact). A ticket with no session shows the strip it always showed, in either tab.

`AgentTasks::agent_tab` is the choice (`None` until somebody picks, then the default above applies), and
`plugins run agent-tasks agent-tab [chat|terminal]` reads and sets it. `plugins run agent-tasks chat <key>
[count]` answers the last messages of the ticket's conversation as data: role, words, and each tool call with
its name and whether it failed. That is the agent's half of reading the Chat tab.

The section keeps its fold disc, its grip and `terminal-height`, which now size whichever tab is showing. The
Chat tab wants more room than the terminal did when nobody has dragged it: 45% of the column.

## 2. The fields card folds

A round chip with a chevron at the left edge of the fields card's column, on the header row beside the close
button, puts the card away and brings it back. With the card away the main column takes the whole width.
`AgentTasks::fields_shut` holds it, beside `todos_shut` and `terminal_shut`, and `fold fields [open|shut]` is
the command. The narrow layout, where the card sits above the column, folds it the same way.

## 3. Icon buttons on a comment

`Edit` is a pencil and `Send to terminal` a paper airplane, each an 18 point square button drawn the way
`raw_or_rendered` draws the view buttons beside them: no ground until the pointer is on it, then the control
colour. Their names are unchanged (`Edit the comment by ...`, `Send the comment by ... to the terminal`), so a
test and a screen reader find them as before, and each has a tooltip. `theme::icon::pencil` is new and goes on
the icon sheet in `tests/icons.rs`. `icon::send` is the paper airplane the chat's send button already uses.
`Save` and `Cancel`, shown while a comment is being edited, stay words: they are the two answers to a question
and a person has to be able to read which is which.

## 4. Agents tick their todos as they go

Two causes, and each has a change.

**Nothing told them to.** The handoff names `todo-add` and not `todo-done`. The handoff, the resumed line and
the comment line now say: make a todo for each step with `todo-add` before starting, tick each one with
`todo-done <key> <n>` the moment it is finished rather than at the end, and remove one that no longer applies
with `todo-remove`.

**Nothing noticed when they did not.** `move-task <key> agent_done` is refused while the ticket has unticked
todos **and** its agent's terminal is running, which is exactly the case of an agent handing a ticket back.
The refusal lists the open todos with their numbers and says the two commands that clear them; `move-task <key>
agent_done anyway` moves it regardless. A person dragging a card is not refused: the drag goes through
`move_card`, not the command, and a person's judgement about their own board is not second guessed.
`commands::open_todos_refusal` is the decision as a pure function, which is what its tests call.

## Tests

- `unluminous-chat`: the transcript reading, from real lines: the prompt, the slash command, the meta line
  skipped, one message from several lines with one `message.id`, a tool call answered and one failed, a
  compaction, a turn opened and closed by `turn_duration` and by an interruption, a sidechain line skipped, and
  the same file read whole and line by line giving the same conversation.
- `unluminous-app` unit: the project folder name, the refusal decision, the default tab per agent.
- Window: a ticket with a transcript written into a temporary folder opens on the Chat tab and shows the agent's
  words and a folded tool run (`agent_tasks_ticket_chat`); `agent-tab terminal` switches; `chat task-1` answers
  the messages; `fold fields shut` widens the column (`agent_tasks_ticket_fields_shut`); a comment shows the two
  icon buttons and pressing the pencil opens the edit field. The transcript's home is given by
  `UNLUMINOUS_CLAUDE_HOME` so a test never reads the person's own `~/.claude`.

## What is left out

- **Codex and OpenCode in the Chat tab.** Codex writes `~/.codex/sessions/<date>/rollout-<time>-<its own
  id>.jsonl` and the ticket does not know that id; reading it is the same work `agent::codex` already defers.
- **Pictures typed into the terminal.** Refused with a sentence.
- **Writing the transcript back.** The chat never changes the file; Claude Code owns it.

## Added during the work: three faults in the Agent-Chat pane

Reported on the ticket while it was being worked, and each reproduced before it was fixed.

**A link in a message could not be clicked.** `markdown_text::follow_a_link` counted a click as a release whose
press had started less than 0.6 s before, read through `PointerState::press_start_time`. egui clears that value
on the frame the button comes up, which is the only frame the release is seen on, so no click on a link ever
counted, in the chat, a ticket's description or its comments. It asks `primary_clicked` now.
`a_link_in_an_answer_opens_in_a_browser_tab_on_a_click` reproduced it (nothing opened) and passes now.

**The bar chart drew a stray line and a box.** Each track carried a thin mark for the earlier series, explained
only by a legend under the chart, and the earlier figure was written `41 s → 38 s`, whose arrow is a missing glyph
in an interface face without one. The earlier value is now said in words, `last week 41 s`, and the mark and the
legend are gone. A table's sort order, written `↑` and `↓`, is a drawn chevron after the column's name.

**The settings gear is in the header**, before the conversations button, and the Chat settings dialog opens under
the header. The conversation runs down to the prompt box.
