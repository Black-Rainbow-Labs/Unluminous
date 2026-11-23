# task-26: the phone's Design tab without desktop Unluminous, with a chat and images

## What this changes

task-19 put the Unluminous Designer's designs on the phone and on a web page (`GET /designs`). The list, the
page, the notes and the history already read the files the Designer writes, so they work with no Unluminous
window open. Asking the agent to change a design does not: `POST /tasks/designs/request` goes to the running
window's Designer and answers 503 when no window has the project open, and the Agent panel only shows the
last eight messages the window or a conversation file holds.

This ticket makes the board run the agent itself, adds a chat per design that looks and behaves like desktop
Agent-Chat, lets a message carry images, and gives the web page the same chat.

Three repositories change:

| Repository | What changes |
|---|---|
| agent-tasks-service | `tasks_core::services::designs::chat` (the conversation store), `designs::runner` (starting, reading and stopping the agent), four routes, the web page |
| unluminous-mobile | the tasks-client models and calls, a Chat panel in the Design tab, picking an element in the page's web view |
| unluminous | this document only. The desktop Designer and Agent-Chat do not change; section 6 says why |

## 1. How the agent runs without the desktop

The board starts Claude Code itself, as a child process of `tasks-api`, with the same command desktop
Agent-Chat uses (`crates/unluminous-chat/src/agent.rs:24`):

```
claude --print --input-format stream-json --output-format stream-json --verbose --include-partial-messages
       --dangerously-skip-permissions [--model <AGENT_TASKS_DESIGN_MODEL>] [--resume <session id>]
       [--add-dir <project folder>]
```

**Why not a pty-daemon terminal.** The ticket offered a pty-daemon session as one way. pty-daemon resolves its
working folder from the board's project allow list and a ticket's worktree, and a design's project comes from
Unluminous's recent projects instead, so the allow list would have to grow a second kind of entry. A terminal
also gives a screen of escape sequences, and the chat needs each text block and each tool call as data.
`--output-format stream-json` gives exactly that, one JSON line per event, which is what desktop Agent-Chat
reads too. So the board runs the CLI directly and reads its output file.

**One turn is one process.** Each message the person sends starts one `claude --print` process:

| File, under `<board data>/design-chats/<project id>/<design key>/` | What it holds |
|---|---|
| `chat.json` | The conversation: every finished message, and the Claude session id |
| `turn.json` | The turn that is running now: its number, the process id and when it started. Absent when nothing runs |
| `turns/<n>.in.jsonl` | The one user message sent on stdin: the prompt text and the images as base64 blocks |
| `turns/<n>.out.jsonl` | Everything the CLI wrote on stdout |
| `turns/<n>.err.txt` | What it wrote on stderr |
| `worktree/` | The agent's working folder when the project is a git repository |

`<board data>` is the folder that holds the board's database. `<design key>` is 12 hex characters of the
SHA-256 of the design's path inside the project, so a design's name never becomes part of a path.

**Starting a turn.**

1. Refuse with 409 when `turn.json` names a process that is still running, or when a window has the project
   open and its Agent-Chat is answering (section 4).
2. Keep a history copy of the design, exactly as the Designer does before a request
   (`designer/history.rs`): `<project>/.unluminous/designs/<design without extension>/<YYYYMMDD-HHMMSS>-<slug>.html`,
   where the slug is the first five words of the request. The History panel then lists it.
3. Write the prompt (section 2) and the images (section 3) as one line to `turns/<n>.in.jsonl`.
4. Start the CLI with stdin from that file and stdout and stderr to the two output files, in a process group
   of its own (`process_group(0)`), with the Claude Code session variables removed from its environment the
   way `scripts/mac-board.sh` removes them, and `--resume` with the session id when the conversation has one.
5. Write `turn.json`, and add the person's message to `chat.json`.
6. A thread waits for the process. When it exits, the turn's output is folded into one assistant message in
   `chat.json`, the session id from the `system`/`init` line is kept, and `turn.json` is removed.

Because the process is in its own process group and its output goes to files, it keeps running when
`tasks-api` is restarted. The next state read after a restart finds `turn.json`, sees whether the process
is still alive (`kill -0`), and finishes the turn from the output file when it is not.

