# task-1906 — the canvas remembers, and two faults in the way of using it

Four reports about the Base of Infinite Space. Two are faults with a named cause, one is a control that
does not exist, and the fourth is the largest single piece of work the canvas has had since it was built:

1. *"When i right click a folder node to open a new folder root, the modal popup is way too the left
   bottom of the node. it should appear where i clicked."*
2. *"In folder view, I don't see the same icons i see next to the files i do in the main folder pane. e.g.
   rust icon for rust files isn't showing to the left."*
3. *"For infinite space, i need a space/view manager modal so i can open other saved spaces/tabs."*
4. *"If I close unluminous and open back up, my spaces should be in the same state. eg. terminal session
   should still have claude-code open with same session. e.g. same web page on the browser node, same
   folders expanded and scroll positions, same file tabs, etc."*

Every section says the same four things: what was reported, what is actually happening, what changes, and
what proves it. `CLAUDE.md`'s rule applies to each: the control a person uses, the same code reached by an
agent through the same path, and tests over both.

## 0. What was measured first

Read against 0.39.1 with `task-1905` committed. Three of the four have a cause that can be pointed at in
the code rather than guessed at, and the fourth is a list of fields — so this opens with the measurements
rather than summarising them later.

- **The menu is in the wrong coordinate space, not the wrong corner.** A node's header reports
  `response.interact_pointer_pos()`, and the header is drawn into the node's own `egui` layer, which
  carries a `TSTransform` with the camera in it — so what comes back is a **world** point.
  `components::context_menu::show` hands its position to `egui::Popup::new`, which wants a **screen**
  point. At the default camera the world origin is the pane's top left corner, so a node at world
  `(700, 460)` reports a position around `(700, 460)` and the menu opens at screen `(700, 460)` — down and
  left of a node that is drawn near the top of the canvas. That is §1, and it is one conversion.
- **A folder node passes no decorations at all.** `show_a_folder_node` builds
  `let decorate = |_: &Path| explorer::Decoration::default()`, so every row is drawn with no plugin icon
  and no git colour. The panel builds a real map — `plugin_icon` per row, `git.state_of` per row — and the
  node was written with the placeholder. That is §2, and the reason it was left is a borrow:
  `plugin_icon` takes `&mut self` and the node loop is already holding `self`.
- **Three node fields are written to `space.conf` and never filled in.** `Terminal::session`,
  `Editor::caret` and `Editor::scroll` all round trip through `store::write` and `store::read`, and the
  only thing in the repository that ever sets one of them is `store`'s own round trip test. So a canvas
  comes back with its nodes in the right places, its browser on the right address and its folders opened
  out — and its agent on a new conversation, and its file at the top with the caret at byte zero. That is
  §4's core, and it is why the report is about state rather than about a missing feature.
- **A space is a project's own file and nothing reaches another one.** `store::save` and `store::load`
  take the project root and write `.unluminous/space.conf` inside it. `Space::views` is a list a person
  walks with the chips in the view bar, and there is no list of *spaces* anywhere — nothing enumerates
  them, nothing opens one, and `space open-view` names a view inside the current file. That is §3, and it
  means the modal the report asks for needs somewhere to look before it can be drawn.

`cargo test --workspace` is green before any of this.

## 1. The node menu opens where the pointer is

**Reported.** Right clicking a folder node's header to reach `Choose Folder...` opens the menu down and
to the left of the node rather than under the pointer.

**What is actually happening.** Two coordinate spaces, and the wrong one crosses the seam.
`components::space`'s note at the top of the file states the rule this breaks in as many words: *a node's
contents are drawn in world points into an `egui` layer of the node's own carrying a `TSTransform` with
the camera in it; a node's decoration is recorded in screen points.* A pointer position read inside that
layer is therefore in world points — `egui`'s `Response::interact_pointer_pos` is in the layer's own
coordinates, which is the same reason `Response::drag_delta` comes back already divided by the layer's
scale, a fact that file already records about the drag.

