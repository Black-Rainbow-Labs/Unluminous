# Base of Infinite Space

`task-1904` asks for a new view called **Base of Infinite Space**: an infinite canvas inside Unluminous
that holds nodes, connects them together, zooms and pans, and remembers where everything was. The
nodes are a terminal, a web browser, a folder tree and a file editor, and every one of them is the
thing Unluminous already has rather than a picture of it. The ticket's own words for the point of it:

> Our nodes are going to be agent-centric. Not only can terminal node agents control the things they
> are connected to, but the main agent for the IDE can control every single node, move it around, add
> new connections, ie any thing a human can do the main IDE agent should be able to do.

That sentence is this repository's own first rule said again, so nothing here is a special case: a
control a person uses, the same code reached by an agent, and tests over both.

The picture the canvas is measured against is Chordical's node graph, which the ticket attaches three
captures of. Chordical is white; this is the dark neumorphism Agent-Tasks is drawn in, through the
same `services::vello_canvas`.

---

## 1. Which repository this is, and what was pulled

The ticket opens with *"Pull down latest Inillucent main"*. Everything after that sentence — Vello,
Agent-Tasks, the file editor pane, the folder panel, the terminal, the browser tab — is Unluminous.
Inillucent is the database Unluminous's Database plugin reads through `inillucent-driver`. Both were
pulled: Unluminous to `485a697`, Inillucent already current. All of the work below is in the
Unluminous repository.

## 2. Where it lives, and why it is core

The ticket answers this itself:

> It should have panel and icon options, similar to a plugin, but since we'll have so many similar
> components, it probably makes sense to have this be core functionality.

It is core, and the reason is stronger than "there will be many of them". A plugin's `UiProvider`
draws into a rectangle and asks the window for things through `plugin_ui::Request`. It cannot reach
`OpenFiles`, it cannot hold a `Document`, and it cannot place the one native browser child the window
owns. Three of the four node kinds need exactly those. A File Editor node written inside a plugin
would be a second editor that agreed with the first one until the day it did not.

What it keeps from the plugin shape is everything the ticket asks for by name. `app::dock::Panel`
gains a fifth variant, `Panel::Space`, so the canvas is a panel with:

- a button in the rail, with a drawn icon, in the top group;
- `Move to` on its header's right click menu and on the rail button's, and the four blue drop bands,
  because those are `app::dock`'s and not each panel's;
- a divider that resizes it, remembered in `panes.space.width` and `panes.space.height`;
- `Cmd`/`Ctrl`+`Shift`+`M` to fill the window with it, because `app::Maximised` is the layout with
  everything else switched off and needs nothing here.

`settings::Panes` says of its own accessors that "a fifth panel would be a fifth arm here and nowhere
else", and `dock::Panel` says the compiler names every place that has to answer for a fifth. This is
that fifth, taken up as the code invited.

**It is not a tile.** `Panel::is_a_tile` is what decides that two character grids never share one
strip, and the Space is not one of those: it holds several grids inside itself deliberately, so it
sits beside the terminal tile rather than putting it away.

**The default is the bottom edge, 560 points tall, not showing until it is asked for.** A canvas
wants room, and the two ways to give it room are already there — drag it, or maximise it. Opening it
does not maximise it, because a control that rearranged the window on its first press is a control
nobody presses twice.

**The ground is the window's own.** `theme::faded(color::editor(), opacity)`, which is what the
explorer, the board and the editing area all paint, so the desktop shows through the canvas exactly as
much as it shows through everything else. This is the ticket's *"Background should be same
transparency like files, agent tasks."*

## 3. The model, which has no window in it

`services/space/model.rs` holds the whole of what a canvas is, in plain values with no `egui` in them
beyond `Pos2`, `Vec2` and `Rect`, which are `emath` and have no graphics card behind them. Everything
in this section is a unit test with no window, no fonts and no device, which is where the arithmetic
that has to be right lives.

```rust
pub struct Space { views: Vec<View>, current: ViewId, next_id: u64 }
pub struct View { id: ViewId, name: String, nodes: Vec<Node>, edges: Vec<Edge>, camera: Camera }
pub struct Camera { at: Pos2, zoom: f32 }
pub struct Node { id: NodeId, kind: Kind, title: String, at: Pos2, size: Vec2, state: State }
pub struct Edge { id: EdgeId, from: NodeId, to: NodeId, pipe: Pipe }
pub enum Kind { Terminal, Browser, Folder, Editor }
pub enum Pipe { Off, Lines }
```

