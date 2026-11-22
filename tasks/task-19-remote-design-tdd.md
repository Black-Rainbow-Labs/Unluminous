# task-19: following a Designer design from the phone and the web

Jason designs with an agent in desktop Unluminous's Designer and wants to follow the design from his phone:
in a new Design tab of the Unluminous phone app, and in a browser on any device on his tailnet. This document
decides what the phone and the web show, where the data comes from, how it travels, and how a design on the
Windows PC would reach the phone later. Everything in it is implemented in the same ticket.

## 1. What exists

| Piece | Where | What it already does |
|---|---|---|
| The Designer | `unluminous/crates/unluminous-app/src/services/designer/` | A design is an HTML file under the project's designs folder (`.designs` unless `plugins/designer/settings.conf` names another). Notes are `<name>.comments.json` beside it. A copy of the design is taken into `.unluminous/designs/<name>/<stamp>-<slug>.html` before each request to the agent. Requests go to Agent-Chat with two `plugins.run agent-chat` commands. |
| The Designer's verbs | `unluminous-cli plugins run designer <verb>` | `designs`, `comments`, `comment`, `reply`, `resolve`, `reopen`, `prompt`, `ask`, `history` and the rest, each with `--design <name>` and `--as <author>`. |
| A window's notes | `Designer::reread_the_notes` | A window reads a notes file again within two seconds when something else wrote it. |
| Agent-Chat | `services/agent_chat/` | Conversations are JSON files in `plugins/agent-chat/conversations/`, each with the folder its agent ran in. `plugins.view agent-chat` answers the live conversation, including the answer still arriving, and whether a turn is streaming. |
| The control channel | `unluminous-cli/docs/protocol.md` | One JSON line to `127.0.0.1:<port>` with the token from `instances/<pid>.conf`, one line back. The instance file names the window's project folder. |
| The board | `agent-tasks-service/crates/tasks-api` on `127.0.0.1:8096` | Every `/tasks` route is behind `authenticate`: a sign in token from `POST /auth/login`, or the machine secret from this Mac only. It already relays a ticket's terminal to the phone (`GET /tasks/:id/agent-terminal/ws`). The phone reaches it at `http://100.75.142.76:8096` through `tailscale serve`, and on the home Wi-Fi through `lan-forward.py`. |
| The phone app | `unluminous-mobile` | Rust, egui and rux on iOS. Four tabs (Board, Backlog, Epics, Settings). No web view. The only UIKit code is `shells/ios/src/keyboard.rs`, which the app reaches through `KeyboardBridge`, a queue the shell polls once a frame. |

## 2. What other tools do

| Tool | What the phone or browser sees | Updates | Comments from the phone | History |
|---|---|---|---|---|
| Figma | A native viewer of the real rendering. **Mirror** shows the frames selected on the desktop, with edits arriving live. [1] | Live | Long press to comment, quick reply, resolve your own. [1] | Not covered on mobile. [1] |
| Claude Design | Chat beside a live canvas. On the phone, the design opens full screen in the Artifacts tab. [2] | Not stated | Inline comments placed by clicking a spot; some comments are only in the comments view. [2] | None yet; you ask Claude to save. [2] |
| Orca (Stably AI) | Its phone app pairs with the desktop and shows agent chat and requests for input, not the rendered work. "Design Mode" sends a clicked element's HTML, styles and selector into the agent's prompt. [3] | Live chat | Replies to the agent | Diff review with line notes. [3] |
| Lovable | A preview link opened in the phone's browser, with a device toggle. [4] | Latest version | Guests can comment. [5] | Every change is a version you can preview or revert. [6] |
| Penpot | A share link to a view mode in the browser. [7] | Live | Commenters need an account. [7] | Autosaved versions for 7 days, pinned versions kept. [7] |

What this design takes from them:

- **The real rendering, not a picture of it.** Figma Mirror and Lovable both show the page itself. A rendered
  image would lose scrolling, hover states and responsive layout, which are what a design on a phone is checked
  for.
- **Notes pinned to an element, with threads and a resolved state**, as Figma and Lovable do, which is the
  model the Designer already has.
- **The agent's chat beside the work**, as Orca's phone app and Claude Design do, because "progress" on an agent's
  design is mostly what the agent is saying and doing.
- **History as before and after each request**, which Lovable does per change. The Designer already keeps a
  copy before each request, so each copy is the "before" and the next copy, or the design now, is the "after".

## 3. What "progress" is

The phone and the web show five things for a design, all live:

1. **The design, rendered**, reloaded within about a second of the agent writing the file.
2. **The notes**, open first then resolved, each with its thread, author and time.
3. **The history**: every request the Designer sent, newest first, with the words of the request and the time,
   and the copy before it, which can be opened in the same viewer.
4. **The agent now**: whether Agent-Chat is answering, its model, and the last messages of its conversation,
   including the answer still arriving and the names of the tools it ran.
5. **Whether a window has the project open**, because that decides what can be sent.

### Watch only, or act too

**Recommendation: act too.** The phone and the web can reply to a note, resolve or reopen one, make a note, and
send a request to the agent. The reason is that following an agent's design from a phone is only useful if you
can answer it: an agent that asks "do you want the hero darker or lighter?" in a note thread is otherwise stuck
until you get back to the desk. Figma's phone app and Orca's phone app both allow exactly this, and both stop
short of editing the design by hand, which this design also does not offer. A request from the phone goes
through the same Designer function the window's button calls, so it takes the same history copy first and can
be undone with Restore.

Two limits, each with its reason:

- **A request needs a window with the project open on the Mac**, because the agent is that window's Agent-Chat.
  With no window, the phone and the web say so and the request box is disabled. Notes still work without a window
  (section 5.4).
- **The web picks an element for a new note; the phone pins a new note to the page.** The web page shows the
  design in a same origin iframe, so it can outline the element under the pointer and build its selector the way
  `designer.js` does. Doing the same on the phone needs a script message handler between WKWebView and Rust;
  that is listed in section 10 as the next step. A phone note on the page is still useful, and a reply to an
  existing note keeps that note's element.

## 4. Where the data comes from and how it travels

### Options

| Option | For | Against |
|---|---|---|
| A. Desktop Unluminous runs a small HTTP server for its designs, and the board relays it | The window knows everything, including what is unsaved | Only works while a window is open; a second server in the desktop app to secure; a design closed in the window cannot be followed |
| B. The board reads the design files itself, and asks a running window only for what only it knows | Works with no window open; one server, already signed in and already on the tailnet; every file is already on the board's disk | The board has to know the Designer's file layout |
| C. The phone reads the files over a file share | None worth having | No sign in, no live agent state, a new service to expose |

**Chosen: B.** The board, agent-tasks-service's `tasks-api`, runs on the same Mac as the user who owns the
designs, so the files are already there. Designs, notes and history are files the Designer writes in a
documented format; the board reads them on each request. The two things only a running window knows, the
agent's live conversation and how to send it a request, are asked of that window over the control channel the
board can open because it runs as the same user and can read `instances/*.conf`.

So desktop Unluminous does not run a server. Its changes are small (section 7).

### Privacy

- Every data route is under `/tasks`, so it is behind `authenticate`: a sign in token, or the machine secret
  from this Mac only. Nothing about the gate changes.
- The board is reachable only on loopback, the home network forwarder and the tailnet. It is not on the public
  internet, and nothing here changes that.
- The rendered page needs its own pictures and stylesheets, and a WKWebView or an iframe cannot add an
  `Authorization` header to those requests. So the state answer includes a **view key**: 32 random hex
  characters, held in the board's memory for 12 hours, naming one project's designs folder. The page is loaded
  from `/design-view/<key>/<path inside the designs folder>`. That route is outside the gate and serves files
  only from under that folder, refuses `..` and absolute parts, and refuses any key it did not issue. A key is
  issued only to a signed in caller. This is the presigned URL pattern, and it keeps `authenticate` untouched.
- A client never names a directory. It names a project id the board issued (the first 12 hex characters of a
  SHA-256 of the folder) and a design path, which is checked to be an HTML file under that project's designs
  folder. This is the rule `AGENTS.md` section 4 states for terminals.

### Which projects

The board looks for projects in three places and keeps the ones that have at least one design:

1. Every running window's `folder` (`instances/*.conf`).
2. Unluminous's `recent.txt`.
3. The board's own agent projects.

Folders under the temporary folders (`/tmp`, `/private/tmp`, `/var/folders`) are left out, because agents make
scratch projects there. `AGENT_TASKS_UNLUMINOUS_HOME` names Unluminous's settings folder, which is
`~/Library/Application Support/Unluminous` on a Mac and `%APPDATA%\Unluminous` on Windows; the tests point it at
a scratch folder.

### The routes

All JSON, all under the gate except the last.