`context_menu::show` builds an `egui::Popup` at the position it is given, and a popup is placed on the
layer the calling `Ui` belongs to — the window's own, not a node's. So the number is right and the space
is wrong, and at the default camera, where the pane's top left corner is world `(0, 0)`, the two differ by
exactly the node's own place on the canvas. A node near the top left of the canvas shows almost no error,
which is why this was not noticed while the canvas was being built with nodes at small coordinates.

**Three places report a position out of a node**, and all three are wrong in the same way: the header's
right click (the node menu), the port drag's free end, and the wire's own right click. The wire's is
already converted — `wires` is drawn in the pane's layer and works in screen points throughout — and the
port drag is already converted, in `show_the_ports`, by an explicit
`layer_transform_from_global`. So the header is the one that was missed, and that asymmetry is the reason:
two of the three were converted at the point they were read and the third was not.

**What changes.** The conversion goes where the other two are — at the point the position leaves the
node's layer — rather than at the point the menu is drawn. `NodeOutcome::menu` is documented as being in
**screen** points, and `show_the_header` converts:

```rust
if response.secondary_clicked() {
    // **Converted here, because this is where the position leaves the node's layer.** A `Response`'s
    // pointer position is in the layer's own coordinates, and a node's layer carries the camera — so what
    // is read here is a *world* point, and `egui::Popup` places a menu in *screen* points. `task-1906`.
    outcome.menu = response
        .interact_pointer_pos()
        .or_else(|| response.hover_pos())
        .and_then(|at| ui.ctx().layer_transform_to_global(ui.layer_id()).map(|out| out * at));
}
```

`layer_transform_to_global` rather than the camera, and that is deliberate: the transform is the one
`egui` is really compositing with, so a menu cannot drift from the drawing even if the camera and the
transform ever came apart. It is also the inverse of the call `show_the_ports` already makes, which is
what makes the pair legible.

**And `NodeOutcome`'s two position fields say which space they are in.** `menu` is screen and `wiring` is
world, they sit next to each other, and nothing said so — which is how this happened. Each gains a line.

**What proves it.**

- `a_nodes_menu_opens_where_the_pointer_is` — the arithmetic with no window: a node at a known place on a
  canvas at a known camera, a right click at a known point inside its header, and the reported position is
  the screen point rather than the world one. It fails on the code as it is, by exactly the node's own
  offset.
- `space_node_menu` — a screenshot of the menu open on a node that is **not** at the canvas's origin, which
  is the picture the report is about and the one a picture of a node at `(0, 0)` could never have shown.

## 2. A folder node's rows are decorated the way the panel's are

**Reported.** *"In folder view, I don't see the same icons i see next to the files i do in the main folder
pane. e.g. rust icon for rust files isn't showing to the left."*

**What is actually happening.** `show_a_folder_node` passes a placeholder:

```rust
let decorate = |_: &std::path::Path| crate::components::explorer::Decoration::default();
```

`Decoration` is the one thing the explorer cannot work out for itself — *"a component draws and does not
reach into the window's state, so it is handed one of these for each row rather than being given the
plugins and the repository to look in"* — and it holds exactly two things: the colour git wants a name in,
and the picture the file's plugin puts in front of it. A node was handed neither, so every row is a plain
name with the coloured square Unluminous draws itself.

`task-1904` asked for the node to have *"the exact same style and functionality"* as the panel, and this
is the one place it does not. The reason it was left is a borrow rather than an oversight:
`UnluminousApp::plugin_icon` takes `&mut self` — it caches the decoded texture in `services::icons` — and
the node loop is holding `self` for the whole of the frame.

**What changes.** The map is built before the loop, which is what the panel already does and for a reason
worth repeating: it used to search a list per row as the row was drawn, comparing paths, so *"a project
with four hundred rows open did a hundred and sixty thousand path comparisons every frame"*. One
`HashMap<PathBuf, Decoration>` a node, built from that node's own visible rows.

