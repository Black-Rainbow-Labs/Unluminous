# The Designer

A plugin that draws a pane for designing a screen in HTML with the agent you already have beside you. A
design is one HTML file. You open it in a tab of the Designer pane, click an element, leave a note on it,
press a button, and the agent in the Agent-Chat pane changes the file while the page reloads.

Like the other panes a plugin contributes, the Designer is an ordinary `ui` plugin. Switching it off in
`Settings -> Plugins` withdraws its pane, its `Designer` menu and its Settings page on the next frame.
[The agent panes](agent-panes.md) describes Agent-Chat, which answers the notes.

## A design is a file

**A design is one `.html` file in the designs folder**, which is `.designs` in the project unless
`Settings -> Designer` names another folder. It is an ordinary file. It is committed with the project,
diffed by git and edited by the agent with the agent's own tools. Unluminous keeps no second copy of it.

A `.html` file inside the designs folder opens as a design tab when it is opened from the explorer, from
`Go to File`, from the Designs list or from the command line. An HTML file anywhere else opens in a browser
tab as it always did. A design may link stylesheets, scripts and pictures beside it, and they load as they do
in a browser tab. A file that is not HTML, such as a React component or an image, is not a design.

**The notes are a JSON file beside the design**, `<name>.comments.json`, so they travel with the design and
are committed with it. What one person was doing in a design, such as the device and the zoom, is kept in the
project's own `.unluminous` folder and is not shared.

## Opening the pane

`Designer -> Show Designer` opens the pane, which is docked at the left by default. It is a pane like every
other: it can be dragged to any edge, zoomed and put away. The `Designer` menu also has `New Designer Tab`,
`New Design...`, `Send Open Notes to Agent`, `Design History`, and the two modes `Browse` and `Comment`.

The pane has tabs of its own, apart from the files open in the editing area. A blank tab lists the designs
in the folder, with how many notes are open on each, and has `New Design`. There is always at least one tab.
A design opened from the blank tab takes that tab, and a design opened from another design's tab gets a tab
after it. The tabs, and which one was showing, are saved in `.unluminous/designs.conf` and come back with the
project. A design whose file has gone is left out.

`New Design...` asks for a name and writes `<folder>/<name>.html` from a starter page, either in the house
style or blank, and opens it. `Open Source` on a design's toolbar opens the same file as text, and a save
there reloads the design.

## Looking at a design

The page is drawn by the window's one native web view, in a carved well. It is drawn at the width of a
chosen **device**:

| Device | Width |
|---|---|
| `desktop` | 1440 |
| `laptop` | 1280 |
| `tablet` | 834 |
| `phone` | 390 |
| `fit` | the width of the tab |

A page wider than the room it has is scaled down so that the whole width is visible, and the page lays itself
out at the width of the device and not at the width of the tab. The page reloads when its file changes on
disk, so an edit by the agent appears without anyone pressing anything. The device a design was last viewed
at is remembered.

**No screenshot Unluminous takes contains the page.** The page is a native child window, so `window
screenshot` of a design shows the toolbar, the sidebar and the prompt box with an empty rectangle where the
page is. `tools/capture-window.ps1` can photograph the page on Windows.

## Choosing an element and leaving a note

The toolbar has two modes in a pill switch. In **Browse** the page behaves as a page, so links and buttons
work. In **Comment**, hovering outlines an element, and a click chooses it and opens a new note on it.

The chosen element is named in the sidebar by its tag, its id or first two classes, and a selector that finds
it and nothing else. `Escape` clears the choice, and a choice survives a reload while the selector still
finds something. `Parent` and `Child` in the sidebar move the choice up and down the tree, which reaches a
small element inside a larger one.

A note is pinned to an element, with a short excerpt of the element's text. Its pin is a numbered disc drawn
in the page at the element's top right, and the pin follows the element as the page scrolls and as the
device changes. A note is a thread: a first message, replies, and who wrote each one. A note is **open** or
**resolved**. Resolved notes are hidden from the page and listed under `Resolved`, where they can be opened
again. A note whose element has gone, because the agent rewrote that part of the page, is kept at the place
it was last seen and marked `Detached`, so it can be moved to another element or resolved. Clicking a pin
opens its thread in the sidebar, and clicking a thread in the sidebar scrolls the page to its element.