| Route | What it answers |
|---|---|
| `GET /tasks/designs` | Every project with designs: `{ projects: [{ id, name, folder, window, designs: [{ path, name, open, resolved, changed }] }] }` |
| `GET /tasks/designs/state?project=&design=&since=` | One design's progress (below). With `since`, waits up to 25 seconds for the revision to differ, checking every half second, then answers. |
| `POST /tasks/designs/notes` | `{ project, design, action: comment\|reply\|resolve\|reopen, note?, selector?, text? }` |
| `POST /tasks/designs/request` | `{ project, design, text, note?, selector? }`: a request to the agent, or `ask` for one note when `note` is given |
| `GET /design-view/:key/*path` | A file under the designs folder, or `~history/<copy>` for a history copy, with a `<base>` pointing at the design's own folder so its pictures still load |
| `GET /designs` | The web page (section 6). Holds no data; it signs in with `POST /auth/login` like the phone does. |

The state answer:

```json
{
  "revision": "9f2c…",
  "project": "a1b2c3d4e5f6", "design": ".designs/landing.html", "name": "landing.html",
  "viewUrl": "/design-view/<key>/landing.html",
  "changed": 1795200000,
  "window": { "pid": 4123, "open": true },
  "notes": [{ "id": 3, "state": "open", "selector": "main > h1", "tag": "h1.hero", "excerpt": "Make…",
              "messages": [{ "author": "you", "body": "…", "at": "2026-11-22T20:41:00Z" }],
              "created": "…", "resolved": null }],
  "history": [{ "name": "20261122-204100-make-the-hero-darker", "words": "make the hero darker",
                "when": "2026-11-22 20:41:00", "url": "/design-view/<key>/~history/…" }],
  "agent": { "state": "streaming", "streaming": true, "model": "…", "source": "window",
             "messages": [{ "role": "assistant", "text": "…", "tools": ["Edit"] }] }
}
```

`revision` is a SHA-256 over the design file's modified time and length, the notes file's, the list of history
copies, and the agent's state and last message, so a client reloads the page only when the page changed and
redraws the rest when anything changed. `agent.source` is `window` when a window answered, `file` when it was
read from the newest conversation file whose folder is the project, and `none`.

### Live updates

Long polling: the client asks for state with the revision it has, and the board answers when it changes or
after 25 seconds. Both the phone (blocking `ureq` on its worker thread) and the page (`fetch`) can do this with
nothing new, it passes through `tailscale serve --tcp` and `lan-forward.py` unchanged, and a phone that slept
just asks again. A WebSocket, which the terminal relay uses, would save nothing here: the board has to look at
the disk on a timer either way, because files can change under it.

## 5. The board's half

A new module, `crates/tasks-api/src/designs/`, in the layered shape the service already has:

| File | What it holds |
|---|---|
| `mod.rs` | The routes. |
| `projects.rs` | Finding projects, the designs folder of each, and project ids. |
| `files.rs` | Listing designs, reading notes and history, checking paths. The file layout is the Designer's, read the way `library.rs`, `notes.rs` and `history.rs` read it. |
| `window.rs` | Finding a running window for a project and speaking the control channel to it. |
| `notes.rs` | Writing a notes file when no window has the project open. |
| `view.rs` | View keys and serving files. |

### 5.1 Agent state

With a window: `plugins.view` with `{ "id": "agent-chat" }`, keeping `state`, `streaming`, `model` and the last 8
messages of `conversation`, text only, each cut to 4,000 characters, with tool names. Pictures are dropped.
Without a window: the newest file in `plugins/agent-chat/conversations/` whose `folder` is the project, read the
same way. A window answer is cached for 400 ms per project so several clients do not each open a socket every
half second.

### 5.2 Requests

`plugins.run` with `{ id: "designer", command: "prompt", arguments: ["--design", <name>, "--as", <who>,
"--element", <selector> | "--whole", <text>] }`, or `ask <note>` for one note. `<who>` is `<user>@phone` or
`<user>@web`, from the signed in user and the client's `x-design-client` header. It has no spaces, because the
window splits a verb's words on spaces.

### 5.3 Notes with a window

The same `plugins.run designer` verbs: `comment <selector> -- <text>`, `reply <note> <text>`, `resolve <note>`,
`reopen <note>`, with `--design` and `--as`. Going through the window keeps its own copy of the notes the one
that is written, so a note written from the phone cannot be overwritten by the window's older copy.

### 5.4 Notes with no window

The board writes the notes file itself, as `serde_json::Value`, keeping every field it does not change, using
the Designer's `next` counter, its ISO times and its anchor shape. A window opened later reads it like any other
file. This is the one place the board writes into a project, and it writes only `<name>.comments.json` beside a
design it has already checked is one.