**Where it runs.** When the project folder is a git repository, the agent's working folder is a detached
worktree of its own, `design-chats/<project id>/<design key>/worktree`, made with
`git -C <project> worktree add --detach <path> HEAD` the first time. The project folder is passed with
`--add-dir`, and the prompt names the design file by its absolute path in the project folder, because that
file is the one the phone, the web page and the Designer show. So the design file changes where everyone
sees it, and any other file the agent writes or any git command it runs stays in the worktree, away from the
checkout a person or another agent is using. The worktree sits outside the repository so it is never an
untracked folder in the person's checkout. A project that is not a git repository runs in the project
folder.

**Stopping.** `POST /tasks/designs/chat/stop` sends `SIGTERM` to the turn's process group, then `SIGKILL`
after two seconds if anything is left. The turn is folded as usual, with `Stopped` as its failure.

**The model.** `AGENT_TASKS_DESIGN_MODEL` names it (for example `opus`). Unset, the CLI's own default is
used. `CLAUDE_CLI_PATH` names the program, as it does for pty-daemon; unset, `~/.local/bin/claude` when it
exists, else `claude` on `PATH`. `AGENT_TASKS_DESIGN_AGENT` replaces the whole program for the tests.

## 2. The prompt

The board writes the same text the Designer's `prompt` and `ask` verbs send
(`designer/prompt.rs:77` and `:127`), so the agent behaves the way it does on the desk:

```
Asked from jason@phone.
Design: .designs/landing.html (open in Unluminous's Designer)
The file is /Users/jason/dev/site/.designs/landing.html. Edit that file.
About the element h1, selector `main > h1.hero`.          <- or: This is about the whole design.
The element now: "Welcome to the site"

Request: make this heading purple and bigger

Change the design file to do this. The open notes on it can be read with:
  unluminous-cli plugins run designer comments --design .designs/landing.html open
```

A message that carries a note uses the `ask` text instead: the note's thread and the element it is on. The
Designer's version tells the agent to reply to the note and resolve it with `unluminous-cli`, which needs a
window. The board's version tells the agent to end with one sentence saying what it changed, and when the
turn finishes without a failure the board itself replies to the note with that sentence (as `claude`) and
resolves it, through the same code `POST /tasks/designs/notes` uses. The `Asked from` line comes from the
`x-design-client` header, as task-19's notes do.

The second and later messages of a conversation go with `--resume`, so they carry only the new request; the
CLI already holds what came before.

## 3. Images

- The phone and the web page upload each image with `POST /tasks/designs/images` (multipart, field `file`).
  It is `AttachmentService::store`, the same code `POST /tasks/:id/attachments` uses (task-18), so the image
  is checked by its first bytes, kept as `attachments/assets/<millis>-<stem>.<ext>`, and read back with
  `GET /tasks/asset?path=assets/...`. The answer is the same `{ relPath, markdown, contentType, bytes }`.
- A message names its images by those `assets/...` paths. The board refuses a path that is not a file the
  attachment service kept.
- The runner puts each image into the stdin message as an Anthropic image block
  (`{"type":"image","source":{"type":"base64","media_type":...,"data":...}}`), before the text. A test run
  on 2026-11-22 confirmed that `claude --input-format stream-json` passes such a block to the model.
  Desktop Agent-Chat writes picture paths into the text instead; the block is used here because the agent
  then sees the image without having to read a file first.
- The model accepts PNG, JPEG, GIF and WebP up to 5 MB. On a Mac, an image that is HEIC or BMP, or larger
  than 1,568 pixels on its long side, is converted to a JPEG of that size with `sips` first. Elsewhere such
  an image is refused with a sentence saying so. The phone already sends JPEG.
- The chat shows each message's images as thumbnails, from `GET /tasks/asset`.

## 4. The chat: how it is stored and streamed

**Stored.** `chat.json` is `{ version: 1, project, design, session, messages: [...] }`. A message:

```
{ id, role: "user" | "assistant", author, at, text,
  images: ["assets/..."], element: { selector, tag, text } | null, note: 3 | null,
  parts: [ { kind: "text", text } | { kind: "tool", tool: { id, name, title, input, answer, failed, done, took } } ],
  failure: null | "Stopped" | "...", took: ms, tokens: n }
```

A person's message has `text`, `images`, `element` and `note`. An assistant message has `parts` in the
order the agent wrote them, so text, a run of tool calls, and more text are shown in that order. `title` is
desktop Agent-Chat's `tool_title` (`components/agent_chat/message.rs:979`): a command's first line, or the
tool's name and the first of `file_path`, `path`, `file`, `pattern`, `query` or `url`. `input` and `answer`
are cut to 4,000 characters for the expanded view. The file is written to a temporary file and renamed. It
lives beside the board's database, so it survives app restarts, board restarts and a phone being replaced.