Four rules the model keeps, each of which is a test.

**A node's place is in world points and nothing else knows where it is on the screen.** `Camera` is
the one thing that turns one into the other: `screen = area.min + (world - camera.at) * camera.zoom`,
and `world = camera.at + (screen - area.min) / camera.zoom`. Both are functions, both are inverses of
each other, and the test asserts that round tripping a point at every zoom on the ladder returns it.

**An id is never an index.** Nodes are deleted and views are cloned, and every index into `nodes`
would shift. `NodeId` is a `u64` from one counter per `Space`, which is what `OpenFiles::clock`
already is and what `unluminous_terminal::Tabs` is not.

**The camera is clamped in zoom and unclamped in place.** `MIN_ZOOM = 0.25` and `MAX_ZOOM = 2.5`,
which are Chordical's own numbers. There is no limit on where the canvas may be scrolled to, because
"infinite" is the name of the thing.

**An edge is directed and a pair of nodes may carry one in each direction.** `from` is the output port
on the right hand edge of a node and `to` is the input port on the left, which is what the ticket's
second capture shows. Two nodes wired both ways is how text goes *back and forth*, so a cycle is legal
and §6 is where the loop it could cause is answered.

## 4. The camera: panning and zooming, and where a wheel goes

The ticket asks that *"All zooming, canvas drags, should be buttery smooth."* Two decisions carry
that, and the second one is the whole of it.

**Nothing is laid out again when the zoom changes.** A node's contents are drawn at their own size
into an `egui` layer of their own, and the layer carries a `TSTransform` with the camera's scale and
translation in it. egui composites the transform, so a terminal keeps its cell count, an editor keeps
its line breaks, and a zoom costs a matrix rather than a relayout of everything on the canvas.
`Context::set_transform_layer` and `Context::set_sublayer` are what do it, and input comes back
through the same transform inverted, which is egui's own arrangement rather than arithmetic written
here.

The cost is stated rather than hidden: a layer's mesh is tessellated at its own scale and then scaled,
so text drawn at a zoom other than 1.0 is scaled pixels rather than re-rasterised glyphs. At 1.0,
which is the default and where somebody actually reads code, it is exact. The alternative — laying
every node out again at each zoom — is a relayout of a terminal, an editor and a file tree on every
notch of the wheel, which is the opposite of what the ticket asked for.

**Where a wheel goes is decided by what is under the pointer.**

| Gesture | What it does |
|---|---|
| Wheel over empty canvas | Zooms about the pointer |
| `Ctrl`/`Cmd` + wheel, anywhere | Zooms about the pointer |
| Wheel over a node's body | Goes to the node — a terminal scrolls its scrollback, an editor scrolls its file |
| Drag on empty canvas | Pans |
| Middle button drag, or `Space` held and drag | Pans, wherever the pointer is |
| Drag a node's header | Moves the node |
| Drag a node's edge or corner | Resizes the node |
| `Ctrl`/`Cmd` + `0` | Fits every node in the view |

The first two are Chordical's, which is what the ticket says to mirror, and they are also the answer
to a conflict Unluminous would otherwise have with itself: `task-1771` gives every pane `Ctrl` and the
wheel as its own zoom, and a pane that had both a pane zoom and a canvas zoom would have two numbers
meaning one thing. So `Panes::zoom_of(Panel::Space)` answers `1.0`, as it does for the three tiles,
and the canvas's zoom is the camera's — which is also the number the ticket asks to be remembered.

**The point under the pointer stays under the pointer**, which is `task-1672`'s rule. Given a pointer
at `p` and a new zoom `z'`, the camera moves to `world(p) - (p - area.min) / z'`. One subtraction,
tested against every zoom step.

**Zooming claims the gesture**, so the editing area behind the canvas does not also zoom. That is
`zoom_taken`, which `zoom_over_a_panel` already sets for every other panel.

## 5. A node

A node is a rounded rectangle with a header, a body, an output port on its right hand edge and an
input port on its left. The header carries the node's name, a kind mark, and the two buttons every
surface in Unluminous has: one that does the node's own thing, and a close. The body is the node kind,
and §8 is what each of them is.