```rust
/// The icons and the git colours for one folder node's rows.
///
/// **Built before the node is drawn**, because `plugin_icon` takes `&mut self` to cache the texture it
/// decodes and the node loop is holding `self`. A map rather than a lookup per row, which is the panel's
/// own reason: searching a list as each row is drawn is a path comparison per row per frame.
///
/// Only the rows that are **showing** — `FileTree::rows` is what the explorer draws, so a folder nobody
/// has opened out costs nothing. `task-1906`.
fn decorations_for_a_folder_node(&mut self, ctx: &egui::Context, node: NodeId) -> Decorations { … }
```

Two things about it are decisions rather than details.

**It reads the node's own tree, not the project's.** A folder node can be rooted anywhere — `task-1905`
gave it `Choose Folder...` for exactly that — so the rows come from `Live::tree(node)` and the git colour
is asked of the repository the *window* has open, which answers `None` for a path outside it. That is the
honest answer rather than a second repository per node: `unluminous-git` runs the machine's real git on
one working tree, and a node pointed at somebody else's checkout is a node showing files git here knows
nothing about.

**And it is built for every folder node that is showing, not only the chosen one**, because the icons are
what the rows *look like* and a node nobody has clicked is still being read. The cost is one map per
visible folder node per frame, over the rows that are drawn — which is what the panel pays for one panel.

**What proves it.**

- `a_folder_nodes_rows_carry_the_same_icons_the_panel_draws` — a canvas with a folder node rooted at a
  fixture holding a `.rs` file, and the decoration handed to the component has a texture for that row. It
  fails on the code as it is, where every decoration is `default()`.
- `space_folder_node_icons` — a screenshot beside the panel showing the same file, which is the comparison
  the report makes.
- The git half is asserted rather than pictured, because a screenshot of a colour is a screenshot of a
  colour: `a_folder_nodes_rows_carry_gits_own_colour` builds a repository with an untracked file, and the
  decoration's tint is the one `git_colour` gives.

## 3. A space manager, and what a space is before there can be one

**Reported.** *"For infinite space, i need a space/view manager modal so i can open other saved
spaces/tabs."*

**What is actually happening, and the report asks for something that has nowhere to look yet.** There are
two words in it and they are two different things:

- A **view** already exists. `Space::views` is a list, the chips along the top of the canvas walk it, and
  `space new-view`, `open-view`, `rename-view`, `duplicate-view` and `delete-view` all drive it. What it
  has not got is a modal: with more than about six views the chips run out of room —
  `view_bar` breaks out of its loop when a chip would reach the zoom controls — and there is no way to see
  the rest at all.
- A **space** does not. `store::save(root, space)` writes `.unluminous/space.conf` **inside the project**,
  so a space is one file per project and nothing enumerates them. *"Other saved spaces"* has no meaning in
  this version: there is exactly one, wherever you are.

Both halves of the report are real and they need different answers, so §3 says which is which rather than
building one thing and calling it both.

### 3.1 The manager is a modal over the views, and it is the half that is asked for

`components::space::manager` is `components::modal` — the frame, the header, the dragging, the resizing and
the Enter that presses the last button, which is what every other dialog in Unluminous has. A tenth
modal drawing its own header would be a tenth modal that almost agrees with the other nine. Its body is a list of rows, one a view:

```
Spaces                                                    [ New Space ]
  ────────────────────────────────────────────────────────────────────
  Main                    4 nodes · 2 connections            showing
  Rendering               7 nodes · 5 connections
  Notes                   1 node
```

A row is one line, because what somebody is doing here is **finding** one — which is
`components::agent_tasks::listings`' own distinction between a board and a list, made again: a card is a
hundred points tall and carries buttons because a lane holds a dozen of them, and a row is one line
because a list holds hundreds. `Enter` opens the highlighted row, a double click opens it, and the
right click menu is `space_view_menu` — the same four entries the chip already has, so there is one menu
about a view rather than two.

**It has a search field**, for the reason the add modal's own comment gives: *"a list that gets long later
should not need a second design then."* A plain case-insensitive match on the name, which is what a list of
this size wants; `file_search`'s subsequence ranking is for two thousand files.