**Streamed.** The running turn is not in `chat.json` yet. Each state read parses `turns/<n>.out.jsonl` into
the assistant message so far:

| Line | What it does to the message |
|---|---|
| `system` / `init` | keeps the session id |
| `stream_event` `content_block_delta` `text_delta` | adds the text to the text part being written |
| `assistant` with a `text` block | replaces that text part with the finished text |
| `assistant` with a `tool_use` block | adds a tool part, not done yet |
| `user` with a `tool_result` block | marks that tool done, with its answer, and failed when `is_error` |
| `result` | the turn's time, tokens, and its failure when `is_error` |

The chat is part of `GET /tasks/designs/state`, which the phone and the web page already long poll. Its
revision includes the output file's size, so a new line of output ends the wait. While a turn is running the
state route looks every 250 ms instead of every 500 ms, so text reaches the phone within about a quarter of
a second of the CLI writing it. Only the newest 60 messages are sent; the board keeps all of them.

**The routes.** All are under `/tasks`, behind the gate.

| Route | Body | What it does |
|---|---|---|
| `POST /tasks/designs/chat` | `{ project, design, text, images?, selector?, element?, note? }` | Adds the person's message and starts a turn. 409 while a turn runs or the desktop's agent is answering in the project |
| `POST /tasks/designs/chat/stop` | `{ project, design }` | Stops the running turn |
| `POST /tasks/designs/images` | multipart `file` | Keeps one image, as above |
| `GET /tasks/designs/state` | | Now also has `chat: { messages, running, startedAt, desktop }` |

`POST /tasks/designs/request` stays for a client older than this ticket. It still goes to the window when
one has the project open, and it is refused with 409 while the board's agent is running a turn on that
design.

**How it looks on the phone.** The Design tab's panels become Chat, Notes and History, and Chat is the first.
It follows desktop Agent-Chat (`components/agent_chat/message.rs`, `composer.rs`), sized for a phone:

- The person's message is a raised card on the right, at most 82% of the width, with the corners of
  desktop's bubble (22, 22, 8, 22), and its images as thumbnails above the text, and the element or note it
  is about as a small line above that.
- The agent's text has no bubble. A speaker row comes first: a round disc with the spark mark and the name
  "Claude".
- A run of tool calls is one card, collapsed: a status well on the left (a running arc, a tick, or a cross
  when every call failed), the title (the one call's title, `Read 3 files`, `Ran 2 commands` or
  `Ran 4 tool calls`), and under it `Running <tool>` or the time it took. Tapping the card opens it: each
  call's state, title and time, and tapping a call shows its input and answer. This is desktop's behavior,
  including staying collapsed while running.
- Under a finished answer: its time and tokens, as `2m 41s · 1.2k tokens`.
- The composer is a carved well with the text box, and the round Send button at the end of the box, which is a
  Stop button while a turn runs. Above the box are the thumbnails of the images waiting to be sent, each with a
  remove button, and a chip for the element or note the message is about. Under it is a row of round buttons:
  Photo, Camera and Paste (task-18's pickers), the microphone (task-24's dictation), and Pick, which turns on
  element picking in the page. They are round icon buttons, as in desktop's composer, because the labeled
  buttons of the ticket sheet do not fit beside the microphone and Pick on a phone.
- While the keyboard is up the row of round buttons is put away, so the box, its chips and its pictures stay
  above the keyboard. Send acts when the finger lands on it: landing anywhere but the box takes the box's
  focus and the keyboard goes down in that frame, so the button would no longer be under the finger when it
  lifted.
- When the desktop's agent is answering in the project, a line above the composer says so.

**Selecting an element.** Pick turns on a mode in the page: the next tap on the page outlines the element
and is not passed to the page. The page script builds a selector for it the way the Designer's own picker
does (an id when there is one, else the tag and its classes and `:nth-of-type` up to the nearest id), and the
phone reads it back with `evaluateJavaScript`'s answer. The element becomes the message's chip, and the
message then goes with `selector` and `element`. A note's "Send to agent" button now puts the note into the
chat composer as a chip and opens the Chat panel, so the person can add words and images before sending.

**The web page.** `GET /designs` gets the same chat beside the page: the same messages and cards in HTML, a
composer that takes pasted images, files chosen with a file input and dropped files, a Pick button that
reads the element from the design's frame (the frame is on the board's own origin, so the page can reach
its document), and Stop.

## 5. Security

- Every new route is under `/tasks`, so it is behind the board's gate: a signed in session or the machine
  token. The images are served only by `GET /tasks/asset`, which is behind the gate too.