## 6. The web page

`GET /designs` is one HTML file with its script and styles inline, compiled into `tasks-api` with
`include_str!`. It is built for a phone first, in the dark look of the board's other clients, and works in any
browser on the tailnet: `http://100.75.142.76:8096/designs`.

- **Sign in**: the board's own form, `POST /auth/login`, the token kept in `localStorage`.
- **List**: projects and their designs, with open note counts.
- **Design**: the page in an iframe from its view URL, at the device width chosen (fit, phone, tablet,
  desktop), reloaded when `changed` moves, keeping its scroll.
- **Notes**: numbered pins drawn over the iframe at each note's element, and the list beside or under it. A
  note's thread with Reply, Resolve and Reopen.
- **Comment mode**: hover outlines an element, a click chooses it and opens a note box; the selector is built
  the way `designer.js` builds one (ids, then classes, then `:nth-of-type`, checked to find one element).
- **History**: the copies, each opening in the iframe, and Back to now.
- **Agent**: the state and the last messages, and a request box that is disabled with a sentence when no window
  has the project open.

## 7. Desktop Unluminous's half

1. **`prompt` takes `--element <selector>` and `--whole`.** A request from the phone must not quietly carry
   whatever element happens to be chosen in the window. `--element` sends that element by selector (its excerpt
   and markup are read from the file), and `--whole` sends none.
2. **`prompt` and `ask` say who asked** when `--as` is given: the request starts with `Asked from jason@phone.`
   so the agent and the transcript show where it came from.
3. **`unluminous-cli/docs/commands.md`** documents both, and the Designer's catalogue line for `prompt` says so.

No server and no new setting. The window already rereads a notes file written by someone else, and already
reloads its page when the design file changes.

## 8. The phone's half

### 8.1 Rendering the HTML

| Option | For | Against |
|---|---|---|
| WKWebView inside the egui app | The real page, scrolling, zoom, hover on tap; the same engine as Safari; reloads cheaply | A native view over the Metal layer, positioned and hidden by the shell |
| A rendered image of the page | Drawn by egui like everything else | Something has to render it: a headless browser on the Mac, which the board does not have; no scrolling or responsive layout; stale between renders |
| Open the page in Safari | No code | Leaves the app; no notes beside it |

**Chosen: WKWebView.** wry adds a WKWebView to the window's UIView in its iOS path [8]. This does the same with
`objc2` directly, because the shell already uses it and wry would bring a second event loop model. It sends the
UIKit and WebKit messages with `msg_send!`: `objc2-web-kit` 0.3 declares `WKWebView` for macOS only, as a
subclass of `NSView`, so on iOS that crate has no `WKWebView` type at all. The approach is the keyboard's: the app crate stays free of platform code and
holds a `WebViewBridge` that says "show this URL in this rectangle, in points" or "show nothing", plus a queue of
scripts to run. `shells/ios/src/web_view.rs` reads it once a frame, makes one `WKWebView` the first time it is
asked for, adds it to the key window, sets its frame, loads a URL when it changes, hides it when nothing asks for
it, and runs the queued scripts with `evaluateJavaScript:completionHandler:`. egui cannot draw over a native view,
so the Design screen lays out the page in a rectangle of its own and draws its controls around it, and the view
is hidden while a sheet is up.

The desktop preview shell has no web view and draws a box saying the page shows on the phone.

### 8.2 The Design tab

A fifth tab, `Design`, between Epics and Settings, with rux's `Icon::Image` (rux has no brush or palette mark).

- **List**: each project with designs, each design with its open and resolved note counts, and whether a window
  has it open.
- **Design**: a header with the name and a back button; the page; under it a segmented choice of `Notes`,
  `Agent` and `History`.
  - Notes: open first, then resolved, each with its thread. Tapping a note scrolls the page to its element and
    outlines it, and each open note has a numbered pin drawn in the page. Reply, Resolve, Reopen and Send to
    agent; New note, which opens a box and pins the note to the page.
  - Agent: the state, the model and the last messages; a request box and Send, disabled with a sentence when no
    window has the project open.
  - History: the requests, newest first; tapping one shows its copy, with Back to now.
- **Live**: the store long polls state on a thread of its own while a design is open, and the page reloads when
  `changed` moves.

The client half is in `tasks-client`: models for the answers and four methods on `TasksClient`. The view URL is
joined to the board's base address.

## 9. A design on the Windows PC