**And it is what the chips overflow into.** The bar keeps its chips — they are the quick way between two or
three views — and gains one row at the end when there are more than fit: `+3 more`, which opens the
manager. That is the answer to a strip that silently stopped listing things.

### 3.2 What "another saved space" means, and the smallest honest version of it

A space cannot be opened from another project without deciding what a space *is* when it is not the
project's own. Three answers were weighed.

**A folder of spaces in the person's own settings folder** — `~/.unluminous/spaces/<name>.conf`, listed by
walking it. It is what the report's words mean most directly, and it is refused: a space names a project's
files, its folders and the folder its terminals run in, so a space opened in another project is a canvas
whose every node points somewhere else. `project_state`'s own rule is the argument — the `.unluminous`
folder is *beside the project* so that copying the project copies its state, and *"two people on one folder
do not fight over one file"*.

**Every project's spaces, gathered by reading `recent.txt`** and loading each one's `space.conf`. It gives
the report exactly what it asks for and it costs a file read per recent project on the frame the modal
opens. It is refused for a sharper reason: opening one is opening *another project's* canvas in this
window, and the nodes would be wrong in the same way. What a person means by "open that space" is
open that project — which Unluminous already does, in a window of its own, because a project is a window.

**So the honest version is the one that is built**: the manager lists **this project's** views, and it
names the others rather than pretending to open them. Its footer has `Open Another Project...`, which is
`Action::OpenFolder` — the same native dialog, the same `launcher::open_window`, a window per project. The
modal's own header line says so in a sentence, because a control that silently did less than its name
suggests would be worse than one that says what it does:

> Every canvas in this project. A canvas belongs to the project it was made in, because its nodes name
> that project's files — `Open Another Project...` opens another project's canvases in a window of its own.

**Several views is what "saved spaces" is, then**, and the one thing genuinely missing was the way to see
them all. §3.1 is that. If a later ticket wants a canvas that outlives a project it needs a design of its
own, and §7 records what it would have to answer.

**The agent's half.** `space views` already lists them and `space open-view` already opens one, so the
manager needs one new command and it is the one that opens the modal: `space manage`. That is
`OpenAddModal`'s shape — a command whose whole job is to put a control in front of somebody — and it is
there so `action list` holds it and so a test can open the modal without a synthesised click.

## 4. The canvas comes back as it was

**Reported.** *"If I close unluminous and open back up, my spaces should be in the same state. eg.
terminal session should still have claude-code open with same session. e.g. same web page on the browser
node, same folders expanded and scroll positions, same file tabs, etc."*

**What is actually happening.** Two of those five already work and three do not, and the three that do not
are all the same fault: a field that is written to disk and never filled in from the live state.

| What the report asks for | Today |
|---|---|
| The same web page | **Works.** `Browser::url` is written on every navigation and `open_the_canvass_browsers` opens it. |
| The same folders expanded | **Works.** `Folder::expanded` is written by `remember_a_folder_nodes_open_folders`. |
| The same terminal session | **No.** `Terminal::session` round trips and nothing ever sets it. |
| The same scroll positions | **No.** `Editor::scroll` round trips and nothing ever sets it; a folder node's scroll is not in the file at all. |
| The same file tabs | **Partly.** One file a node — `Editor::path` — where `task-1905` gave a node several tabs. |

So the work is four things, and each is small on its own.

### 4.1 An agent comes back on the conversation it was on

**The id is one Unluminous chooses and gives, not one it reads back.** That is
`services::agent_tasks`' own answer to the same problem and the reason it works there: a first run is
`claude --session-id <uuid>` and a later one is `claude --resume <uuid>`, so the id in the ticket is one
Claude *answers to* rather than one Unluminous hopes to parse out of a stream. `agent::can_resume` records
the other half honestly — Codex names its own sessions, so an id here is only Unluminous's marker that a
worker was there, and starting it again begins a new conversation.

A terminal node runs a **command line a person typed**, though, which is the difference from a ticket. So:

- **A node whose command is an agent Unluminous knows how to resume gets a session id when it starts.**
  `space::launch` grows one question — is this command `claude` — and `Terminal::session` is filled in with
  a fresh uuid, which goes on the command line as `--session-id`. `agent_tasks::new_session_id` is the
  generator, moved to `services::agent_tasks` where both callers can reach it rather than copied.
- **Coming back, the same command is `--resume <id>`.** `start_a_space_terminal` already has a `resume`
  argument for exactly this — `Resume session` on the node's menu — and what changes is that a restored
  node uses it by itself rather than waiting to be asked.
- **A command that is not a resumable agent is untouched.** `zsh` gets no session id, because a shell has
  no conversation, and giving one a `--session-id` argument would be a shell refusing to start.

**And it is stated where a person can read it**, because this is the one part of §4 that cannot be
complete: a node's header already says what it is running, and the node's own menu keeps `Resume session`
for the case where a restored agent has to be started again by hand. The limitation `why_it_cannot_resume`
records for a ticket is the same limitation here, in the same words.

### 4.2 A node's scroll and caret are written down

`Editor::caret` and `Editor::scroll` exist and are read; what is missing is the frame that fills them in.
It is one function, called where the canvas is already asked to catch up:

```rust
/// Take down where each node's own file is being read, so it comes back there.
///
/// **Derived rather than reported**, which is `follow_the_open_file`'s rule and the reason this is one
/// function rather than a line at each of the places a caret can move: a list of the places that have to
/// remember to write it down is a list whose next entry is the one that forgets. `task-1906`.
fn note_where_the_nodes_are_reading(&mut self) { … }
```

Two rules in it. **It writes only when the number changed**, because `Space::change` marks the canvas dirty
and a canvas that wrote `space.conf` on every frame of a scroll would write it sixty times a second — which
is `Space::is_dirty`'s whole reason. And **a folder node's scroll joins the file**, as `folder.scroll`, for
the same reason the editor's does: `task-1905` gave a folder node a scroll and left it in `Live`, which is
right for a thing that dies with the window and wrong for a thing the report asks to come back.

### 4.3 A node's tabs all come back, not one of them

`Editor::path` is one path. A node holds several tabs since `task-1905`, so it becomes `Editor::paths`, a
list, with the one that was showing named — which is exactly the shape `open-files.txt` and `files.panes`
already have for the panes, and it is written the same way: one line, `|` separated, relative to the
project where the path is inside it.

`open_the_canvass_editors` opens each in turn and shows the one that was showing. The caret and the scroll
follow the file that was showing, because a caret per tab is `open-files.txt`'s job and a node is not where
that list should be duplicated — which is the one thing §4 deliberately does less than the panes do, and
§7 says so.

### 4.4 And the write happens before the window goes

`write_the_space_if_it_changed` is called at the end of a frame on which something changed, and
`on_exit` calls it with `f64::MAX` so a pending write cannot be lost. That is already right. What is added
is that the three new pieces of state are marked dirty when they change, which they are by going through
`Space::change`.

**What proves it.**

- `a_canvas_written_down_and_read_back_is_the_same_canvas` — the existing round trip test, extended with
  the session, the two scrolls, the caret and the list of paths. It fails on the code as it is for each.
- `an_agent_node_is_started_on_the_session_it_was_left_on` — the command line a restored node builds,
  asserted with no process: a fresh node gets `--session-id <uuid>`, and the same node after a restore gets
  `--resume <that uuid>`. This is the assertion §4.1 exists for.
- `a_shell_node_is_given_no_session` — the other half, so a `zsh` node cannot be handed an argument that
  would stop it starting.
- `a_nodes_scroll_and_caret_come_back` — through the harness: scroll a folder node and an editor node, save,
  load, and both are where they were.
- `every_tab_on_a_node_comes_back_with_the_one_that_was_showing` — three tabs, saved and restored.

### 4.5 What is derived is only written once there is something to derive it from