**Resizing is from any edge or any side**, which the ticket asks for. `space::resize` is the
arithmetic: a rectangle, which of the eight grips was taken, the pointer's movement in world points,
and the kind's smallest size, giving a new rectangle. It is the same shape as
`components::resize_edges`, which resizes the window, and `modal::show`, which resizes a modal, and it
is a function with unit tests rather than eight branches written inline. Dragging the left edge moves
the left edge and leaves the right one where it is, which is the case that is wrong in every
implementation that stores a position and a size and forgets one of them.

**A grip is six points wide in *screen* points, not world points.** A canvas zoomed out to 0.25 would
otherwise have grips a point and a half wide, which nobody can hit. The rectangle is converted, the
grip is measured, and the delta is converted back.

**The smallest size is the kind's.** A terminal below about twenty columns is not a terminal and an
editor below about ten lines is not an editor, so each kind answers `Kind::smallest()` and the resize
clamps to it. Absent rather than dimmed does not apply here: the grip is still there, it simply stops.

**A node is drawn only when it intersects the pane.** `task-1666`'s rule, which the board already
keeps: a canvas with forty nodes on it draws the six that are showing. This also decides when a
terminal node's session is pumped — see §12.

## 6. Connections

**Making one.** Press the output port and drag. A curve follows the pointer, dashed while it is
looking for somewhere to land and solid when it is over a port that will take it, which is Chordical's
`ConnectionLine` and its `connectionStatus`. Let go over an input port and the edge is made; let go
anywhere else and nothing happens, because "a drag can be thought better of" is the promise the
explorer's row drag and the tab drag already make.

**The curve is a cubic Bézier with horizontal handles**, which is `getBezierPath`'s own shape: the
control points are offset along x by a distance that grows with the horizontal gap and is floored, so
two nodes stacked vertically still get a curve rather than a spike. `space::curve` is that arithmetic,
and it is a function with a test because a curve that inverts when the target is to the *left* of the
source is the visible fault every node editor has had at some point.

**An edge is drawn behind every node**, in its own layer under the nodes' layers, so a wire never
crosses a terminal's text.

**Right click an edge to delete it**, and an edge whose node has gone goes with it — `Space::tidy`,
asserted after every change, which is `OpenFiles::tidy`'s arrangement.

### 6.1 What a connection means

This is the half that is not drawing, and the ticket says what it is for in three places. A connection
does two things.

**It grants control.** An agent running in a terminal node may act on the nodes that terminal is
connected to, and on nothing else. That is the whole of the permission model and it is deliberately
simple: `space send`, `space browser`, `space folder` and `space editor` all take a `--from` node, and
a command whose `--from` node has no edge to its target is refused with a sentence naming what it is
connected to instead. The terminal's own environment carries `UNLUMINOUS_SPACE_NODE`, so an agent
started in a node knows which node it is without being told, and `unluminous-cli space connections`
answers what that node is wired to. That is the ticket's *"A connected terminal with an agent to a
browser should know it's connected to a browser"* and its *"When connected to a terminal with agent,
the agent should be able to expand, highlight files"*.

**It carries text, when it is asked to.** `Edge::pipe` is `Off` or `Lines`. With `Lines`, each
complete line the source's program writes is typed into the target's input. This is the ticket's *"If
I have 2 terminal nodes connected, they should be able to pipe text back and forth"*, and two things
about it were decided rather than assumed:

- **It is off unless somebody says so.** A shell's output is its prompt, its escape sequences and its
  echo as well as its answers, and a canvas that started shovelling all of that into another shell the
  moment two nodes were wired would be a canvas nobody wires anything on. `space connect --pipe lines`
  and the edge's right click menu are how it is turned on.
- **A line that arrived through a pipe is never sent back out.** Two nodes wired both ways is exactly
  what "back and forth" means, and a shell echoes what is typed into it, so without this rule one line
  would go round for ever. `space::pipe::forward(new_lines, recently_piped_in)` drops a line that
  matches one recently piped into that node and takes it off the list, which is precisely the echo and
  nothing else. It keeps the last 64, it is a pure function, and it has the loop as a test:
  `a_line_piped_between_two_terminals_wired_both_ways_does_not_go_round_for_ever`.

**A pipe into a browser, a folder or an editor is refused** when the edge is made, with the commands
that do apply named in the refusal. Typing a line of shell output into a file would be an edit nobody
asked for.

## 7. The right click modal