- A client never names a folder or a file. It names a project id and a design path, which are checked as in
  task-19, and image paths, which must be `assets/<file>` names the attachment service kept.
- The agent runs as the person who runs the board, on this Mac, with `--dangerously-skip-permissions`, as
  desktop Agent-Chat's Full setting and every board ticket agent already do. What it is asked to do comes only
  from a signed in caller.
- Nothing leaves the machine except the CLI's own calls to Anthropic. The board listens where it already
  listens: 127.0.0.1, the home network through the LAN forwarder, and the tailnet through `tailscale serve`.
  No new port is opened.
- The prompt text is passed on stdin as JSON, never on a command line, so nothing a person types becomes an
  argument.

## 6. When desktop Unluminous is also open

**The rule: the board's agent answers every chat message, whether or not a window has the project open.**

Reasons:

- The chat is then the same conversation every time, with its images, its stop button and its history, and
  it does not change shape when the Mac's window opens or closes.
- The Designer's verbs that `POST /tasks/designs/request` uses for a phone request (`--element`, `--whole`,
  `--as`) reached unluminous main with task-19 on 2026-11-22. A desktop built before that answers those verbs
  with an error, so sending to the window would work only on a desktop released after task-19.
- The window's Designer takes words only. It cannot carry an image.

**Keeping the two from editing the same design at once.**

- The board refuses to start a turn while a window has the project open and its Agent-Chat is answering. The
  phone says "Unluminous on the Mac is answering in this project. Wait for it to finish, or stop it there."
- The board refuses `POST /tasks/designs/request` while its own agent is running a turn on that design.
- While the board's agent runs, the state the window can see does not change, so a person at the desk who
  types into desktop Agent-Chat by hand is not stopped. The phone and the web page show that the desktop is
  also answering when it is. Stopping this case would need a lock that desktop Agent-Chat reads before it
  sends, which is a change to desktop Unluminous and a desktop release, and it is left as a follow up.

## 7. Test plan

**agent-tasks-service** (`cargo test`):

- The output parser: a recorded `out.jsonl` from a real run (text deltas, two tool calls, a failing tool, a
  result) becomes the expected parts, titles, done and failed flags, time and tokens.
- The prompt: the whole design, an element, and a note, with and without a window.
- The store: a message added, a turn folded, the file renamed into place, the session id kept.
- The routes, on a scratch board with throwaway folders and `AGENT_TASKS_DESIGN_AGENT` set to a script that
  writes a recorded stream and edits the design: a message starts a turn, the state shows it streaming, the
  design file changes, the turn is folded; a second message while it runs is 409; stop ends a script that
  sleeps; an image that the attachment service did not keep is refused; a turn survives a restart of
  `tasks-api` and is folded after it.
- The desktop rule: a fake window (a TCP listener answering the control channel's `plugins.view` with
  `streaming: true`) makes the chat route answer 409.
- One run with the real CLI on `haiku`: a message with a red PNG asks the agent to set the heading's color
  to the color in the image, and the design file then has red in it.

**unluminous-mobile**:

- Every control the XCUITests tap is written to the frame log as `event: place <name> at x,y size wxh` (the
  menu and its rows, the sign in fields, the design list, the panel tabs, the note buttons, and the chat's box,
  buttons and chip), and the tests tap the middle of those places instead of fixed points, so a change of
  layout does not break them.

- `cargo test`: the models read a state with and without `chat` (an older board), the tool cards group the
  same way desktop does (`Read 3 files`, `Ran 2 commands`, `Ran 4 tool calls`), the element script cannot be
  broken out of by a selector.
- XCUITest in the iOS Simulator against a scratch board on its own port, with its own database and a
  throwaway project folder, and no Unluminous window (`shells/ios/uitests/run-chat.sh`): open the design,
  paste an image from the pasteboard, pick the heading on the page, type a request, send, see the agent answer
  and the page reloaded, and see the design file changed on disk. Also: Stop during a turn, and the history
  keeping the conversation after the app is relaunched. task-19's Design XCUITest runs on the new layout too.

## 8. What the tests found

- The Design tab's hand made text box took no tap once task-25's selection code was in, in the Notes panel as
  well. Both boxes are now the ticket sheet's `Editor`, which a Simulator tap was already known to focus.
- See section 4 for the Send button and the keyboard.
- The look is checked on the installed build on the iPhone, not from development screenshots.

**The web page**: a Playwright run against the scratch board: sign in, open the design, paste an image, send,
see the answer.