**The fault this whole section nearly shipped with, found by driving the installed build.** §4.2's function is
derived from the live state, which is the right shape — but before the nodes have been started that state is
**empty**, and a canvas is written on any frame it is marked dirty. So between a project opening and
`bring_the_current_view_to_life` running, `remember_a_nodes_tabs` wrote an empty list over the three paths the
file held, and the canvas came back with its editor nodes blank. Measured: `space.conf` held
`node.3.paths = Cargo.toml|CHANGELOG.md|README.md` before a restart and had no `paths` key at all after it.

So it is guarded by the same question the line above it already asks: `brought_to_life == current_id()`.
`brought_to_life` is the view whose nodes really are running, and reading a node's state before that is
reading nothing.

**And the harness could not catch it**, which is worth saying plainly rather than leaving as a gap. A test
drives frames on a window that was already restored, so it never sits in the window between opening a project
and the canvas coming alive — the exact frames this happens on. What the test asserts instead is the property
that *would* have caught it if it could: after a second window has restored the canvas, **the file on disk
still names the three tabs**. That is the assertion the fault was hiding behind, because the canvas in memory
looked right while the file had been emptied, and a third window would have opened nothing.

This is the fifth time in three tickets that the installed binary found something no reasonable test would
have, and `verify-before-saying-done` is the rule it keeps proving.

### 4.6 A copied view does not copy the conversation

`duplicate_view` clones a node's state under a fresh id, and §4.1 put a conversation id in that state — so a
duplicated view held two terminal nodes both resuming one conversation, which is two agents writing into one
thread. The copy's is cleared, which is the same reasoning that function already applies to the node's own id:
*"a copy is a second thing. Keeping the ids would make one node live on two canvases, and the live terminal
behind it would then be drawn twice and typed into twice."* A conversation is the same kind of thing.

The copy still runs the same program, because that is what was copied; what it does not do is join a
conversation somebody else's node is already on.

### 4.7 Two views naming one file, which is where a tab went missing

**Also found by driving the installed build**, and it is the one place §4.3 collides with a rule that predates
it. `OpenFiles::open` has always said that a file already open is *shown* rather than opened twice — two
`Document`s over one path would be two windows on one file, and whichever was saved second would win — so
`open_in_a_space_node` **moves** an already-open tab onto the node that asked for it.

A canvas with two views whose editor nodes name the same file therefore lost it: bringing the second view to
life took the tab off the node on the first, and switching back left that node with fewer tabs than it had.
Measured on the installed build — three paths on one node became two after switching away and back.

Two changes, and the second is the one that fixes it.

**A node's record keeps a file another node has open.** `remember_a_nodes_tabs` walks the live tabs, so a file
that had been taken away simply stopped being written down. It now keeps a path it was left holding when
another *node* has that file open — and drops one that was merely closed, because closing a tab is somebody
saying so. Both nodes name the file, which is honest: it is what each was left holding, and whichever view is
showing opens the ones it can.

**And a node does not take a file off another node.** `open_the_canvass_editors` skips a path that is already
living on a different node rather than moving it, so a view coming to life cannot empty a node on the view
being left. That one is belt and braces — the record keeping alone fixes the report — and it is worth having
because it stops the state and the screen disagreeing in the frames between.

### 4.8 A test that writes into a shared fixture, which this found the hard way

`restore_project` is what turns writing on — `remembers_this_project` gates the space, the marks and the
breakpoints on it — and `sample_folder()` is one folder behind a `OnceLock` that every test wanting a project
uses. So a test that calls `restore_project` on it leaves a `space.conf` in a fixture other tests **copy**:
`a_split_project_opens_split_again` copies `sample_folder` out of the way and then restores three editor node
tabs it had never opened, and its split does not come back because the three arrive in pane zero first.

That is `task-1654`'s third rule — *"a fixture two tests share is written once"* — and this is what breaking
it looks like from the other side: not a test reading a half-written file, but a test **writing** one that a
later test reads. So `everything_a_node_was_left_holding_comes_back` copies the fixture to a folder of its own,
which is what the split test already does and for the same reason, and says so where a reader will look.