Right click the canvas and a modal opens: a search field with the keyboard already in it, and a list
of the node kinds under it. Type to narrow, up and down to walk, Enter to add, Escape to close. The
node is added at the world point that was right clicked, which is what makes the right click the way
somebody adds a node rather than a menu at the top.

It is `components::modal`, so it is the shape every other modal in Unluminous is — the frame, the
header, the body, the footer, dragging, resizing, and Enter pressing the last button. Four kinds is a
short list today; the search field is there because the ticket says *"we'll have so many similar
components"*, and a list that is long later should not need a second design then.

The same list is `View -> Base of Infinite Space -> Add Node`, and `unluminous-cli space add`.

## 8. The four node kinds

Each one is the window's own component, given a rectangle. None of them is a copy.

### 8.1 Terminal

`components::terminal_panel::grid` is what the terminal tile and the run tile already share, and it is
what a Terminal node draws. It takes a session, a rectangle, a font size and an id, which is exactly
what a node has. So a node's terminal has the emulator, the colours, the selection, the clipboard
rules, the mouse reports and the key encoding, because it *is* the terminal.

- **It starts the shell the person actually uses, in the project folder.** `unluminous_terminal::Session`
  and `Settings::shell()`, which is `task-1670`'s four rules, so a node's terminal is not a terminal
  that opens in `C:\Windows`. The ticket asks that *"we definitely need to verify claude and codex
  work"*, and what makes them work is this: the machine's own environment, the project folder, and
  `Ctrl+]` reaching the program, which `terminal_panel::symbol` already answers.
- **Its font size is its own.** The ticket asks to increase and decrease it per node.
  `Node::state.terminal.font_size` walks `settings::FONT_SIZES`, by the two buttons in the node's
  header and by `Ctrl`/`Cmd` and the wheel *over the node*, which is where §4's table sends that
  gesture. The terminal tile's `terminal.font.size` is untouched.
- **A node names its own program.** `space add terminal --command "claude"` starts that instead of the
  shell, which is `run_configurations::split_command`'s splitting rather than a shell's, so nothing
  globs and nothing expands.

### 8.2 Web Browser

`components::browser_view::show` draws Unluminous's own toolbar — back, forward, reload and the
address — and returns the rectangle the native child is placed in. A node hands it the node's body and
pushes the `BrowserPlacement` it gets back into `browser_placements`, which is the same list the
editing area's browser tabs go into and which `raw_input_hook` reconciles before the egui pass.

Three things follow from the window having exactly one native view, and each is stated rather than
discovered:

- **One browser node renders at a time.** The others draw the toolbar and say *"This page is showing
  in another node"*, which is the sentence `browser_view::show` already says when a second rendered
  tab sits in another pane. The one that renders is the one most recently pointed at.
- **The page is not scaled by the canvas's zoom.** A native child is a real window placed in screen
  coordinates, so the node's rectangle is converted through the camera and the page renders at its own
  resolution. It is the one thing on the canvas that stays sharp when everything else is scaled, and
  it is also the one thing that cannot be blurred.
- **A node scrolled off the canvas hides its view**, through the same path that hides it while a modal
  is open. A native child painted over a pane it is no longer inside is the visible fault here.

A connected terminal's agent drives it with `space browser <node> go|back|forward|reload|url`, and
`space browser <node> shot` writes a screenshot of the node's rectangle to a file and answers with the
path — the ticket's *"see screenshots"*. It is `window screenshot`'s own code given a rectangle rather
than a second capture path.

### 8.3 Folder View

`components::explorer::show` is the folder panel, and a node draws it. The component gains one
argument, `Host`, with two values: `Host::Panel` draws the heading strip that is also the dock handle
and the button that puts the panel away, and `Host::Node` draws neither, because a node is moved by its
own header and closed by its own button. Everything else — the filter box, the rows, the icons, the
disclosure marks, the right click menu, the drag that moves a file, the keyboard — is the same code, so
the ticket's *"Should be exact same style and functionality"* is true by construction rather than by
care.

**A folder node has a `FileTree` of its own.** `FileTree::new(root)` takes a root, so a node can be
pointed at a folder that is not the project's, and two folder nodes can be open on two folders with
their own expanded sets. The expanded folders are written down per node, which is what
`project_state` already does for the panel's.

**The keyboard.** The explorer's keys are read before any pane draws, and only while the explorer has
them. A folder node is the same: the keys are read while `Focus::Space` and the current node is a
folder, which is the one owner rule `Focus` already is.

