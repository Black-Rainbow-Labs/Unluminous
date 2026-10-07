# The Realm

A fifth panel holding an infinite canvas, and ten kinds of node that can live on it: a terminal, a web
page, a folder tree, a file editor, an agent chat, the task board, a picture, a note, a sound and a
video. Nodes are wired to each other, and **a connection is what lets an agent running in a terminal
node act on the node it is wired to**.

**A realm is a file in the project.** Each one is a `.realm` file, kept in `.realm-files/` unless it
was saved somewhere else, so a realm is committed, reviewed, copied and shared like any other file in
the repository. A project can hold as many as it likes, and one of them is open at a time.

`View -> Realm` opens the panel, `Cmd/Ctrl+Shift+M` maximises whatever panel has the keyboard, and
`unluminous-cli realm` drives the whole of it. [What it looks like](overview.md#the-realm) has a
picture.
[What it looks like](overview.md#the-realm) has a picture.

## Why it is core rather than a plugin

The ticket asked for it as a plugin and the answer was no, for a reason with three parts: a File
Editor node needs the window's open files, a Document and the editing area; a browser node needs the
one native child view the window owns; and an Agent-Tasks node needs the one board there is. None of
those is reachable from a plugin.

What it keeps from the plugin shape is everything a panel already buys. It is a fifth variant of the
window's own panel list, so the rail button, the four `Move to` rows, the drop bands, the divider and
the maximise chord all arrived with no code of their own.

**The `realm` plugin is the switch for it.** The plugin is `kind = ui` and names `ui.provider =
realm`, a provider built into Unluminous, listed in `plugins::CORE_PROVIDERS` because the window draws
it rather than a provider object. What the plugin contributes is the file type and the folder:
`ui.extensions = .realm` makes a `.realm` file open in a realm tab from the explorer, with the plugin's
icon on its row, and `explorer.shows = .realm-files` lists that folder in the explorer although its name
starts with a dot. Switching the plugin off in `Settings -> Plugins` takes away the panel, its rail
button, its `View` menu rows, its commands, the icon and the folder, and a `.realm` file opens as text.

## A zoom costs a matrix, not a relayout

Each node draws into a layer of its own carrying the camera, made a sublayer of the pane's so it is
composited directly above it. A terminal keeps its cell count and an editor keeps its line breaks
while the canvas is scaled, which is what "smooth" has to mean.

The cost is stated rather than hidden: a layer's mesh is tessellated at its own scale and then scaled,
so **text at a zoom other than 1.0 is scaled pixels**. At 1.0, the default and where somebody reads
code, it is exact. Two things follow. A node's decoration is recorded in **screen** points into the
pane's own canvas, because that canvas is rasterised once underneath every node layer, so the
Gaussians are drawn at the size they are seen at. And a node's contents are clipped clear of the
window's own resize grips — a sublayer is above the pane, so without that a node against the window's
edge would take the drag that resizes the window.

**A zoom glides.** One notch of a mouse wheel is fifty units of scroll, so the camera used to take the
whole ten per cent on the frame the notch arrived and sit there. It is geometric rather than linear,
so a step from 1.0 to 1.1 and one from 2.0 to 2.2 take the same time: what a person reads as a step of
zoom is the ratio. The point it is about is remembered with it, because the pointer moves during a
glide. And `realm camera --zoom` sets both at once, because a script that had to wait out an animation
to read back what it just set is a script with a race in it.

## The ten kinds of node

| | What it is |
|---|---|
| `terminal` | a shell, or the program named by `--command`, in the project folder |
| `browser` | a rendered web page, local or remote |
| `folder` | a folder tree with its own filter box |
| `editor` | a file, with the gutter, the folds, the breakpoints, git blame, find, go to definition and everything else |
| `chat` | an Agent-Chat conversation of its own |
| `tasks` | the window's one Agent-Tasks board |
| `image` | a picture the picture tab can open (PNG, JPEG, GIF, WebP, BMP, TIFF or ICO), fitted to the node; a double click shows it at its own size |
| `note` | a Markdown file, with the three view buttons in the node's header |
| `audio` | an MP3, WAV, FLAC, OGG, M4A or AAC file, with a play button, a seek bar, the time and a volume |
| `video` | an MP4, M4V, MOV or WebM file, played in the window's web view |

`realm add image <file>`, `realm add audio <file>` and `realm add video <file>` make one from a file in
the project, and so does dropping a file from the explorer on the canvas. Choosing `Image`, `Audio` or
`Video` in the canvas's add dialog opens a file chooser. A file from outside the project is copied into
`.realm-files/<realm>/` and the node names the copy, so the realm stays complete when it is shared. The
same picture added twice is one copy, and a different file with the same name gets a number after it.

**A note is a file, and the realm only names it.** `realm add note Plan` writes
`.realm-files/<realm>/Plan.md` and opens it in the node through the same editing area a tab uses, so
saving, undo, find and the preview all work as they do in a pane. The three buttons in the node's header
are `Raw Markdown`, `Side by side` and `Markdown preview`, and which one is showing is written in the
realm. Renaming the node renames the file through the explorer's own move, so anything linking to it is
rewritten.

**A sound plays in the window.** It is decoded and played by `rodio`, which is pure Rust on Windows and
on macOS and needs no SDK, so every sound node on a realm can play at once. Where a sound was paused is
remembered on this machine. A window a test builds plays through a silent player that keeps time, so the
tests hear nothing.

**A video plays in the window's one web view**, so it follows the browser node's rule: the chosen video
plays and the others draw a placeholder with a play button. The page is
`unluminous://realm/video/<node>`, and the file is served at `unluminous://realm/media/<node>` by node id,
so a page can load the one file its node names and nothing else, with a `Range` request answered 206 so
the video can seek. The page reports where it is by setting its own title, which the window reads.
`realm play`, `pause`, `seek` and `volume` drive a sound or a video the same way.

**A kind this Unluminous does not know is kept.** A node a newer build wrote is drawn as a placeholder
naming its kind, can be moved and resized, and is written back with every key it was read with.

**A File Editor node is the editing area rather than a second editor.** A tab records where it lives —
a pane, or a node — and the canvas borrows the focus exactly as the pane loop does, so the file
showing in a node is the active file while that node has the keyboard, and the gutter, the folds, the
breakpoints, git blame, `Ctrl+F`, go to definition, `editor text` and `tab save` all work with nothing
duplicated. A tab living on a node is in no pane, cannot leave one empty and is not renumbered into
one; moving the last tab out of a pane leaves a fresh untitled one behind.

**A chat node holds its own conversation**, beside the terminal sessions and the browser tabs, and
draws the same pane the panel draws — so the composer, the picture button, the drop, the paste, the
streaming, the history, the provider list and the tool blocks all arrive with no code of their own.
What differs is which conversation it is handed, and that is the whole of what the feature needs: two
views of one conversation are one agent that cannot say which node it is. Which conversation each node
is on is written down, because a canvas whose agents all reopened the newest would come back as
several views of one.

**A tasks node draws the window's one board**, and is deliberately not per-node: the board is one
SQLite file with one watchdog behind it, so a second instance would be a second connection to the same
tickets, each refreshing without the other. Two tasks nodes show the same board, which they should.

**One browser node renders at a time**, because a window has one native child view. The others draw
the toolbar and say the page is showing in another node.

## Connections

`realm connect <from> <to>` wires one node to another. `realm browser`, `realm folder`, `realm editor`
and `realm send` all take `--from`, and a node that is not wired to its target is **refused with the
list of what it is wired to**; a command with no `--from` is the window's own agent and may reach
everything.

Each terminal node's environment carries `UNLUMINOUS_REALM_NODE`, so an agent started in one knows
which node it is without being told, along with `UNLUMINOUS_CLI` and `UNLUMINOUS_INSTANCE` saying
where the command line is and which window it drives — it is on nobody's `PATH`.

A chat node has no client and no environment, so the node id is **filled into the call** before it
runs: `node` for `realm here`, which is the one command that asks *which node is calling*, and `from`
for every other one. It is read from the catalogue rather than from a list, because a key a command
does not name is a usage refusal and filling one in blindly would turn `realm list` into an error; and
a `from` the model named is left alone, so an agent asking about a node it is not wired to is refused
exactly as one typing at a terminal is.

**A pipe is off unless somebody turns it on.** `realm connect a b --pipe lines` types each line the
first node's program writes into the second node's terminal, and it is off by default because a
shell's output is its prompt and its escape sequences as well as its answers. **A line that arrived
through a pipe is never sent back out**, which is the one rule that stops two terminals wired both
ways looping for ever.

## Many realms

The realm bar along the top of the panel has a chip for each `.realm` file in the project and a plus
that makes a new one. The `View` menu and the `Realms...` dialog have `New Realm`, `Rename Realm...`, `Duplicate Realm` and `Delete Realm`, and each acts on the file:

- **New** writes `.realm-files/<name>.realm` and opens it.
- **Rename** moves the file through the explorer's own move, and moves its sidecar with it.
- **Duplicate** writes a copy with fresh ids, so two realms never share a node.
- **Delete** goes through the explorer's delete, so on Windows the file goes to the Recycle Bin.

Opening a `.realm` file from the explorer, from `Go to File` or with `tab open` shows it in a **realm
tab**: a tab in the editing area with the realm bar and the canvas drawn in it, which comes back with the
project. `realm open <name or path>` and the realm bar show it wherever the canvas is. There is one canvas,
so while a realm tab is showing, a panel that was already open says the realm is showing in a tab, and a
command or a button that asks for the canvas leaves a closed panel closed. Choosing another realm on
the bar inside a tab makes the tab that realm's. Switching realms writes the one that was open
first, stops its sounds and closes its pages. Which realm was open is `realm.current` in
`.unluminous/workspace.conf`, and that is the one a project opens on.

`realm list`, `open`, `new`, `rename`, `duplicate`, `delete`, `info` and `import` are the same
things from the command line.

## What is written down, and when

**The realm file holds what the realm is**: its nodes, their kinds, places, sizes, titles and files, its
connections, and its name. Paths in it are relative to the project and written with `/`, so the same
file reads the same on Windows and on macOS, and a path that leads outside the project is refused when
the file is read. Ids are eight random hexadecimal digits, so two people adding nodes to the same realm
on two branches do not collide when the branches are merged, and `z` says which node is drawn on top.

**The sidecar holds what one person was doing in it**: the camera, the chosen node, a terminal's
session, which file each editor was showing with its caret and scroll, a picture's zoom, and where a
sound or a video was paused. It is `.unluminous/realms/<path of the realm>.conf`, beside the rest of the
project's own state, so moving the camera never changes a file the repository holds.

Both are written **when they change and not on every frame**. Dragging a node writes once at the end
rather than sixty times a second, which is the one thing this deliberately does not copy from the rest
of the project's state. A realm whose text would come out the same as what is on the disk is not
written at all, so opening a realm and looking at it leaves `git status` clean.

**A realm is one format, and three numbers say who can read it.** `realm.format` is the format the
file is in, `realm.reader` is the oldest Unluminous that can read it, and `realm.writer` is the oldest
that can save it without losing anything. `realm.needs` names any feature a reader must have. An
Unluminous older than `reader` refuses the file and says why. One older than `writer`, or missing a
feature in `needs`, opens it for reading only, with a banner saying so, and never writes it. A key it
does not know, on a node, on a connection or on the realm, is kept and written back as it was.

**A project from before realm files is imported once.** When a project has a `.unluminous/space.conf`
and no realm files, each view in it becomes a realm file in `.realm-files/` named after the view, and the view that was showing
is the realm that opens. Nothing is deleted and nothing is overwritten: `space.conf` stays where it is,
and a realm file already at that path is left alone. `realm.imported` in `workspace.conf` says it has
happened, so a realm deleted afterwards does not come back. `realm import` runs it again by hand.

What comes back is a canvas rather than a moment: a terminal node returns as a fresh session running
the same command in the same folder, with the agent's session id added so a node can offer to resume
the conversation, and the screen it had on it printed back inside its own console.

**A frame is asked for by a pipe, never by a session that exists.** A terminal that prints wakes the
window itself; asking for a frame because a session is running would keep an idle window drawing for
as long as a shell sat at its prompt. What genuinely needs a frame is reading a pipe, which is a poll
on a clock.

## Dropping things on it

Settled after every panel and every node has been drawn, which is the earliest moment anything knows
where all of them are.

- A **tab** let go on the empty canvas breaks out into a File Editor node of its own.
- A **file** carried out of the explorer or out of a Folder node opens as a node, or as a new tab when
  it lands on a File Editor node that is already there.
- A drop the list itself claimed is a **move on disk** and is not offered twice.

The list a row was picked up in cannot know about a node it has never heard of, so the explorer
reports what is in the air and where the pointer is and decides nothing.

## What it will not do

**No screenshot Unluminous takes contains a rendered page.** A page is a native child window the
operating system composites on top of the surface the screenshot captures, so `window screenshot` has
never held one and `realm browser <node> shot` does not either — what it photographs is the node, its
address bar and where it is on the canvas. Measured rather than assumed: the same page in the editing
area's own browser tab, at full height, comes back as an empty rectangle while two dozen web view
processes are running. A picture that really held the page would be a capture of the **desktop**.

**A node cannot be dragged smaller than its contents can be drawn.** Each kind has a floor, and two of
them were too low: a board at 320 by 240 drew its rail, its sprint name and its Add Task button over
the whole node and put the first lane's heading on top of its own cards. The alternative is a node
that can be dragged to a size at which it draws something nobody can read.

**A page keeps its whole width.** Setting a native child's bounds sets its viewport as well as its
position, so a placement cut to the pane is what makes a responsive page relay out into what is left.
A placement carries two rectangles now: the whole node, which is what the page lays itself out
against, and the part of it inside the pane, which is all that may be painted. On Windows the crop is
a window region on the container the view is built inside; on macOS there is no container to mask and
the placement says so.