**The rule to take from it:** a test that calls `restore_project` needs a folder of its own, always. It is
not enough for the test to be about the project state — every test after it in the same process inherits
whatever it wrote.

## 5. The look

Nothing new in the palette and one new drawing, which is the manager's own row. Every part of it is
`components::modal` and `components::controls`:

- The frame, the header, the body rectangle and the footer buttons are `modal`'s.
- The search field is `controls::search_field`, which is what the add modal and `Go to File` use.
- A row is `size::ROW` tall with the pill `explorer` and `go_to_file` already draw for the highlighted one,
  so a row means one thing in every list in the window.
- `New Space` and `Open Another Project...` are `modal::footer` buttons, so `Enter` presses the last one.

**And the modal is one size for every state**, which is the Settings window's rule: a list that grew the
dialog as views were added would be a dialog that jumped under the pointer. It opens at 520 by 460 — the
same shape `Go to File` opens at, because it is the same kind of thing — and the list scrolls.

## 6. Tests

Four layers, as everything here has.

1. **Unit tests with no window.** The menu's coordinate conversion; `store`'s round trip for the session,
   the two scrolls, the caret and the list of paths; the question `space::launch` asks about
   whether a command is an agent that can be resumed; the
   manager's own filter.
2. **`unluminous-app` unit tests.** The decoration map for a folder node; `space manage` in the catalogue
   and reachable from `action list`.
3. **Screenshot tests**, each through `builder()` and never `Harness::builder()`, for `task-1654`'s reason.
   Four new pictures: `space_manager`, `space_manager_filtered`, `space_folder_node_icons`, and
   `space_node_menu` retaken on a node away from the origin.
4. **The real window.** The layer §4 can only be answered at, and the layer the report is written from: a
   canvas with a `claude` terminal node, a browser node on a real page, a folder node scrolled and opened
   out, and an editor node with three tabs — then Unluminous closed and opened again, and every one of
   those checked by hand. `verify-before-saying-done` is the rule; a report that the state comes back,
   made against a working tree, is not a report about anything.

## 7. What is deliberately left out

- **A space that outlives its project.** §3.2 says why: a canvas names a project's files, so a space opened
  elsewhere is a canvas of broken nodes. What a later ticket would have to answer first is what a node's
  path *means* when the space is not the project's — and the answer may well be that it names a project
  as well, which is a second thing to keep in step.
- **A caret and a scroll per tab on a node.** §4.3: `open-files.txt` already holds one per tab for the
  panes, and a second list for the nodes would be a second thing to keep in step. What comes back is the
  file that was showing, at where it was.
- **Resuming Codex.** `agent::can_resume` is the existing answer and it is honest: Codex names its own
  sessions, so a restored Codex node starts a new conversation. Reading its id back out of its own output
  would be a parser over somebody else's stream, which is what `--session-id` exists to avoid.
- **A terminal node's scrollback.** What a program printed is gone with the program, which is
  `project_state`'s own promise about the terminal tile: what comes back is the same shell in the same
  folder, and now the same conversation — not the same screen.
- **A space manager that renames or deletes across projects.** The rows are this project's views, so the
  menu is `space_view_menu` and nothing more.

## 8. What the review found

Codex Sol read the whole change and raised sixteen items. Nine were real and each is fixed with a test that
fails without the fix; four were not, and the reason each is not is worth writing down, because three of them
are the same misreading and it is one a later reader would make again.

### 8.1 The nine that were real

- **A duplicated view shared one conversation.** `duplicate_view` clones a node's state under a fresh id, and
  §4.1 put a conversation id in that state — so a duplicated view held two terminal nodes both resuming one
  conversation, which is two agents writing into one thread. §4.6 is the fix and its reasoning.