### 8.4 File Editor

The hardest of the four, and the one that decides a change in `app::files`.

`UnluminousApp::show_editor` is four hundred lines that draw the gutter, the folds, the breakpoints,
the execution point, the inline values, the find bands, the caret, the selection, the colouring and the
scrollbar — for `self.files.active()`. The pane loop already draws it into several rectangles at once
by **borrowing the focus**: it sets `files.focus` to the pane it is about to draw, calls the same
function, and puts the focus back. A node is one more place a tab can be, so it is drawn the same way.

So `OpenFile::pane: usize` becomes `OpenFile::home: Home`:

```rust
pub enum Home { Pane(usize), Node(u64) }
```

and `OpenFiles::focus` becomes a `Home` as well. What that buys is that a File Editor node has
**every** capability the editing area has, with no second implementation: `editor text`, `editor
insert`, `tab save`, git blame, breakpoints, folding, `Ctrl+F`, go to definition, rename — all of
them, because all of them go through `files.active()` and `files.active()` answers with the node's
file while the node has the keyboard.

Four things this has to keep, each of which is an existing invariant said again for the new case:

- **`tidy` renumbers panes and counts empty ones over `Home::Pane` files only.** A node's tab is not in
  any pane, is not in any tab strip, and can never leave a pane empty.
- **`focused_pane()` still answers a number**, because two dozen callers want one. `OpenFiles` keeps the
  last pane the keyboard was in, set whenever `focus` becomes a `Pane`. It is a memory of a past value
  rather than a second opinion about the present one, which is the distinction `app::Maximised` already
  draws.
- **The pane loop restores what it borrowed, whatever that was.** `let had = files.focus()` and
  `files.restore_focus(had)`, rather than `focus_pane(keyboard)`, or a keyboard living at a node would
  be dragged back into the editing area on every frame.
- **A file already open is shown rather than opened twice.** `OpenFiles::open` has said this since it
  was written, and it is why `Split Right` moves a tab rather than copying it. The same answer here:
  opening in a node a file that is open in a pane **moves** it to the node, and the status bar says so.
  Two `Document`s over one path is the fault that rule exists to prevent, and a canvas is not a reason
  to make an exception.

## 9. Views: many of them, saved on edit, restored on open

> I should be able to have multiple projects/views that are saved on edit, and restored on project
> open. Positions of nodes, zoom level, state, terminal session, agent session, etc should all be
> retained and restored. Edit project/view names, delete, create new, clone/duplicate.

**A view is a named canvas and a project holds several.** A bar along the top of the pane carries the
views as a row of chips with the current one lit, a `+` that makes one, and a right click menu with
`Rename…`, `Duplicate` and `Delete` on it. The same four are `unluminous-cli space view new | rename |
duplicate | delete`, and the modal that asks for a name is `prompt_dialog`, which every other name in
Unluminous is typed into.

**Where it is written.** `.unluminous/space.conf` inside the project, in the `services::store` key and
value format the other three project files already use, numbered the way `run-configurations.conf`
numbers a list:

```
space.current                  = 1
space.view.0.id                = 1
space.view.0.name              = Main
space.view.0.camera.x          = -240
space.view.0.camera.y          = 120
space.view.0.camera.zoom       = 1.00
space.view.0.node.0.id         = 7
space.view.0.node.0.kind       = terminal
space.view.0.node.0.x          = 160
space.view.0.node.0.y          = 80
space.view.0.node.0.width      = 620
space.view.0.node.0.height     = 360
space.view.0.node.0.title      = claude
space.view.0.node.0.command    = claude
space.view.0.node.0.folder     = crates/unluminous-app
space.view.0.node.0.font       = 13
space.view.0.node.0.session    = 6f1c…
space.view.0.edge.0.from       = 7
space.view.0.edge.0.to         = 9
space.view.0.edge.0.pipe       = lines
```

Paths are written relative to the project wherever they are inside it, which is `project_state`'s own
rule so that a project that moves still opens what it was left with.

**Saved on edit** means written when something changed and not on every frame: a `dirty` flag set by
every mutation in `Space`, written at the end of the frame it was set on. The board writes its own
database the same way and `project_state` writes on every frame, which is the thing this deliberately
does not copy — a canvas being dragged would write a file sixty times a second.

