# The Base of Infinite Space

A fifth panel holding an infinite canvas, and six kinds of node that can live on it: a terminal, a web
page, a folder tree, a file editor, an agent chat and the task board. Nodes are wired to each other,
and **a connection is what lets an agent running in a terminal node act on the node it is wired to**.

`View -> Base of Infinite Space` opens it, `Cmd/Ctrl+Shift+M` maximises whatever panel has the
keyboard, and `unluminous-cli space` is twenty-four commands that drive the whole of it.
[What it looks like](overview.md#the-base-of-infinite-space) has a picture.

## Why it is core rather than a plugin

The ticket asked for it as a plugin and the answer was no, for a reason with three parts: a File
Editor node needs the window's open files, a Document and the editing area; a browser node needs the
one native child view the window owns; and an Agent-Tasks node needs the one board there is. None of
those is reachable from a plugin.

What it keeps from the plugin shape is everything a panel already buys. It is a fifth variant of the
window's own panel list, so the rail button, the four `Move to` rows, the drop bands, the divider and
the maximise chord all arrived with no code of their own.

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
glide. And `space camera --zoom` sets both at once, because a script that had to wait out an animation
to read back what it just set is a script with a race in it.

## The six kinds of node

| | What it is |
|---|---|
| `terminal` | a shell, or the program named by `--command`, in the project folder |
| `browser` | a rendered web page, local or remote |
| `folder` | a folder tree with its own filter box |
| `editor` | a file, with the gutter, the folds, the breakpoints, git blame, find, go to definition and everything else |
| `chat` | an Agent-Chat conversation of its own |
| `tasks` | the window's one Agent-Tasks board |

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

`space connect <from> <to>` wires one node to another. `space browser`, `space folder`, `space editor`
and `space send` all take `--from`, and a node that is not wired to its target is **refused with the
list of what it is wired to**; a command with no `--from` is the window's own agent and may reach
everything.

Each terminal node's environment carries `UNLUMINOUS_SPACE_NODE`, so an agent started in one knows
which node it is without being told, along with `UNLUMINOUS_CLI` and `UNLUMINOUS_INSTANCE` saying
where the command line is and which window it drives — it is on nobody's `PATH`.

A chat node has no client and no environment, so the node id is **filled into the call** before it
runs: `node` for `space here`, which is the one command that asks *which node is calling*, and `from`
for every other one. It is read from the catalogue rather than from a list, because a key a command
does not name is a usage refusal and filling one in blindly would turn `space list` into an error; and
a `from` the model named is left alone, so an agent asking about a node it is not wired to is refused
exactly as one typing at a terminal is.

**A pipe is off unless somebody turns it on.** `space connect a b --pipe lines` types each line the
first node's program writes into the second node's terminal, and it is off by default because a
shell's output is its prompt and its escape sequences as well as its answers. **A line that arrived
through a pipe is never sent back out**, which is the one rule that stops two terminals wired both
ways looping for ever.

## Views

A canvas has named views, and `space view`, `new-view`, `rename-view`, `duplicate-view` and
`delete-view` manage them. Which view is chosen is written down, and restoring puts the saved choice
back **after** each node's tabs have been opened — because opening a file into a node chooses that
node, so a canvas with a File Editor node used to come back with the keyboard there whatever it was
left on.

## What is written down, and when

`.unluminous/space.conf`, **when it changes and not on every frame**. Dragging a node writes once at
the end rather than sixty times a second, which is the one thing this deliberately does not copy from
the rest of the project's state.

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
never held one and `space browser <node> shot` does not either — what it photographs is the node, its
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