- **A folder node's saved scroll reached nothing.** `Folder::scroll` was written to `space.conf` and read back
  out of it while nothing ever put the number where the drawing reads it, which is `Live::scrolls`. So the rows
  came back at the top — and then the first idle frame compared the saved 120 against the live 0, decided they
  had moved, and wrote the zero over the file. One restart lost the number and every later one had nothing left
  to lose. `scroll_the_canvass_folders` is the fourth call `bring_the_current_view_to_life` was missing.
- **A conversation id given was not always the id written down.** The command line asked what the node had
  recorded and the recording asked whether this was a resume, and those are different questions: `Resume
  session` on a node that has never run passes `resume` true with an empty session, so it was handed a fresh
  `--session-id` and recorded nothing. Claude then answered to an id the node had already forgotten.
  `launch::session_for` answers both halves from one value, so they cannot disagree.
- **A command that already named a conversation was given a second one.** A node's command is somebody's own
  words, so `claude --resume abc` is an ordinary thing to find there, and appending `--session-id <fresh>`
  hands Claude two conflicting instructions — after which the node has recorded an id that is not the one in
  use. Such a command is now left exactly as it is.
- **Opening a project rewrote `space.conf` having changed nothing.** Bringing a view to life opens each of a
  node's tabs in turn and every one of those calls `remember_a_nodes_tabs`, which compares the tabs open *so
  far* against the whole saved list — so the first path made that comparison say the list had changed, and
  `Space::change` marks the canvas dirty whatever the closure did. Two things fix it: the record is written
  again once the right tab is showing, and `bring_the_current_view_to_life` compares the canvas against what it
  found. `Space::holds_the_same_as` is that comparison, and it leaves out `dirty` and `View::chosen` — neither
  is in `space.conf`, so a canvas differing only in those is a canvas the file already describes.
- **The last thing done on a view was lost if the same frame switched away.** What
  `note_where_the_nodes_are_reading` writes down is derived from the live state and it only ever walks the view
  that is showing, so a wheel and a chip in one input frame left the wheel unrecorded. It is asked in
  `catch_the_space_up`, which is the one place that knows the view has changed — `show_view` is in `services`
  and cannot reach the live state, and it has seven callers, which is `follow_the_open_file`'s rule about a
  list whose next entry is the one that forgets.
- **A `|` in a filename was read as a separator.** Both lists a node writes — its tabs and its open folders —
  were one value with the paths joined by `|`, and `|` is legal in a Unix filename, so `a|b.rs` came back as
  two paths that do not exist and the node quietly lost a tab. They are numbered keys now, which is the shape
  `run-configurations.conf` already writes a list in and precisely so that no character in a value can be the
  separator. The `|` form is still *read*, so a `space.conf` already on somebody's disk opens unchanged.
- **Two conversation ids could be one id.** `new_session_id` varies only with the clock, and a nanosecond is
  not long enough: bringing a view to life asks for them in a loop. A ticket's agent was protected by the
  guarded claim that writes its id down, and a canvas node has no such guard, so two nodes would have shared a
  conversation in silence. A counter in the generator answers it for both callers.
- **A test helper described a command line nothing builds.** `space_terminal_settings` reused whatever id the
  node had recorded, where the real path asks for a fresh one on every start and only a resume reuses the
  recorded one. It takes the id from its caller now, so the caller says which run it is asking about — which is
  the thing the two runs differ on. `a_node_takes_a_session` went with it: a helper written for a test with no
  test using it is dead code.

### 8.2 The four that were not, and why

- **Three findings said an omitted key leaves a stale value in `space.conf`** — a `showing` that is not
  written when it is zero, a `scroll` that is not written when it is zero, and the older `path` key that the
  new writer never clears. All three read `write_a_node` without its caller: `store::save` builds a fresh
  `Values` and writes the whole file, so a key that is not written is a key that is not there. Measured rather
  than argued: a canvas saved with `showing = 1` and saved again with `showing = 0` produces a file with no
  `showing` key at all, and reads back as zero.
- **One said an out of range `showing` should open the first tab rather than the last.** Nothing here promises
  that; the code, its comment and its test all say the last one, which is also what the editing area does with
  an index past the end. Clamping is the decision and it is written down where it is made.