## Sending to the agent

The agent is the one in the Agent-Chat pane. The Designer runs no model of its own, so a request needs
Agent-Chat switched on and an agent set up in it.

- **Ask agent** on a note sends that note to the chat: the design's path, the element's selector and
  excerpt, the start of its HTML, and the note's words. The chat pane opens if it is put away. The request
  is an ordinary message, so it appears in the conversation and waits its turn if an answer is arriving.
- **Send open notes** sends every open note on the design in one message, numbered.
- **The prompt box under the page** sends a request about the whole design, or about the chosen element if
  there is one, without making a note first.

Every message tells the agent the commands it can use to answer: read the notes, reply to one and resolve
one. When the agent replies, the note shows the reply, and resolving it marks the note resolved.

## History

Before each request is sent, the design is copied into `.unluminous/designs/<path>/`, named by the time and
the first words of the request. `History` on the toolbar lists the copies and `Restore` puts one back. A
restore first copies the current file, so a restore can itself be undone. The newest copies are kept, 20 by
default and up to 200. Git still holds whatever was committed. History covers the minutes between commits,
where an agent's edit most often needs undoing.

## Settings

`Settings -> Designer` has five settings:

| Setting | Meaning |
|---|---|
| Designs folder | relative to the project, `.designs` by default |
| Default device | one of the five devices above |
| Starter page | `House style` or `Blank` |
| Send the HTML of the element | on by default. When it is off, a request carries the selector and the excerpt alone |
| History copies kept a design | 20 by default |

## From the command line

`unluminous-cli plugins run designer <verb>` does everything the buttons do, and the MCP tools are generated
from the same catalogue.

```sh
unluminous-cli plugins run designer designs
unluminous-cli plugins run designer new Landing page
unluminous-cli plugins run designer open Landing page
unluminous-cli plugins run designer device phone
unluminous-cli plugins run designer select main > h1
unluminous-cli plugins run designer comment main > h1 -- Make this larger
unluminous-cli plugins run designer ask 1
unluminous-cli plugins run designer reply --as claude 1 Changed to 48px
unluminous-cli plugins run designer resolve 1
```

| Verb | Does |
|---|---|
| `open-pane` | shows the Designer pane |
| `designs` | lists the designs with their open and resolved note counts |
| `new <name>` | makes a design from the starter page and opens it |
| `open <name>` | opens a design in a tab of the pane |
| `tabs`, `new-tab`, `show-tab <number or name>`, `close-tab [number or name]` | work on the pane's tabs, counted from 0 |
| `device desktop\|laptop\|tablet\|phone\|fit` | changes the device of the showing design |
| `mode browse\|comment`, `mode-browse`, `mode-comment` | change the mode |
| `select <selector>`, `selection` | choose an element as a click would, and say what is chosen |
| `comments [open\|resolved\|all]` | lists the notes on a design |
| `comment <selector> <text>` | makes a note on an element. A selector with spaces in it is ended with `--` |
| `reply <note> <text>`, `resolve <note>`, `reopen <note>`, `delete <note>` | work on a note |
| `move <note> <selector>` | pins a note to another element |
| `ask <note>`, `ask-all`, `prompt <text>` | send to Agent-Chat as the buttons do |
| `history`, `restore <copy>` | list the copies, and put one back by name or by place in the list |
| `reload`, `source` | load the page again, and open the design's file as text |

A verb about the showing design takes `--design <name>` as its first two words, so an agent can work on a
design nobody is looking at. `--as <name>` says who is writing a note or a reply, and is `agent` when it is
left out. `select`, `device` and `mode` need the page, so they refuse and name `open` when the design is not
showing.

## What it does not do

One design is one page. There is no canvas of several artboards, no dragging of elements and no style panel,
because a second editor of the HTML beside the agent and the text editor would need a way to merge two
writers. There are no variants side by side, only the copies History keeps. The [Realm](the-realm.md) is the
canvas Unluminous already has, and a browser node can hold a design. Notes are a file in the project, shared
the way any file is shared, through git.