The PC's board is the same `tasks-api` binary, so the same module serves the PC's designs from the PC's files
and the PC's Unluminous windows, with `%APPDATA%\Unluminous` as the settings folder. The first version supports
this Mac only, because the phone talks to one board.

The next step, not built here: the Mac board gains a list of other boards it may relay to (the PC's, reached the
way machine coordination already reaches it), and `GET /tasks/designs` includes their projects under the
machine's name, with each route forwarded by project id. The phone and the web then need no change, because a
project id is already opaque to them.

## 10. Not in this version

- Picking an element for a new note on the phone (a `WKScriptMessageHandler`, or polling a picker script with
  `evaluateJavaScript`, which is how the desktop's `designer.js` reports).
- Editing the HTML by hand from the phone or the web.
- Restoring a history copy from the phone or the web. Restore changes the design, so it stays a desk action.
- Designs on the PC (section 9).
- Push notifications when the agent answers.

## 11. Tests

| Repository | What is tested |
|---|---|
| agent-tasks-service | Project discovery against a scratch settings folder; the path checks (`..`, absolute, not a design); listing, notes and history read from fixture files; writing a note with no window keeps unknown fields and the `next` counter; the view key serves a file, refuses a path outside, refuses an unknown key, and puts a `<base>` into a history copy; the state long poll answers early when a file changes; the control channel against a scripted window on a loopback port (agent state, a request, a note); the web page is served; every new `/tasks` route answers 401 with no credential. |
| unluminous | `prompt --element` and `--whole` and the `Asked from` line, in the Designer's own tests. |
| unluminous-mobile | The client models parse a real state answer; the store's long poll loop; the Design screen's list and design view in the headless egui tests; an XCUITest in the Simulator that signs in to a scratch board, taps the Design tab, taps a design, and checks the page and a note are shown. |

## 12. What was built and measured

- **Board** (`agent-tasks-service`): `tasks_core::services::designs` and `tasks-api/src/routes/designs.rs`, as
  sections 4 to 6 describe, with 16 tests in `crates/tasks-core/tests/designs_smoke.rs` (one of them speaks the
  control channel to a scripted window on a loopback port). `scripts/auth-exercise.cjs` passes 255 of 255
  against a scratch board with the new routes, including the 401 for every new gated route.
- **Desktop** (`unluminous`): `prompt --element` and `--whole`, and `Asked from <who>.` on `prompt` and `ask`.
  Driven live: a request sent to the scratch board as the phone reached a real window's Agent-Chat, the agent
  changed the design's heading, and the state's `pageRevision` moved, which is what reloads the page.
- **Web page**: driven in WebKit through Playwright on loopback: sign in, the list, the design in the iframe,
  pins, a note made by clicking the page's heading arriving back through the long poll, the agent and history
  panels.
- **Phone** (`unluminous-mobile`): 8 headless tests of the Design tab and 5 client tests, and 4 XCUITests with
  real touches on a Simulator (`shells/ios/uitests`): open a design, the page in the WKWebView with the starter
  page's heading rendered in it, a note typed on the keyboard reaching the notes file, Resolve and Reopen, and
  Back hiding the page.
- **Found on the way**: rux's `TextArea` makes its egui id from its position, so a box that moved when the
  keyboard rose lost its focus and the keyboard closed at once. The Design screen's boxes have ids of their own.
- **The tailnet**: `tailscale serve --tcp` forwards the tailnet port to loopback unchanged, which is how the
  phone already reaches the board. This Mac's `tailscaled` runs with userspace networking, so the Mac cannot
  reach its own tailnet address (the board on 8096 does not answer that way either), and the check over the
  tailnet itself is the phone's once the board's `tasks-api` is restarted with this build.

## Sources

1. Figma Help Center, "Use the Figma mobile app": https://help.figma.com/hc/en-us/articles/1500007537281
2. Claude Help Center, "Get started with Claude Design": https://support.claude.com/en/articles/14604416-get-started-with-claude-design
3. Stably AI, Orca: https://github.com/stablyai/orca
4. Lovable docs, preview: https://docs.lovable.dev/features/projects/preview
5. Lovable docs, sharing a project: https://docs.lovable.dev/features/share-project.md
6. Lovable docs, history: https://docs.lovable.dev/features/projects/history.md
7. Penpot user guide, view mode: https://help.penpot.app/user-guide/view-mode/
8. wry source, `src/wkwebview/mod.rs` (the iOS path adds the WKWebView to the window's UIView): https://docs.rs/crate/wry/latest/source/src/wkwebview/mod.rs