**What "state … retained and restored" honestly means.** `project_state` already says it for the
terminals it restores: *"What a program was doing when the window closed cannot be brought back; what
is restored is the same number of shells in the project's folder."* This keeps that and adds what it
can:

- A terminal node comes back as a **fresh session running the same command in the same folder** with
  the same name and the same font size.
- When that command was an agent and Unluminous learned a **session id** from it, the id is written
  down and the node offers `Resume session`, which starts `claude --resume <id>`. That is
  `services::agent_tasks::agent`'s own arrangement and its own limitation: Claude takes a session id it
  is given and Codex names its own, so a Codex node is started again rather than resumed.
- A browser node comes back on the same address. A folder node comes back on the same root with the
  same folders open. An editor node comes back on the same file, at the same caret and the same scroll,
  which is what `open-files.txt` already restores for a pane.

**Only the released binary reads or writes it.** `restore_project` is called from `main.rs` and by
nothing else, so a screenshot test never writes a `.unluminous` into a sample folder. That rule is
`project_state`'s and is kept unchanged.

## 10. The agent's half

Everything above is a command, and the commands are rows in `unluminous-cli/src/catalogue.rs` with arms
in `app/cli.rs`, so the MCP tools are generated and the documentation test fails until
`unluminous-cli/docs/commands.md` has a section for each. There is no second list.

| Command | What it does |
|---|---|
| `space view list \| new \| rename \| duplicate \| delete \| show` | The views, and which one is current |
| `space add <kind>` | Add a node at a world point, or at the middle of what is showing |
| `space list` | Every node in the current view: id, kind, title, place, size, and what it is connected to |
| `space move <node> --x --y` | Move a node |
| `space size <node> --width --height` | Resize a node |
| `space title <node> <text>` | Rename a node |
| `space remove <node>` | Delete a node |
| `space connect <from> <to> [--pipe lines]` | Make an edge |
| `space disconnect <edge>` | Remove one |
| `space connections [--from <node>]` | What a node is wired to |
| `space camera [--x --y --zoom \| --fit]` | Pan and zoom, and fit everything in view |
| `space focus <node>` | Give a node the keyboard |
| `space send --from <node> --to <node> <text>` | Type text into a connected node's terminal |
| `space browser <node> go\|back\|forward\|reload\|url\|shot` | Drive a connected browser |
| `space folder <node> expand\|collapse\|select\|open\|root` | Drive a connected folder tree |
| `space editor <node> open` | Put a file in an editor node |
| `space view` (as `plugins view`) | The whole canvas as data |

**`--from` is what a connection is for.** A command given `--from` is acting *as* that node and is
refused when there is no edge from it to the node it names. A command given no `--from` is the window's
own agent, which may do anything — the ticket's *"the main agent for the IDE can control every single
node"*.

**Nothing here waits.** `space send` types and returns, and `space list` says what happened, which is
the shape `run start` and `run output` already have and the reason `plugins run agent-chat send` does
not block: a command that waited would be a command holding the frame.

**A scenario goes into the agent study.** `tools/agent-study/` drives a real window through a local
model and grades what happened against Unluminous's own state read back through `unluminous-cli`. The
repository's rule is that a feature nobody has watched an agent use is a feature nobody knows is
reachable, so this adds one: wire a terminal to a browser, have the agent in the terminal open a page
and read its address back.

## 11. The look

Dark neumorphism, through `services::vello_canvas`, which is what Agent-Tasks is drawn in. Core code
builds a `Chrome::recording()`, draws into it, and fills the slot in with `paint_the_chrome` — the same
three steps `show_the_plugin_panes` takes, and the same `Canvases` cache, so a still canvas costs
nothing and only a changed one rasterises.

What is drawn with it:

- **A node** is a `Chrome::raised` rectangle in `board_card` with `Lift::Small`, the header a shade
  above the body, and the whole thing clipped to its own corners.
- **The chosen node** carries a one point `ACCENT` ring, which is what every list in Unluminous draws
  for the row with the keyboard.
- **A port** is a `Chrome::disc` with a `Chrome::ring` round it, and it gains a `Chrome::glow` while a
  wire of the right kind is looking for somewhere to land, which is Chordical's pulsing socket.
- **The canvas ground** is the pane's ground with a dot grid over it, 20 world points apart, drawn only
  where it shows and faded out below zoom 0.5 so it does not turn into a grey wash.
