# Designer: a PRD for designing in HTML with an agent beside you

`task-2254`. *"We want a designer plugin that works similar to Claude Design, Orca design, etc. It should
allow users to interact with an html design, add comment notes like figma, select components to put notes
on, chat with an agent to make the changes, etc. ... It should match our other designs in agent tasks, rux,
agent chat, etc. The current agent chat should be used to control the designs. Design files should be
stored in a .designs folder, but also can be configured through the setting modal."*

`_agent_output/task-2254-designer/research.md` is the survey of the tools this is measured against, with its
sources. `tasks/task-2254-designer-plugin-tdd.md` is how it is built.

## 1. The problem

Designing a screen with an agent today, in Unluminous, is three windows' worth of work done in one. A
person asks the agent in the chat pane to write an HTML mock, opens the file as a browser tab to look at
it, and then describes what is wrong in words: "the second card in the pricing row, the button under the
price, make it less loud". The agent has to work out which element that sentence means by reading the
HTML, and often picks the wrong one. Nothing the person says stays attached to the design, so the
feedback from Monday is gone by Tuesday, and there is no way to see which notes were dealt with.

Every tool in the survey solves the same three things, and Unluminous has none of them for a design:

1. **Pointing.** Clicking an element of the rendered page says exactly which element is meant. v0, Lovable,
   Bolt, Builder.io, Onlook and Claude Design all select a DOM element by clicking it and hand the agent
   that element rather than a description of it.
2. **Notes that stay put.** Figma's comments are pins on the canvas, in threads, that are resolved when
   they are dealt with. A note is the record of what was asked for and whether it was done.
3. **Asking for the change where you are looking.** The request to the agent carries the element and the
   note, so the agent edits the right thing the first time.

The closest tool to this design is the Design Mode of Orca (Stably AI's open source IDE for running
Claude Code and Codex), which captures a clicked element's HTML and computed styles and sends a tray of
notes to the agent's terminal. Lovable has the best pinned comments of the tools surveyed, and Claude
Design has the Comment mode and the `Send to Claude` button this pairs with Agent-Chat. Figma's pins are
coordinates, and its forums show them drifting when frames move, which is why a note here is anchored to
an element and marked `Detached` rather than left floating.

## 2. Who it is for

- **Jason, designing a screen for one of his products**, who writes the design as HTML because that is what
  the agents write well, and who wants to iterate on it the way he would in Claude Design: look, point,
  say what is wrong, watch it change.
- **An agent** working in the Agent-Chat pane, which needs to know which element a note is about, read the
  open notes, change the HTML, and mark notes as dealt with. Unluminous's rule is that everything a person
  can do here an agent can do too, through the same command.

## 3. What a design is

**A design is one HTML file in the project's designs folder**, `.designs/` by default, and changeable on
the Designer page of Settings. It is an ordinary file: it is committed with the project, it is diffed by
git, and the agent edits it with its own tools. Unluminous holds no second copy of it.

**A design's notes are a JSON file beside it**, `<name>.comments.json`, so they travel with the design,
are committed with it, and can be read by any agent or person with a text editor. What one person was
doing in a design (the device size, the zoom, which note was open) is not shared, and lives in the
project's own `.unluminous` folder.

## 4. What a person can do

### 4.1 Make and open designs

- `Designer -> New Design` asks for a name and writes `<folder>/<name>.html` from a starter page drawn in
  the house style, then opens it.
- A `.html` file inside the designs folder opens as a **design tab** when it is opened from the explorer,
  from `Go to File`, from the Designs pane or from the command line. An HTML file anywhere else opens as it
  always did.
- `Open Source` on the design tab's toolbar opens the same file as text, for anybody who wants to edit it
  by hand. A save there reloads the design.
- The **Designs pane** lists every design in the folder with how many notes are open on it, and has `New
  Design`. It is a pane like every other one: it can be docked to any edge, zoomed and put away.

### 4.2 Look at a design

- The page is drawn by the window's own web view, at the width of a chosen **device**: `Desktop` (1440),
  `Laptop` (1280), `Tablet` (834), `Phone` (390), or `Fit`, which is the width of the tab. A page wider than
  the room it has is zoomed down to fit, so the whole width is always visible and the page lays itself out
  at the device's width rather than the tab's.
- The page reloads when its file changes on disk, which is what makes an agent's edit appear without
  anyone pressing anything.

### 4.3 Point at an element

- The toolbar has three modes, in a pill switch: **Browse** (the page behaves as a page: links and buttons
  work), **Select** (hovering outlines an element and labels it, a click chooses it), and **Comment** (as
  Select, and the click opens a new note on the element).
- The chosen element is named in the sidebar the way the page's own picker names it: its tag, its id or
  first two classes, and a selector that finds it and nothing else. `Escape` clears the choice. A
  selection survives a reload as long as the selector still finds something.
- `Parent` and `Child` on the sidebar move the choice up and down the tree, which is how a small element
  inside a larger one is reached (Bolt calls this "Pick from layers"; Orca and Lovable both have it).

### 4.4 Leave notes

- A note is pinned to an element, with a short excerpt of the element's text so it can be found again by a
  person. Its pin is a numbered disc drawn in the page at the element's top right, which follows the
  element as the page scrolls and as the device changes.
- A note is a thread: the first message, replies, and who wrote each one (`you`, or the agent).
- A note is **open** or **resolved**. Resolved notes are hidden from the page and kept in the list under
  `Resolved`, where they can be opened again.
- A note whose element has gone (the agent rewrote that part of the page) is kept, drawn at the place it
  was last seen, and marked `Detached` so it can be moved onto another element or resolved.
- Clicking a pin in the page opens its thread in the sidebar, and clicking a thread in the sidebar scrolls
  the page to its element and outlines it.

### 4.5 Ask the agent

**The agent is the one in the Agent-Chat pane**, the one the person already has signed in and already
talks to. The designer does not run its own.

- **`Ask agent`** on a note sends that note to the chat: the design's path, the element's selector, its
  excerpt and the start of its HTML, and the note's words. The chat pane opens if it is put away. The
  request also carries a few of the element's computed styles (its font, size, colour, background,
  padding and margin), which is what Orca's Design Mode sends and what saves the agent reading the
  stylesheet to learn them. The request goes as an ordinary message, so it appears in the conversation, it is queued if an answer is
  already arriving, and it can be read back later.
- **`Send open notes`** sends every open note on the design in one message, numbered, which is the
  Claude Design habit of collecting several remarks and then asking once.
- **A prompt box under the page** sends a request about the whole design, or about the chosen element if
  there is one, without making a note first. "Make the hero darker" with the hero chosen is one message.
- Every message tells the agent the commands it can use to answer: read the notes, reply to one, and
  resolve one. When the agent says it has dealt with a note, the note shows the reply and is resolved.

### 4.6 Go back

- **Before each request is sent, the design is copied** into the project's history for that design, named
  by the time and the first words of the request. `History` on the toolbar lists the copies, and
  `Restore` puts one back (copying the current file first, so a restore can itself be undone).
- Git still holds whatever was committed; this covers the minutes between commits, which is where an
  agent's edit most often needs undoing.

### 4.7 Settings

The Designer page in Settings has:

- **Designs folder**, relative to the project, `.designs` by default.
- **Default device**, one of the five.
- **Starter page**, `House style` or `Blank`.
- **Send the HTML of the element**, on by default: how much of the element's markup goes with a request
  (off sends the selector and the excerpt alone).
- **History copies kept a design**, 20 by default.

## 5. What an agent can do

Everything above, through `unluminous-cli plugins run designer <verb>`, generated into the MCP tools with
nothing written by hand:

| Verb | Does |
|---|---|
| `designs` | The designs in the folder, with their open and resolved note counts. |
| `new <name>` | Make a design from the starter page and open it. |
| `open <name>` | Open a design in a design tab. |
| `device <name>` | Change the device of the showing design. |
| `mode browse\|select\|comment` | Change the mode. |
| `select <selector>` | Choose an element by selector, as a click would. |
| `selection` | What is chosen: selector, tag, excerpt, the start of its HTML. |
| `comments [open\|resolved\|all]` | The notes on the showing design (or `--design <name>`). |
| `comment <selector> <text>` | Make a note on an element. |
| `reply <note> <text>` | Reply to a note, as the agent. |
| `resolve <note>` / `reopen <note>` | Change a note's state. |
| `ask <note>` / `ask-all` / `prompt <text>` | Send to Agent-Chat, as the buttons do. |
| `history` / `restore <copy>` | The copies, and putting one back. |

## 6. How it looks

It is drawn in the language the Agent-Tasks board, the Agent-Chat pane and the Settings window already use:
`rux` surfaces, a carved well for the page, pill switches for the device and the mode, round chip buttons,
note threads as raised cards like chat messages, the house dark and light themes, and nothing that names a
colour of its own. `_agent_output/task-2254-designer/mocks/` holds the HTML mocks the implementation is
measured against.

## 7. What it does not do, in v1

Each of these is something a tool in the survey has; each is left out for a stated reason.

- **A canvas of several artboards** (Claude Design, Figma, Stitch). One design is one page. The Realm is
  the canvas Unluminous already has, and a browser node can hold a design today.
- **Direct manipulation**: dragging an element, a style panel for padding and colour (Onlook, Webflow,
  Builder.io). That is a second editor of the HTML beside the agent and the text editor, and two writers of
  one file need a merge story this version does not have.
- **Variants side by side** (v0, Stitch). One page at a time. History copies cover "go back to the last
  one".
- **Its own model.** Unluminous runs the agent the person already has, which is the Agent-Chat design.
- **Multiple people commenting at once.** Notes are a file in the project, shared through git.
- **Designs that are not one HTML file**: a React component, a Figma file, an image. A design may link
  stylesheets, scripts and pictures beside it in the folder; they load as the browser tab already loads
  them.
- **Screenshots of the page from Unluminous's own `window screenshot`**, which cannot contain a native page.
  `tools/capture-window.ps1` can.

## 8. How we know it works

- A person can make a design, select the hero heading, leave a note, press `Ask agent`, watch the agent
  change the file, see the page reload, and see the note resolved with the agent's reply, without leaving
  the window or typing a selector.
- An agent driving only `unluminous-cli` can do the same sequence: `new`, `select`, `comment`, `ask`,
  `comments`, `reply`, `resolve`.
- A note's pin follows its element across a scroll, a device change and an agent edit that keeps the
  element, and is marked `Detached` after one that removes it.
- History restores the design to the copy taken before a request.
- Changing the designs folder in Settings changes what the pane lists and which HTML files open as designs.