- **The wire** is a `Chrome::line` polyline along the Bézier, because `Decor::Line` is what the
  renderer has and a cubic sampled at a few dozen points is indistinguishable from one at this size.
- **The view bar, the add modal and the node headers** are `components::controls` and
  `components::modal`, so they are the window's own controls rather than a second set.

The palette is closed and gains nothing. Everything above is `board_page`, `board_lane`, `board_card`,
`board_well`, `accent` and the text ladder, which are the names the board already uses.

**It can be switched off.** `plugins.chrome` is the setting that turns the decoration off for the
board; the canvas reads the same one, because a person who has turned depth off has turned it off.

## 12. What a frame costs

Three rules, each of which is `task-1666`'s or `task-1765`'s applied here.

- **Only nodes that intersect the pane are drawn**, and only their layers are given transforms. Forty
  nodes off screen cost a rectangle comparison each.
- **A terminal node is pumped when it is drawn, and when it is not drawn it is pumped once every frame
  it has output waiting**, which is `UiProvider::catch_up`'s bargain: a program that printed while its
  node was scrolled away must not lose what it printed, and a node nobody is looking at must not cost a
  relayout. `Session::pump` reads what is there and is cheap when there is nothing.
- **The decoration is recorded once a frame and rasterised only when it changed**, which is `Chrome`'s
  own comparison of the kept list against the new one. A drag moves a node, so a drag rasterises; a
  pointer moving across a node does not, because hover is a wash `egui` paints on top.

`cargo run --release -p unluminous-app --example space_cost` measures a canvas of a given size the way
`vello_cost` measures a board, and the numbers go in §12 of this document when they exist rather than
being promised here.

## 13. Tests

Four layers, as everything here has.

1. **`services::space` unit tests, with no window.** The camera's round trip, the eight resize grips,
   the Bézier's control points, the pipe's echo rule, `tidy` after a node is deleted, the view list's
   rename and duplicate, and the whole of `space.conf` written and read back.
2. **`unluminous-app` unit tests.** The fifth panel: `Panel::all` holds it, `Panes::width_of` and
   `height_of` answer for it, the settings file round trips `panes.space.*`, and the default layout is
   unchanged by its arrival — `the_default_layout_is_the_arithmetic_the_window_used_to_do_inline` has
   to still pass, which is what proves the other four panels did not move.
3. **Screenshot tests.** An empty canvas, a canvas with one node of each kind, a wire being dragged,
   the add modal, two nodes wired, the view bar with three views, and the canvas at 0.5 and at 2.0
   zoom. Each through `builder()` in the test file, never `Harness::builder()`, for the reason
   `task-1654` gives.
4. **The real window.** `cargo run --release`, a terminal node running `claude`, a browser node beside
   it, wired, and the agent in the terminal driving the browser. This is the layer that answers whether
   `claude` and `codex` really work in a node, which the ticket asks for by name and which no
   offscreen test can answer.

And the two the repository requires of every feature: a row in `unluminous-cli/docs/commands.md` for
each command, which `documentation.rs` fails without, and
`every_command_is_offered_as_a_tool_in_both_shapes`, which needs nothing because the tools are
generated.

## 14. What is deliberately left out

- **Grouping, marquee selection and saved node groups.** Chordical has all three; they are a second
  feature on top of a canvas that does not exist yet, and nothing in the ticket asks for them.
- **Undo on the canvas.** Moving a node is not an edit to a document, and an undo stack of its own here
  would be a second undo in a window that has one. Deleting a node asks first when the node owns a
  running program, which is the answer that costs nothing.
- **More than one input port and one output port a node.** The ticket's capture shows one of each and
  the four kinds have one thing to say and one thing to hear. A kind that later needs two ports adds
  them to `Kind`, and `Edge` already names a node rather than a side.
- **A minimap.** Worth having on a canvas with a hundred nodes on it; `space camera --fit` and the
  zoom controls answer the same question at this size.
- **Nodes for the other panels** — git, run, debug, the Agent-Tasks board, the Agent-Chat pane. The
  ticket names four kinds. `Kind` is an enum and the compiler names every place a fifth has to answer
  for, which is the same bargain `Panel` makes.
- **Sharing a canvas between two windows.** A second Unluminous window is a second process, and two of
  them writing one `space.conf` is a conflict nobody asked for. The file is written by the window that
  changed it, and a window that did not is not told.
