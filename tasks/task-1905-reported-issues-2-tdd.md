# task-1905 — the second round of reported issues: what each one is, and what changes

`tasks/unluminous-issues-2.md` reports eleven things across the Base of Infinite Space, the window's
panel layout and the Agent-Chat pane. This document is the specification for all of them, written
before the code, because seven of the eleven are faults whose cause has to be named rather than gaps
to fill — and one of them is not what the report thinks it is.

Every section says the same four things: what was reported, what is actually happening, what changes,
and what proves it. `CLAUDE.md`'s rule applies to each: the control a person uses, the same code
reached by an agent through the same path, and tests over both.

## 0. What was measured first

The report is read against 0.39.1. Four of the eleven were reproduced by arithmetic against the real
code rather than by reasoning about it, and each of those four measurements is quoted in the section
it belongs to instead of being summarised here:

- **The add modal cuts its last row off.** `components::modal::body` gives a 380 by 360 modal a body
  from y=60 to y=300; the search field takes 28 and leaves the list starting at y=98; four rows of 60
  put the fourth row's bottom at y=334. `show_the_rows` breaks out of its loop when a row's bottom
  passes the body's, so **File Editor is never drawn**. That is §4 — the report guesses the kind is
  missing from the modal, and it is in the modal and off the bottom of it.
- **A panel on the left or the right is annihilated by one on the top or the bottom whenever the
  editing area is hidden.** Measured through `dock::regions_with` with `editor = false` on a 1150 by
  700 body: the Agent-Tasks pane on the bottom is handed all 700 points of height, `middle` becomes
  the gap between the two strips, which has no height at all, and the Explorer, Agent-Chat and Database panes come
  back 262 by **0**, 420 by **0** and 468 by **0**. That is §7.
- **A folder node paints over the rail.** `components::explorer::show` calls
  `Ui::set_clip_rect(list_rect)`, which **replaces** the clip rather than intersecting it, so a node's
  own clip — the one `components::space::clip_for_nodes` worked out — is thrown away for the whole of
  the row list. `components::terminal_panel::grid` intersects (`terminal_panel.rs:443`), which is why
  a terminal node does not do it. That is §3.
- **A browser node's toolbar never changes.** `UnluminousApp::change_browser_tab` and
  `browser_tab` both walk `self.files`, and a canvas node's tab is in `space.live.browsers()`, which
  is a different map. So `BrowserEvent::Title`, `LoadStarted` and `LoadFinished` are dropped for every
  node, `Back` and `Forward` can never find a history to step, and the address the node reports is the
  address it was given rather than the one it arrived at. That is §1.

`cargo test --workspace` is green before any of this. The screenshot images that already differ on this
machine are unchanged by it: every new picture in §12 is a new file rather than a change to an accepted
one, except the four this change deliberately alters, which are named there.

## 1. A browser node gets the toolbar it was always missing, and the events that were being dropped

**Reported.** *"I need a url address bar and back/forward and reload buttons at the top."* The capture
shows a Web Browser node with an empty body reading "Give this node an address to open." and no
toolbar at all.

**What is actually happening, and it is two faults rather than one.**

The first is the one the picture shows. `UnluminousApp::show_a_browser_node` returns early when the
node has no tab yet:

```rust
let Some(tab) = self.space.live.browser(node.id).cloned() else {
    // ... paints "Give this node an address to open." and returns
};
```

`components::browser_view::show` — which is where the Back, Forward and Reload buttons and the address
strip live — is therefore never reached until something has already given the node an address. And the
only ways to give it one are `space browser <node> go --url …` from the command line and `space add
browser --url …`. **There is no way to type an address into a browser node at all**, which is what the
report is about: the node it shows is not a node whose toolbar is missing, it is a node that has no
toolbar because it has no page, and no way to get one.

The second fault is in the toolbar itself and would have made it wrong even where it is drawn.
`browser_view::show` reads `tab.can_go_back()`, `tab.can_go_forward()`, `tab.loading` and
`tab.current_url()` — and nothing in the window ever updates any of those for a node's tab. Both
functions that change a `BrowserTab` walk the wrong list:

```rust
fn browser_tab(&self, id: u64) -> Option<&BrowserTab> {
    self.files.iter().filter_map(|file| file.browser.as_ref()).find(|tab| tab.id == id)
}

fn change_browser_tab(&mut self, id: u64, change: impl FnOnce(&mut BrowserTab)) {
    if let Some(tab) = self.files.iter_mut().filter_map(|file| file.browser.as_mut()).find(...) {
```

A node's tab lives in `space::live::Live::browsers`, a `HashMap<NodeId, BrowserTab>`, which neither of
them looks at. `Live::browser_mut` and `Live::node_of_browser` exist and **have no callers at all** —
they were written for this and never wired in. So on the canvas today: the title never arrives, the
address bar would say the address the node was *sent* to for ever rather than the one it *reached*,
`loading` is set once and never cleared, and `Back` and `Forward` answer "There is nowhere for this tab
to go that way" because the history has one entry in it however many pages were visited.

That second half is also the honest reading of the report's *"loading: true"* note in its own second
capture: `browser status` said loading because nothing had ever told the tab it had finished.

**What changes.**

**A browser node draws its toolbar whether or not it has a page.** `show_a_browser_node` stops
returning early. The toolbar is the node's own furniture, in the same way a terminal node's header is,
so it is drawn from the node's state; the body underneath it is where the "give this node an address"
sentence goes when there is no tab. That is the absent-control rule read correctly: an address bar on a
browser node is not a control that can never apply, it is the control that makes the node usable.

`browser_view::show` therefore takes what it needs rather than a `&BrowserTab`. A new value:

```rust
/// What the toolbar is drawn from: a tab when there is one, and the node's own address when there
/// is not.
pub struct Toolbar<'a> {
    /// The tab, when a page has been opened. `None` before one has.
    pub tab: Option<&'a BrowserTab>,
    /// What is in the address field. Owned by the caller, because it is being typed into.
    pub typed: &'a mut String,
}
```

with `can_go_back`, `can_go_forward`, `loading` and the address all answered from `tab` when there is
one and from nothing when there is not — so with no tab, Back, Forward and Reload are **dimmed** rather
than absent, because they are controls that will apply the moment a page is opened, and that is the
distinction `design/style-guide.md` already draws.

**The address strip becomes a field.** It is `controls::field_takes_the_whole_rectangle` plus an
`egui::TextEdit`, which is the one path all nineteen text boxes in Unluminous go through since
`task-1795`: a press anywhere in the field's own padding claims the keyboard, and it is handed over on
the **next** frame through `app::hold_the_keyboard`, because handing it over inside the press does
nothing. `Enter` in the field is `BrowserCommand::Go(address)`; `Escape` puts back what the tab really
says, which is what every address bar does. The field is given an explicit id derived from the tab or
the node, because an id from egui's auto counter shifts when the number of widgets above it changes —
the second latent fault `task-1795` records.

Where the caller keeps that typed text matters and there is only one right answer: **on the node**, in
`space::node::Browser`, as `typed: String`. Keeping it in `egui`'s memory would lose it when the node
scrolled off the canvas and stopped being drawn, which is exactly what `is_showing` does to a node
every time somebody pans. It is deliberately **not** written to `space.conf`: what a project remembers
is the address the node is *on*, which is `Browser::url`, and a half-typed address is not state a
project should come back with. `store::save` skips it, and there is a test that says so.

The editing area's own browser tab gets the same field, because it is the same function and the same
`Toolbar` — its `typed` lives on `OpenFile` beside the tab. One address bar, drawn in two places, which
is the whole reason `browser_view` is a component.

**And the events reach a node's tab.** `browser_tab` and `change_browser_tab` are the two places that
answer "which tab has this id", and they get one more place to look:

```rust
fn browser_tab(&self, id: u64) -> Option<&BrowserTab> {
    self.files
        .iter()
        .filter_map(|file| file.browser.as_ref())
        .find(|tab| tab.id == id)
        .or_else(|| self.space.live.browsers().find(|tab| tab.id == id))
}
```

and `change_browser_tab` the same, through `Live::browser_mut` and `node_of_browser`, which is what
those two functions were written for. **The two lists are searched in one function rather than at each
of the places that ask**, which is `follow_the_open_file`'s rule: a list of the places that have to
remember to look on the canvas as well is a list whose next entry is the one that forgets. It is the
same rule `raw_input_hook` already keeps for the placements — the canvas's tabs are reconciled in the
same list the editing area's are, and this is the reading half of that.

A tab id is unique across both, because `BrowserHost` hands them out from one counter, so "the first
list, then the second" cannot answer with the wrong tab.

**`space browser <node> url` gains a `title` field** in its reply, because it now has one to give, and
its summary says the address is where the page really is rather than where it was sent.

**What proves it.**

- `a_browser_node_draws_its_toolbar_before_it_has_a_page` — a screenshot, `space_browser_empty`, of a
  browser node with no address: the three buttons dimmed and the field empty with its hint showing.
- `typing_an_address_into_a_browser_node_opens_it` — through the harness: click the field, type, press
  Enter, and `space browser <node> url` answers with it.
- `a_page_that_finished_loading_says_so_on_a_node` — the half with no window:
  `change_browser_tab` applied to a node's tab through `receive_browser_events`' own three events, and
  `Live::browser` read back. This is the test that fails on the code as it is.
- `a_nodes_history_steps_back_and_forward` — two navigations, then `space browser <node> back`, and the
  address is the first one. It fails on the code as it is, with the refusal about nowhere to go.
- `the_address_a_node_is_typing_is_not_written_to_the_project` — `store::save` then `store::load`, and
  `typed` is empty.

## 2. An agent in a terminal node is told what it is wired to, rather than having to guess

**Reported.** *"When I connect a terminal then launch claude, and then ask it to go to a url, it
doesn't seem to know that a web node is connected to it."* The capture shows `claude` calling
`unluminous` nine times and running two shell commands to work out what the report calls its context,
and then answering with a paragraph of caveats. *"We need a good architecture for how this tool
calling should be wired up, so claude understands the context its operating in as a node."*

**What is actually happening.** Everything an agent needs is reachable and none of it is reached, which
is `CLAUDE.md`'s own distinction: *"Reachable is not the same as reached, and the second one is the
bar."*

Three things are true today, and the third is what the report is about.

- **The agent knows which node it is.** `start_a_space_terminal` puts `UNLUMINOUS_SPACE_NODE=<id>` in
  the session's environment.
- **The agent can find out what that node is wired to.** `space connections --from <id>` answers
  exactly that, and its summary says so in as many words: *"which is what an agent in a terminal node
  asks to find out what it may act on."*
- **Nothing ever tells it that either of those exists.** The MCP preamble is generated from the
  catalogue and is the same for every window and every conversation. It contains a `space` area
  description that opens *"Read `space view --json` first"* — advice for the window's own agent, which
  is a different reader with different permissions. An agent started in a node has no `CLAUDE.md`
  section about nodes, no `AGENTS.md` one, and nothing in its environment that reads as an instruction.
  `UNLUMINOUS_SPACE_NODE` is a number in `env` that nothing suggests looking at.

So the agent did what the report shows: it reasoned from first principles, spent nine calls and two
shell commands establishing what one call would have said, and hedged its answer because it never
established what it was allowed to do.

**What changes, and the shape of it is the point.** Three additions, and every one of them is data the
catalogue or the environment already holds, said where the reader is already looking. Nothing here is a
second list.

**First: one command that answers the whole question.** `space here` — no arguments, no flags.

```
$ unluminous-cli space here
node 7  terminal  "claude"  on view Main
wired to:
  9   browser   Web Browser        space browser 9 go --url <address> --from 7
  11  folder    unluminous         space folder 11 rows --from 7
wired from:
  nothing
```

`--json` gives the same as fields. It reads `UNLUMINOUS_SPACE_NODE` from **its own environment**, which
is the one thing `unluminous-cli` can do that no other command in the catalogue does, and it is why
this is a command rather than a paragraph: the answer depends on which process is asking.

The value of it is not that it saves eight calls, though it does. It is that **the answer names the
command for each thing it lists**, which is what `task-1695` found decides whether a command is used at
all: an agent handed a node id and left to work out which of twenty-three `space` verbs applies to a
browser reaches for `bash`. An agent handed the command line writes it.

`space here` with no `UNLUMINOUS_SPACE_NODE` set is **not** a failure. It answers that this process is
not running in a node and says what that means — every node is reachable, no `--from` is needed —
because the window's own agent will run it too, and a refusal there would be a refusal about nothing.
That is `picture::from_the_clipboard`'s rule: the absence is the ordinary case, so it is an answer
rather than an error.

**Second: the environment says there is something to read.** Beside `UNLUMINOUS_SPACE_NODE`, a terminal
node's session gets `UNLUMINOUS_SPACE_HINT`, whose value is a sentence rather than a number:

```
This terminal is node 7 on an Unluminous canvas. Run `"$UNLUMINOUS_CLI" --instance
$UNLUMINOUS_INSTANCE space here` to see which nodes it is wired to and how to drive them.
```

**And it names the two variables rather than a bare command, because driving the real window found that
the bare command does not work.** `unluminous-cli space here` typed into a node answered `zsh: command not
found: unluminous-cli` — it is on nobody's `PATH`, since on macOS it lives inside the application bundle
beside `unluminous` and on Windows in the installation folder. So the very command §2 exists to tell an
agent to run first could not be run.

The Agent-Tasks board had already met this and already had the answer: `agent::ENV_CLI` and
`ENV_INSTANCE`, filled in by `agent_tasks::beside_this_program`, which its own comment introduces with
*"`unluminous-cli` is not on anybody's `PATH` … so an agent told to run `unluminous-cli` would answer that
there is no such command."* A terminal node's session gets the same two, so there is one answer to this
rather than two. **Nothing was invented for it** — which is the shape the whole of §2 is: what was
missing was never a mechanism, it was that nobody had told the agent the mechanism existed.

That is also the fourth thing this ticket only learned from the installed binary rather than from a test,
and §11 records it beside the other six.

A variable holding an English sentence is unusual and the reason is measured rather than stylistic: an
agent that runs `env` — which `claude` does, and which the capture shows it doing — reads values, and a
number tells it nothing it can act on. It is the cheapest possible place to put a pointer, it costs the
child nothing, and it is read by any agent whatever its own tooling. `UNLUMINOUS_SPACE_NODE` stays
exactly as it is, because a program that wants the id wants the id.

**Third: `space` says what a node agent is, in the catalogue.** The area description already exists and
is already handed to every agent in the grouped preamble. It gains two sentences, and they go at the
**front**, because that is what gets read:

> If `UNLUMINOUS_SPACE_NODE` is set in your environment you are running inside a node on this canvas,
> and `space here` is the first thing to run: it says which node you are, which nodes you are wired to,
> and the command that drives each of them. You may act on the nodes you are wired to and no others, so
> every command you send carries `--from <your node>`.

That is the whole architecture the report asks for. It is deliberately not a new permission model, a
new protocol or a session: `a_reachable_node` already enforces the wiring, `--from` already says who is
asking, and the refusal already names what the asking node *is* wired to. What was missing was that
nobody had told the agent any of it existed.

**Two things that were considered and are not being done**, because each would break a rule this
repository keeps.

- **A per-node MCP server, or a per-node tool set.** It would mean the tools an agent is handed depend
  on which node started it, which means the grouped preamble is no longer generated from the catalogue
  alone — and `mcp::tools`' whole argument is that a second source of truth is the fault to avoid. It
  would also cost a server per node.
- **Injecting `--from` on the agent's behalf**, by having the window read `UNLUMINOUS_SPACE_NODE` off
  the calling process. The window cannot: a command arrives down the control channel from
  `unluminous-cli`, which is a *different* process from the agent, and the id would have to travel with
  the request anyway. Having `unluminous-cli` add `--from` itself when the variable is set was weighed
  and refused for a sharper reason: it would make the same command line mean two different things
  depending on where it was typed, and an agent that then read the refusal *"node 7 is not connected to
  node 9"* would have no idea where the 7 came from.

**What proves it.**

- `space_here_names_every_node_it_is_wired_to_and_the_command_for_each` — the whole reply, against a
  canvas with a terminal wired to a browser and a folder.
- `space_here_outside_a_node_says_so_rather_than_refusing` — with the variable unset.
- `a_node_agents_environment_points_at_the_command_that_orients_it` — `start_a_space_terminal` builds
  its `SessionSettings`, and both variables are in it. A unit test with no process.
- `the_space_area_tells_an_agent_in_a_node_what_it_is` — the catalogue's own description contains
  `UNLUMINOUS_SPACE_NODE` and `space here`, so the preamble cannot lose them silently.
- `documentation.rs` fails until `space here` has a section in `unluminous-cli/docs/commands.md`, which
  is the test that already exists.

## 3. A folder node scrolls, picks its own root, and stays inside the canvas

**Reported.** Four things about the Folder View node.

1. *"I can't scroll the node."*
2. *"I need to be able to click the top bar and see an option to pick the source/base folder, so i can
   have multiple nodes with different root folders."*
3. *"If I double click a file, it should open a file view node and connect it, if one isn't already
   open, or open a new tab in the connected file view node."*
4. *"Folder view node is going over the top of the left bar with icons, but terminal node isn't. all
   should be behind the left bar and not cover it."*

The fourth and the first have the same shape — both are a component reaching outside what the node gave
it — and the answer to each is one line. The third depends on §4, so it is specified there and only
named here.

### 3.1 It cannot scroll, and the reason is a clip rectangle rather than a scroll rectangle

`components::explorer::show` puts its rows in an `egui::ScrollArea`, and a `ScrollArea` decides whether
the wheel is its own by asking `Ui::rect_contains_pointer`, which is `Context::rect_contains_pointer`,
which ends:

```rust
if self.layer_id_at(pointer_pos) != Some(layer_id) {
    return false;
}
```

`Areas::layer_id_at` walks the layer order and, for each layer, reads `self.areas.get(&layer.id)` — the
`AreaState` a layer registers when it is drawn. A node's contents are drawn into a layer made by hand:

```rust
let layer = egui::LayerId::new(egui::Order::Background, egui::Id::new(("space-node-layer", node.id)));
ui.ctx().set_sublayer(parent, layer);
ui.ctx().set_transform_layer(layer, to_global);
```

`set_sublayer` puts the layer in the **order** list and nothing puts it in the **areas** map, because
only `egui::Area` does that and `set_state` is `pub(crate)`. So `layer_id_at` skips every node layer and
answers with the pane behind them, `rect_contains_pointer` is false for every rectangle in every node,
and **no `ScrollArea` inside a node can ever take the wheel**.

Ordinary widgets in a node are unaffected, which is why clicking a row works and is why this was not
noticed: `hit_test` reads `WidgetRects` and the layer transform map, neither of which needs an
`AreaState`. Only the two functions that ask "is the pointer over this layer" are broken, and a
`ScrollArea`'s wheel is the one thing in Unluminous that asks.

**What changes: the window reads the wheel and the explorer is told where to scroll.** The mechanism is
already there and already used — `explorer::View::scroll_to` sets `vertical_scroll_offset` and
`ExplorerOutcome::scroll` reads the offset back, which is how `keep_the_place_through_a_panels_zoom`
puts the explorer panel back after a zoom. A folder node keeps its offset in `space::live::Live`, beside
its tree and its cursor, and `show_a_folder_node` passes it in and takes it back out.

The wheel itself is read in `show_a_folder_node`, over the node's body, from the frame's
`smooth_scroll_delta` — the same reading `take_the_canvas_input` already does for the camera. Two rules
about it, and each is a thing that would otherwise be wrong:

- **The delta is in screen points and the offset is in the node's own points**, so it is divided by the
  camera's zoom. A canvas at 0.5 would otherwise scroll a folder node twice as far as the pointer moved.
- **A wheel the node takes is taken out of the frame**, by clearing `smooth_scroll_delta`, which is what
  `ScrollArea` itself does at `scroll_area.rs:1243`. Without it the canvas would pan or zoom at the same
  time as the node scrolled, which is one gesture doing two things.

`Live::scroll_of` and `Live::scroll_to` are the pair, and the offset is deliberately **not** written to
`space.conf`: `Folder::expanded` is what a project comes back with, and a scroll position into a tree
whose folders may have been opened or shut since means nothing. That is the same line
`project_state` draws about a terminal node — what comes back is the shells, not what they were doing.

**The general fault is named where it belongs.** A comment at the top of `app::space` says that a node
layer registers no `AreaState`, that `rect_contains_pointer` and `hovered_layer`-shaped questions are
therefore false inside one, and that anything put in a node which needs the wheel has to be driven the
way this is. That is cheaper and more honest than registering a fake `AreaState` per node, which would
put every node in `layer_id_at`'s answer and change what the canvas behind them thinks the pointer is
over.

### 3.2 It draws over the rail, and one call is the whole of it

`components::resize_edges` records the rule this breaks: a node's contents are clipped clear of the
window's own furniture by `components::space::clip_for_nodes`, which is handed to the node's `Ui` as
`node_ui.set_clip_rect(clip)`. Inside the explorer, one line throws it away:

```rust
let mut list = ui.new_child(egui::UiBuilder::new().max_rect(list_rect));
list.set_clip_rect(list_rect);
```

`Ui::set_clip_rect` **replaces** — `self.painter.set_clip_rect(clip_rect)` is an assignment. So for the
whole of the row list the clip becomes `list_rect`, which is inside the node's body but is measured in
the canvas's own points and reaches wherever the node has been dragged to. Everything else in the
explorer paints through `ui.painter_at(...)`, which is `Painter::with_clip_rect` and **intersects**, so
the heading, the filter box and the footer are all clipped correctly and only the rows escape. That is
exactly what the report describes, and it is why a terminal node does not do it:
`components::terminal_panel::grid` already writes the intersecting form —

```rust
painter_ui.set_clip_rect(ui.painter().clip_rect().intersect(area));
```

**What changes.** The explorer's line becomes that one. Nothing else in this section changes, and the
panel is unaffected because there the incoming clip is the panel's rectangle already.

**Every other `set_clip_rect` in the window is checked and named.** There are three:
`explorer.rs:399`, `terminal_panel.rs:443` and `editor_view.rs:941`, and the third is in a test with no
node behind it. `a_component_never_widens_the_clip_it_was_given` is the test that keeps that true — it
draws each of the three components into a `Ui` whose clip is deliberately smaller than the rectangle it
is given, and fails if anything is painted outside it. It fails on the code as it is.

### 3.3 The header picks the folder

**A node's header is where the things about that node are**, which is `actions::tab_menu`'s rule and
what `space_node_menu` already is. So the folder is picked from there rather than from a control drawn
inside the tree: a Folder View node's right click menu gains `Choose Folder...` above the rows it
already has, and the menu is **absent** on every other kind, which is the same absence that keeps
`Restart` off a browser node.

`Choose Folder...` opens `rfd::FileDialog::pick_folder`, which is the same native dialog
`Action::OpenFolder` and the Database plugin's own file picker already use — it blocks inside a frame,
which is what a native file dialog is on every platform. It starts at the folder the node is already
showing, which on a node whose root has been deleted is that path's parent rather than nowhere.

What it then does is `space folder <node> root --path <folder>`, which already exists and already does
the right thing: it writes the root into the node's state, clears `expanded`, and **forgets** the live
tree so it is built again from the node's own state on the next frame. So the pointer and the command
line reach the same function, which is the rule, and no new state is invented.

Two things follow.

- **The node's fallback name follows its root**, which `name_of_a_node` already does — it answers with
  the root's own file name. So a canvas with three folder nodes on three roots has three different
  headers with no renaming, which is what makes "multiple nodes with different root folders" legible.
- **A root outside the project is allowed.** The ticket asks for several roots and says nothing about
  where; `FileTree` takes any folder, `space folder root` already accepts an absolute path, and refusing
  one would be a refusal about nothing. What is **not** allowed is a file: `pick_folder` cannot return
  one, and the command answers that a folder is wanted.

### 3.4 What proves it

- `a_folder_node_scrolls_with_the_wheel` — the harness turns the wheel over a folder node holding more
  rows than it is tall, and `Live::scroll_of` has moved. It fails on the code as it is, because the
  offset is zero however far the wheel is turned.
- `a_wheel_a_folder_node_took_does_not_also_move_the_camera` — the same gesture, and the camera is where
  it was.
- `the_scroll_delta_is_read_in_the_nodes_own_points` — a unit test on the arithmetic at zoom 0.5 and 2.0,
  with no window.
- `a_component_never_widens_the_clip_it_was_given` — §3.2 above.
- `space_folder_node_over_the_rail` — a screenshot with a folder node dragged so it would cover the rail:
  the rows stop at the rail's edge. This is the picture the report is about.
- `choosing_a_folder_for_a_node_goes_through_the_command_the_agent_uses` — the action with a folder
  supplied, and `Live::tree` is rebuilt on the new root with `expanded` empty.
- `the_choose_folder_row_is_absent_on_a_node_that_is_not_a_folder` — `space_node_menu` for all four kinds.

## 4. A File Editor node is the editing area, with tabs

**Reported.** *"This should be just like our editing area, where I can see and edit files in multiple
tabs, have line breaks, expand code, etc. I think we already have this, but it's just not showing in the
add node modal?"*

**What is actually happening.** The report's guess is wrong, and where it is wrong matters. The kind
is in the modal. `add_modal::matching("")` returns `Kind::ALL`, all four of them, and
`the_filter_matches_a_name_or_the_line_under_it` asserts it. What happens is that the fourth row is
drawn off the bottom of the modal and the loop that draws the rows gives up:

```rust
if row.bottom() > area.bottom() {
    break;
}
```

The arithmetic, which is §0's first measurement: `modal::body` on a 380 by 360 modal is y=60 to y=300;
the search field takes 28, leaving the list from y=98; `ROW` is 60, so the fourth row is y=278 to y=334,
and 334 is past 300. **`Kind::Editor` has never been drawn in that modal.** The accepted screenshot
`space_add_modal` shows three rows and a large empty space under them, which is what a person reads as
"there are only three kinds".

The other half of the report is right and is a separate change: an editor node holds one file, not
several. `OpenFiles::tab_in_node` is `position`, the first tab whose home is that node, and
`open_in_a_space_node` **closes** whatever was there before opening the next one, which its own comment
says outright: *"a node shows one file, so the one that was there is closed"*. Everything else the report
asks for — line numbers, folding, find, breakpoints, git blame — is already true, because a node's tab
is an ordinary tab and `show_editor` draws it unchanged. That is `OpenFile::home`'s whole purpose.

**What changes.**

**The modal is as tall as its rows.** `HEIGHT` goes from 360 to 424, which is the four rows plus the
field plus the frame — worked out from `modal::HEADER`, `modal::FOOTER` and `ROW` rather than typed in,
so a fifth kind does not silently fall off the bottom again:

```rust
/// Tall enough for every kind, worked out rather than chosen.
///
/// `task-1905`: at 360 the fourth row's bottom fell six points past the body and `show_the_rows` broke
/// out of its loop, so `Kind::Editor` was never drawn and the modal read as a list of three. A number
/// typed here is a number the next kind added would break again.
const HEIGHT: f32 = modal::HEADER + 14.0 + FIELD + 10.0 + ROW * Kind::ALL.len() as f32 + modal::FOOTER + 8.0;
```

and `every_kind_is_drawn_in_the_add_modal` is the test: it asserts that the last kind's row bottom is
inside `modal::body`'s rectangle, in arithmetic, with no window. It fails on the code as it is.

**A file editor node holds a strip of tabs.** Three changes and no new concepts.

- `OpenFiles::tab_in_node` answers **which tab is showing** rather than the first one, which is
  `showing_at(Home::Node(node))` — a function that already exists and already means exactly this, and
  which is how `showing_in` answers the same question about a pane. `OpenFiles::tabs_in_node` is the
  list, beside `tabs_in_pane`.
- `open_in_a_space_node` stops closing what was there. It opens the file, moves its tab onto the node
  and stamps it, which makes it the one showing — `move_to_node` already stamps.
- `show_an_editor_node` draws `components::file_tabs::show` across the top of its body and the editing
  area under it, which is what `show_pane` does for a pane. The strip is the same component, so
  dragging a tab along it, the close cross, the middle-click and the unsaved dot all arrive with no code:
  `file_tabs` reports and decides nothing.

**A tab dragged out of a node is settled where every other tab drag is settled.** `settle_the_tab_drag`
runs after every pane has been drawn, because a tab picked up in one pane is dropped in another as often
as not. A node is a third kind of place a tab can be dropped, so the node strips join the list of strips
that function reads — `node_tab_strips`, each with the node's own rectangle beside it — and
`drag_tab_to_node` is `drag_tab`'s twin. That is the one place the rule lives, so a tab dragged from a node
into a pane and one dragged the other way are the same code.

Three details fell out of it, and each was a thing that would otherwise have been wrong.

**A node is asked about before the panes.** A canvas docked to the bottom is inside the body the panes were
laid out in, so a point inside a node is very often inside a pane as well — and the node is what the
pointer is actually over.

**The pointer is converted.** A node's strip reports in the node's own world points, because that is the
layer it was drawn in, and the drag is settled in screen points against every strip in the window. It is
converted where the camera is to hand rather than at the far end.

**And `drag_tab` had a fault this found.** It read the carried tab's origin as `pane_of`, which answers `0`
for a tab living on a node — so dragging the first tab out of a node into pane zero took the "dropped where
it already is" branch and nothing moved. It reads `home.pane()` now, which is `None` for a node, and
`a_tab_is_dragged_between_a_node_and_a_pane` is the test that found it.

**What closing the last tab in a node does** is the question `close` already answers for a pane, and the
answer is the same: nothing. A node with no tab draws "Give this node a file to edit", which is what it
draws before one has been opened, and `move_to_node`'s promise that the editing area always keeps a tab
is untouched.

**And a double click in a folder node opens a file in a wired editor node**, which is §3's third ask.
`act_on_a_folder_node` reads `outcome.open_permanently` — the double click — and, instead of opening into
the editing area, asks the model which editor nodes this folder node is wired to:

- **One or more**, and the file opens in the first of them as a tab, through `open_in_a_space_node`.
- **None**, and an editor node is made beside this one and wired to it, and the file opens there. Made
  *beside* means to the right of the folder node's own rectangle with a gap, so the wire is visible
  rather than crossing the node that drew it.

A **single** click still opens into the editing area, unchanged, because a single click in the explorer
is a way of looking through a folder and the editing area is where a preview belongs. That is the same
split the panel keeps between `open` and `open_permanently`, and it is why the report asks for a double
click rather than a click.

`space folder <node> open --path <file>` gains the same behaviour, so the agent's half is the pointer's
half: it opens into a wired editor node when there is one and into the editing area when there is not,
and its summary says which.

**What proves it.**

- `every_kind_is_drawn_in_the_add_modal` — the arithmetic above.
- `space_add_modal` — the accepted screenshot is retaken, and this is one of the four in §12 that
  deliberately changes. It shows four rows.
- `an_editor_node_holds_more_than_one_tab` — two `space editor` commands at one node, and both tabs are
  still open with the second showing.
- `space_editor_node_tabs` — a screenshot of an editor node with three tabs and the second one chosen.
- `closing_the_last_tab_in_a_node_leaves_the_node_asking_for_a_file` — and the editing area still has
  its own tab.
- `a_double_click_in_a_folder_node_opens_a_wired_editor_node` — and, separately,
  `a_double_click_with_no_editor_node_makes_one_and_wires_it`, which asserts the new node's kind, that
  the edge is there, and that the file is in it.
- `a_single_click_in_a_folder_node_still_opens_into_the_editing_area` — the other half, so the change
  cannot quietly take the preview away.

## 5. The zoom modifier over a node zooms that node, and the wheel alone still moves the canvas

**Reported.** *"If I CMD/CTRL mouse wheel while hovering over a node, that node should zoom in/out,
rather than the entire canvas."*

**What is actually happening.** Both gestures reach the camera and nothing reaches a node.

`take_the_canvas_input` reads the **plain** wheel over the body and zooms the camera with it, which is
Chordical's own feel and is what the canvas means by a wheel — that stays. `zoom_over_a_panel` reads the
**modifier** gesture through `zoom_steps`, which is `egui`'s `zoom_delta` and is what `Ctrl`/`Cmd` with
the wheel becomes, and its `Panel::Space` arm zooms the camera as well:

```rust
dock::Panel::Space => {
    let body = self.space.body;
    let about = body.center();
    self.space.space.current_mut().camera.zoom_by(steps, body.min, about);
```

So the two gestures do the same thing, and the modifier one does it about the middle of the pane rather
than about the pointer. There is no per-node zoom at all.

**What changes, and the interesting part is what "zoom a node" means.** It is not one thing, because the
four kinds have four different numbers that decide how big their contents are, and `task-1771` already
settled the principle: *where a pane already has a point size a person chooses, the zoom walks that
setting*, because one number saying how big the text is beats a setting and a multiplier that can
disagree. Applied to the four kinds:

| Kind | What one step walks | Why |
|---|---|---|
| Terminal | the node's own `Terminal::font_size` | It already exists, `space font` already walks it, and the two buttons on the node's header already press it. |
| Editor | the node's own font size, new | An editor node is an ordinary tab, and the editor's font is one setting **for the window** — so a node cannot walk that one without changing every other tab. See below. |
| Folder | a multiplier over everything it draws | The explorer has no point size of its own; this is `settings::ZOOMS` and `explorer::View::zoom`, which the panel already uses. |
| Browser | the page's own zoom | A page is a native child view, and `wry::WebView::zoom` is what changes how big a page is. |

**A terminal node** needs nothing new: `step_a_node_font` is the function, and it is what the header's
two buttons and `space font --bigger` already call.

**An editor node gets a font size of its own**, `Editor::font_size`, `0.0` meaning "follow
`appearance.font.size`" — which is the same convention `Terminal::font_size` already uses and the same
one a terminal tab's name uses for "call it after its program". That is a real addition and the reason
for it is that the alternative is worse: `set_the_font_everywhere` exists because the editor's font is
one setting for the whole window, so a node walking it would resize every other tab, which is the exact
fault `task-1657` fixed in the other direction. A node's own size is applied by giving that tab's
document a base style of its own for the frame it is drawn in — the same borrow `show_an_editor_node`
already does with the focus, and put back the same way.

**A folder node** keeps a `zoom` in its own state and passes it as `explorer::View::zoom`, which every
measurement in that component already goes through as `view.at(...)`. So this costs one field and one
line.

**A browser node** is the one that is not Unluminous's own drawing at all, and that is stated rather than
worked around: how big a page is drawn is the page's business, and `wry` has `WebView::zoom(f64)` for
it. `services::browser` gains `zoom(tab, factor)`, the node keeps the factor, and it is applied when the
view is pointed at that tab — because a window has one native view and a node that is not rendering has
no view to set. A node’s zoom therefore has to be applied again when the view moves to it, which
`reconcile` is the place for.

**Where the gesture is claimed.** In `show_a_node_body`'s caller, over the node's own rectangle, before
`zoom_over_a_panel` is reached — which is the ordering `zoom_taken` already enforces:
`show_the_space_nodes` runs inside `show_the_space`, and `zoom_over_a_panel(Panel::Space)` is the last
line of it. So a node under the pointer takes the gesture and sets `zoom_taken`, and the canvas gets it
only when no node did. That is the pointer rule the whole zoom story is built on, one level further in,
and it means the canvas's own modifier zoom keeps working over the empty ground.

`zoom_steps` is called **at most once a frame** — its own note says so, because calling it twice spends
the same notch twice — so the node loop asks which node the pointer is in first and calls it once for
that node, rather than calling it per node.

**And the plain wheel over a node does nothing to the canvas.** That is not new but it is now load
bearing: a folder node takes the plain wheel to scroll (§3.1), and a terminal node's grid already takes
it for its own scrollback. What is left is that the plain wheel over a browser or an editor node passes
through to the camera, which is what it does today.

**The command line half.** `space zoom <node> [--factor <n> | --bigger | --smaller | --reset]`, which
answers the factor the node is drawn at and which of the four numbers above it really walked — the shape
`space font` already has, including the honesty about `itsOwn`. `space font` stays as it is, because it
is about a terminal's point size specifically and `space zoom` is about a node whatever kind it is;
`space zoom` on a terminal node walks the same number, so the two cannot disagree.

**What proves it.**

- `the_modifier_wheel_over_a_node_zooms_the_node_and_not_the_camera` — the harness, over a terminal node:
  the node's font size moved and `camera.zoom` did not. It fails on the code as it is, where the opposite
  is true of both.
- `the_modifier_wheel_over_the_empty_canvas_still_zooms_the_camera` — the other half, so the change
  cannot quietly take the canvas's zoom away.
- `one_notch_is_spent_once_however_many_nodes_are_showing` — four nodes on the canvas, one notch, and the
  chosen node moved one step. This is the `zoom_the_text` fault `task-1672` records, which a per-node
  zoom would reintroduce.
- `each_kind_of_node_zooms_the_number_that_decides_its_size` — a unit test walking all four, asserting
  which field moved and that the window's own `appearance.font.size` and `terminal.font.size` did not.
- `space_editor_node_zoomed` — a screenshot of an editor node at a larger font beside one at the default,
  which is the picture that shows the window's setting was not touched.
- `a_browser_nodes_zoom_is_applied_when_the_view_is_pointed_at_it` — the reconcile half, with no window.

## 6. The zoom controls are a plus and a minus in circles

**Reported.** *"The icons for zoom in/out at the top right are not good. should be classic - + buttons
with cirlces around them."*

**What is actually happening.** There are no zoom controls on the canvas at all. `space_view::view_bar`
draws one chip a view and a `New view` plus at its right hand end, and the `+` the report is looking at
is that plus — which is why pressing it made a view rather than zooming. The zoom is reachable by the
wheel, by `Cmd`+`Shift`+`M`'s neighbours on the `View` menu and by `space camera`, and by no button.

So this is a control being **added** rather than an icon being redrawn, and the report's description of
what it should look like is the specification.

**What changes.** Two buttons at the right hand end of the view bar, before the `New view` plus, and a
reading of the zoom between them:

```
[ Main ] [ Rendering ]                                  ⊖  100%  ⊕     +
```

- `theme::icon::zoom_out` and `theme::icon::zoom_in` are the two new drawn marks: a circle with a
  horizontal stroke through it, and a circle with a cross. **Drawn rather than lettered**, which is
  `design/style-guide.md`'s rule and the reason the eleven box-drawing characters in a Markdown table are
  painted — a glyph's ink is an em box and does not centre in a control. They are the only two new
  drawings, and they are `icon::plus` and a new minus stroke inside `Painter::circle_stroke`.
- The reading between them is the camera's zoom as a whole percentage, and it is a **button**: pressing
  it is `space camera --zoom 1`, which is the `Reset Font Size` gesture every other zoom in Unluminous
  has. It says `100%` at 1.0 and the ends of the ladder are `25%` and `250%`, which are `MIN_ZOOM` and
  `MAX_ZOOM`.
- Both buttons **dim at the ends of the ladder** rather than disappearing, because a zoom that cannot go
  further is a control that will apply again the moment the other one is pressed. That is the dimmed
  half of the absent-control rule, the same reading `browser_view`'s Back button gets in §1.
- They step by one notch, which is `Camera::zoom_by(±1, ...)` about the **middle of the pane**, because a
  button has no pointer — which is the same choice `step_the_zoom_of` already makes for the keys.

Each has a name, which is the rule: `Zoom out`, `Zoom in`, `Reset zoom`. The reading's name carries the
number, so a test can read the zoom back out of the accessibility tree rather than out of a picture.

**Where they go, and why the view bar rather than a corner of the canvas.** The report says "top right",
and the top right of the canvas *is* the right hand end of the view bar — the bar spans the pane. A
floating control over the ground would be a control drawn above the node layers, which are sublayers
composited above the pane, so it would be underneath every node that happened to be there. The bar is
also where the chips already stop when they run out of room (`view_bar` breaks when a chip would reach
within 30 points of the right edge), and that reserve becomes the room these three take.

**What proves it.**

- `space_zoom_controls` — a new screenshot: the bar with two views, the two buttons and `100%`.
- `space_zoom_controls_at_the_end_of_the_ladder` — at `MIN_ZOOM`, with `Zoom out` dimmed. The picture that
  shows the dimming rather than an absence.
- `pressing_zoom_in_steps_the_camera_one_notch` — the harness finds the button by name and the camera
  moved by the same factor `Camera::zoom_by(1, ..)` gives.
- `pressing_the_reading_puts_the_zoom_back_to_one` —
- `the_zoom_buttons_go_through_the_same_camera_the_command_line_does` — one step by hand and
  `space camera --zoom` to the same number, and the two cameras are equal.
- `every_named_icon_is_actually_drawn` already exists and covers the two new marks, which is the test
  `task-1777` added after `PANE_ICONS` and `activity_bar::pane_icon` were found to be two lists.

## 7. A panel on one edge no longer annihilates a panel on another when the editing area is hidden

**Reported.** *"The panels for database explorer, agent tasks, and agent chat, don't show when editing
area is toggled off. We need each toggle to be independent so I can view any arrangement I want."* The
capture shows the Base of Infinite Space filling the window with a terminal node in it, the rail's
buttons for the other three panes lit, and none of those three panes on the screen.

**What is actually happening, and it is arithmetic rather than a missing branch.** Measured through
`dock::regions_with` on a 1150 by 700 body, with `editor = false`, the Explorer on the left, the
Agent-Tasks pane on the bottom and the Agent-Chat and Database panes on the right:

| panel | side | rectangle | drawn |
|---|---|---|---|
| Agent-Tasks | bottom | 830 x 700 | yes |
| Space | bottom | 320 x 700 | yes |
| Explorer | left | 262 x **0** | no |
| Agent-Chat | right | 420 x **0** | no |
| Database | right | 468 x **0** | no |

The chain has five links and every one of them is doing what it was written to do.

`regions_with` splits the height with `fill_the_depth` when the editing area is hidden, and
`fill_the_depth`'s whole job is to give the room to the panels rather than leave a gap — so it returns
two numbers that **always sum to the full height** whenever either strip has a panel on it. `middle` is
then defined as the band between the two strips:

```rust
let middle = Rect::from_min_max(
    Pos2::new(body.left(), top_strip.bottom()),
    Pos2::new(body.right(), bottom_strip.top()),
);
```

which therefore has no height at all. `left_region` and `right_region` are cut out of `middle`, so they have no height
either, and `lay_columns_out` guards only the **width**:

```rust
if order.is_empty() || region.width() <= 0.0 {
    return;
}
```

so it writes rectangles that are hundreds of points wide and nothing tall. Those are not `Rect::ZERO`,
which is this module's own word for "not there" — they are **degenerate**, so they survive as far as the
drawing, where `show_the_plugin_panes` skips a pane whose rectangle is under a point in either direction
and the pane silently does not appear.

**Why it shipped.** The two tests that cover the hidden editing area each show exactly one panel:
`hiding_the_editing_area_gives_the_whole_width_to_the_panels` shows the Explorer alone, and
`hiding_the_editing_area_gives_a_strip_the_whole_height` shows the Terminal alone. Neither has a strip
panel and a column panel showing at the same time, which is the only arrangement that breaks. The rule
in one sentence: **with the editing area hidden, one panel on the top or the bottom annihilates every
panel on the left and the right.**

**What changes. The height is split three ways rather than two.**

`fill_the_depth` is right when there is nothing between the strips and wrong when there is. So
`regions_with` works out which panels are on the left and the right **before** it decides how deep the
strips are — they are currently worked out afterwards — and asks a different question in each case:

```rust
// **The columns are a band of their own, and the strips cannot have their height.** With the editing
// area hidden the strips were given the whole of it, because there was thought to be nothing between
// them; a panel on the left or the right is exactly that something, and `middle` — the gap between
// the two strips — came out with no height, so every column was laid out into nothing and silently
// skipped. `task-1905`.
let columns_want_a_band = !visible(Side::Left).is_empty() || !visible(Side::Right).is_empty();
let (top_depth, bottom_depth) = match (editor, columns_want_a_band) {
    // Something is between the strips, so they keep to what they asked for and it gets the rest.
    (true, _) | (false, true) => {
        share_the_depth(body.height(), depth(&top_panels), depth(&bottom_panels), keep_height)
    }
    // Nothing is, so they fill the height between them rather than leaving a gap — `task-28`.
    (false, false) => fill_the_depth(body.height(), depth(&top_panels), depth(&bottom_panels)),
};
```

`keep_height` is `0.0` when the editing area is hidden, which is what it already is, and that is now the
one thing that has to change with it: what is kept in the middle is no longer being kept **for the
editing area**, it is being kept for whatever is there — so with no editing area and columns present it
becomes `COLUMN_BAND_MIN`, a new constant beside `EDITOR_MIN_HEIGHT`:

```rust
/// The least the band between the two strips may be squeezed to when the editing area is hidden and
/// something is docked to the left or the right.
///
/// [`EDITOR_MIN_HEIGHT`] is what is kept **for the editing area**, so with no editing area it is zero —
/// which was right while there was nothing else in the middle. A column is something else in the
/// middle. The number is the same 120, because what it is protecting is the same thing: enough of a
/// panel to be worth drawing.
pub const COLUMN_BAND_MIN: f32 = EDITOR_MIN_HEIGHT;
```

Two things follow and both are wanted. The horizontal axis needs no change: `fill_the_depth` over
`middle.width()` gives the whole width to whichever of left and right has panels, which is correct
because with the editing area hidden there is genuinely nothing between them — and a comment says so, so
the asymmetry is a decision rather than an oversight.

**And `lay_columns_out` stops writing degenerate rectangles.** Its guard reads both dimensions:

```rust
if order.is_empty() || region.width() <= 0.0 || region.height() <= 0.0 {
    return;
}
```

so a band with no height leaves `Rect::ZERO`, which is what every reader of a rectangle in this module
already treats as "not there". That is the invariant the module's own comment claims and which had
quietly stopped being true; with it, a regression here is an absence somebody can see rather than a
silent skip. `lay_a_strip_out` already guards its own height.

**Every question about where a panel is gets the same answer.** Four callers work out a layout with
`dock::regions`, which hardcodes `editor = true`, so each of them answers about a window that is not on
the screen while the editing area is hidden:

- `UnluminousApp::panel_area`, which is what a pseudoconsole is opened at the size of — so a run started
  with the editing area hidden was opened at the wrong size, which is the fault `task-1684` measured
  losing a program's first line.
- `dock::zones` and `dock::position_in`, which are where a dragged panel would land and where in that
  side — so the four blue bands and the drop were both computed against the wrong layout, which breaks
  `task-1697`'s promise that *the highlight is the layout rather than a picture of it*.
- `show_the_drop_zones`' preview, for the same reason.

All four take whether the editing area is showing. `dock::regions` keeps its two arguments for the
tests that use it, and its own comment says it means "with the editing area showing"; every caller in the
window passes `self.editor_visible`.

**What this does not change, and it is a product decision rather than an omission.**
`put_the_other_tiles_away` still applies: showing a pane that declares `pane.group = bottom` puts away
the other tiles **on the same side**, because two character grids stacked in one strip are two grids half the
size. That is `task-1683`'s rule and `task-1697` already narrowed it from "the bottom of the window
holds one" to "one per side". The Agent-Tasks pane is in that group, so it and the terminal, run and
debug tiles still take turns along the bottom. The report's *"each toggle independent"* is satisfied for
every panel that is not competing for one strip, which is the Explorer, the Agent-Chat pane, the Database
pane and the canvas — the three the report names by name and the one its capture shows. Moving the
Agent-Tasks pane to another edge is what makes it independent of the terminal, and `Move to` on its
header already does that.

**What proves it.** The two configurations no existing test covers:

- `a_panel_on_a_strip_does_not_annihilate_one_on_a_column_when_the_editing_area_is_hidden` — the
  measurement in the table above, made a test: with `editor = false`, one panel on the bottom strip, one left
  column and two right columns showing, every one of the four comes back with a height over a point, and
  they tile the body with no gap and no overlap. It fails on the code as it is, and it is the test this
  section exists for.
- `hiding_the_editing_area_gives_a_strip_the_whole_height` — the existing test, unchanged, which is what
  proves the `fill_the_depth` case still works when there is nothing in the middle.
- `a_collapsed_band_leaves_nothing_rather_than_a_rectangle_with_no_height` — `lay_columns_out` against a region
  with no height, asserting `Rect::ZERO`.
- `every_question_about_a_panels_place_agrees_with_what_was_drawn` — with the editing area hidden,
  `panel_area`, `zones` and the drawn `panel_rects` are compared for each showing panel.
- `space_with_every_panel_and_no_editing_area` — a screenshot: the canvas along the bottom, the explorer
  on the left and a plugin pane on the right, all four visible, with no editing area. This is the picture
  the report is about.
- `toggling_a_pane_changes_only_that_pane` — the rail's button for each of the three, pressed with the
  editing area hidden, asserting the other panels' rectangles are unchanged in the axis they do not share.

## 8. The Agent-Chat pane reads a key out of the shell profile, as everything else Unluminous starts already does

**Reported.** *"I get `illiad` reads its key from $ANTHROPIC_API_KEY and this window has no such
variable."*

**What is actually happening, and the sentence is telling the truth about this process.** Unluminous is
started from the Dock, so launchd gives it about fourteen variables and
`PATH=/usr/bin:/bin:/usr/sbin:/sbin`. `ANTHROPIC_API_KEY` is exported from `~/.zshrc`, so it is not in
this process and `std::env::var` cannot find it. `Provider::key` reads it with `std::env::var`:

```rust
pub fn key(&self) -> Option<String> {
    let named = self.key_env.trim();
    if !named.is_empty() {
        if let Ok(value) = std::env::var(named) {
```

`services::login_shell` was written for exactly this and its module comment records this exact failure:
*"`~/.zshrc` is where `ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY` and the gateway's own
`ANTHROPIC_CUSTOM_HEADERS` and `NODE_EXTRA_CA_CERTS` are set. An agent spawned straight from the window
got none of them and said it was not logged in."* `services::agent_tasks::the_key` already goes through
it. The Agent-Chat pane does not, and it cannot: `unluminous-chat` depends on `serde_json` and `ureq` and
nothing else in the workspace, and `login_shell` lives in `unluminous-app`, which depends on
`unluminous-chat`. The dependency points one way and it should keep pointing one way.

`provider.rs` already says so twice, at the two places it was forced to duplicate something for the same
reason: the `program` walk carries *"which is `login_shell::find`'s rule"*, and the keychain read carries
*"This crate cannot call that module (it is in `unluminous-app`, which depends on this one), so the two
lines are here."*

There are **three** faults of this shape, not one, and only the first is what the report saw:

1. **The key.** `key_env` is read from this process, so a key in the profile is invisible.
2. **The program.** `program()` walks `std::env::var_os("PATH")`, so `claude` in `~/.local/bin` cannot be
   found — which is the *first* of the two failures `login_shell` records, and it would have been
   reported next.
3. **The child's environment.** `agent.rs` spawns with no `.env()` at all, so a `claude` that did start
   would get launchd's fourteen variables and none of the gateway's — which is `login_shell`'s second
   failure verbatim, and is why `agent_tasks` lays `for_a_child()` under its own `SessionSettings`.

**What changes: `unluminous-chat` is handed the environment rather than reading it.** The crate keeps its
two dependencies and learns nothing about a login shell. What it gains is one value it is given:

```rust
/// The environment a request or a program is answered out of.
///
/// **Given rather than read**, because this crate cannot see `unluminous_app::services::login_shell`
/// and should not: the dependency points one way. An Unluminous started from the Dock has launchd's
/// dozen variables and the person's key is in their `~/.zshrc`, so a key read from *this process* is a
/// key that is not there — which is what `task-1905` reports. The window hands over what a command
/// typed in its own terminal would have had; a test hands over a map it wrote; and `Environment::of_this_process`
/// is the honest fallback for a caller that has nothing better, which is what every existing caller
/// gets.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    variables: Vec<(String, String)>,
    /// The `PATH` a program is looked for on, which is a variable and is also asked for on its own.
    path: Option<std::ffi::OsString>,
}
```

with `Environment::variable(name)`, `Environment::search_path()` and `Environment::of_this_process()`.
`Provider::key`, `Provider::why_not`, `Provider::program_path`, `provider::program` and `agent::run` each
take an `&Environment`, and `agent::run` lays `variables` under the child through `Command::envs` before
it spawns — which is the third fault, fixed by the same change rather than by a second one.

**Where the window builds it.** One function in `services::agent_chat`, called where the readiness is
worked out and where a turn is dispatched:

```rust
/// The environment the pane answers out of: the person's own shell profile.
///
/// `services::login_shell` is what reads it, once, on a thread at startup. This is the one place the
/// two are joined, so a second reader cannot come to a different answer about the same key.
fn the_environment() -> unluminous_chat::Environment {
    unluminous_chat::Environment::from(crate::services::login_shell::for_a_child())
}
```

`for_a_child()` already carries the whole profile with `PATH` set to `search_path()` and with `PWD`,
`OLDPWD`, `SHLVL` and `_` dropped, so this is the same environment the Agent-Tasks board's agents, the
run tile's programs and the debug adapters all get. One reading of the profile, four consumers, which is
the property that made `login_shell` worth writing.

**Three details, each of which would otherwise be a fault.**

- **`Environment` prints its names and never its values.** It holds the person's whole profile, so
  `ANTHROPIC_API_KEY` is in it, and it is a field on `Ask`, which derives `Debug` — so a derived `Debug`
  here would put a key in the next `{:?}` somebody wrote on a failed turn. Nothing prints one today, which
  is exactly why it is worth closing now: `keychain`'s rule is that a secret does not travel where it can
  be read, and `client.rs` already redacts a key out of a server's own words before quoting them.
  `printing_an_environment_shows_the_names_and_never_the_values` is the test.
- **The keychain read, which runs a program, is looked up on the profile's `PATH` too.** `security` and `secret-tool` are
  spawned by bare name today, and `/usr/bin` is in launchd's `PATH` so `security` happens to work — but
  `secret-tool` on Linux is not necessarily, and a program found by name is whichever one that machine
  finds first, which is the `tar` fault `services::debuggers` records. `read_a_keychain_entry` takes the
  `Environment` and resolves through the same `program()` walk.
- **The refusal's wording changes, because its advice was wrong.** It said *"Set it and start Unluminous
  again"*, which was true only because the value was read from this process at startup; the value is now
  read at the moment of use from a profile that is read once. What it says instead names where it looked:
  *"`illiad` reads its key from `$ANTHROPIC_API_KEY`, and neither this window nor your shell profile has
  one. Export it from your shell profile and start Unluminous again."* Still a sentence naming the
  variable and never the value, which is the rule.
- **`readiness()`'s cache is unchanged and still needed.** Its own comment says why it exists — `why_not`
  for a program is a walk of `PATH` and a directory listing per folder — and reading the profile costs
  nothing extra, because `login_shell::for_a_child` is an `OnceLock` read after the first call. What is
  new is that building an `Environment` allocates a vector of the whole profile, so `the_environment()`
  is called once per `readiness()` rather than once per row.

**What proves it.** The two halves are tested in the two places they belong.

- `a_key_in_the_environment_it_was_given_is_found_rather_than_one_in_this_process` — in `unluminous-chat`,
  with an `Environment` holding a name this process certainly does not have. It fails on the code as it
  is, because `key()` cannot be handed one.
- `a_program_is_looked_for_on_the_path_it_was_given` — the same shape for `program()`, against a folder
  the test made with a file in it. `look_up`'s own trick, and it is why `login_shell::look_up` takes a
  `PATH`: so a test can hand in its own folder.
- `an_agent_is_spawned_with_the_environment_it_was_given` — the child's own view of it, read back through
  the harness of scripted agents in `crates/unluminous-chat/tests/`.
- `the_pane_answers_out_of_the_shell_profile` — in `unluminous-app`: `the_environment()` is what
  `readiness` and `dispatch` are given, asserted by construction rather than by running a shell, because
  `login_shell::start_reading` is called only from `main` and a test must not run somebody's profile.
- `the_refusal_names_the_variable_and_never_the_value` — the existing rule, asserted again against the new
  wording.

**And it is verified in the installed binary, not only in the tests.** This is the one section whose
fault cannot be seen from a test at all: every test process has the person's environment already, so the
old code passes every test that could reasonably be written. `verify-before-saying-done` is the rule
here — the release is built, installed, started **from the Dock**, and the Agent-Chat pane's Settings
page is read: it has to say `Ready · key set from $ANTHROPIC_API_KEY` where it said the refusal. Then a
turn is sent and an answer arrives. Anything less is a report about a working tree.

## 9. The agent's half, and the one number that has to move with it

Every change above that a person can do, an agent can do by the same path. Most of them need nothing,
because the machinery does it: a menu row is walked into `action list`, and the three new rows —
`Choose Folder...`, `Zoom In` and `Zoom Out` — are reachable the day they are added because
`app/action_names.rs` fails when a menu entry has no name.

Three commands are new and each is a row in `unluminous-cli/src/catalogue.rs` and an arm in
`UnluminousApp::run_cli`:

| Command | What it does |
|---|---|
| `space here` | Which node this process is running in, what it is wired to, and the command for each — §2 |
| `space zoom <node> [--factor \| --bigger \| --smaller \| --reset]` | How big one node's contents are drawn, whichever number its kind keeps — §5 |
| `space address <node> <url>` | Type an address into a browser node's field, which is what pressing Enter in it does — §1 |

Two existing commands gain something: `space browser <node> url` answers with the page's `title` as well
as its address, because §1 makes one arrive; and `space folder <node> open` opens into a wired editor node
when there is one, which its summary now says.

`space address` is deliberately separate from `space browser <node> go`. `go` navigates, which is what an
agent wants; `address` is what the **field** does, so a test can drive the control a person uses rather
than the command underneath it — and the two reach the same `send_a_space_browser_to`. Without it the
field would be the one control on the canvas with no way to exercise it except a synthesised keystroke.

**The grouped MCP schema is at 20,792 tokens today and its ceiling is 21,000.** Three commands will cross
it. That is not a test to loosen quietly — `mcp::tools`' own note says the ceiling exists to say when the
default has grown, and that every one of the four times it moved was a real cost being accepted. So it
moves again, to 22,000, with the reason written beside the four that are already there:

```
//   20,792   where task-1905 starts
//   ~21,3xx  `space here`, `space zoom` and `space address` — the three commands `task-1905` needs
//            for the canvas: the first is what orients an agent running inside a node, which is
//            the whole of that ticket's §2, and an agent that cannot ask where it is is an agent
//            that spends nine calls working it out. Measured rather than estimated before the
//            ceiling is moved.
```

The exact number is measured with `unluminous-cli mcp tools --count` **before** the ceiling is changed,
and the comment carries the measurement rather than the estimate. `mcp serve --areas` remains the answer
for an agent that does not need the canvas at all.

## 10. The look, and what it adds to the palette

Nothing. Every new control is built from what is there:

- The two zoom buttons and the reading are `components::controls::icon_button` and a text button, in the
  view bar, which is already drawn in `color::toolbar()` with `color::divider()` under it.
- `theme::icon::zoom_in` and `zoom_out` are two drawn marks in the icon palette's existing roles.
- The address field is `controls::field_takes_the_whole_rectangle` over `color::field()` with
  `color::control_border()` round it, which is what `browser_view` already draws — it becomes a field
  rather than gaining a colour.
- A file editor node's tab strip is `components::file_tabs::show`, unchanged, so it is the same strip in
  the same colours as a pane's.
- `Choose Folder...` is a row in `components::context_menu`.

The one thing that changes shape is the add modal, which gets 64 points taller. `design/style-guide.md`
records the rule that applies: the Settings window is one size for every page and the tallest page is what
it has to hold, and *"a page that fits exactly is a page where the next line added to it is drawn off the
bottom and nobody notices"* — which is precisely what happened here to the fourth row. So the modal's
height is derived from `ROW * Kind::ALL.len()` rather than chosen, and the style guide gains a line saying
that a modal whose body is a list of a known length computes its height from that length.

## 11. What each fault would have needed to be caught

Worth writing down, because six of the eleven shipped with a full test suite passing and the pattern is
the same in five of them.

| Fault | Why every test passed |
|---|---|
| The add modal's fourth row (§4) | The test asserted `matching("")` returns four kinds, which is true. Nothing asserted a row was **drawn**, and the accepted picture of three rows was accepted. |
| The column band (§7) | Both tests of the hidden editing area showed exactly one panel, and one panel is the only case that works. |
| The folder node's clip (§3.2) | No test drew a component into a `Ui` whose clip was smaller than its rectangle. |
| The folder node's wheel (§3.1) | A `ScrollArea`'s offset is egui's state; nothing read it back. `ExplorerOutcome::scroll` exists and was only used by the panel's zoom. |
| A node's browser events (§1) | Every browser test is about a tab in the editing area, which is the list both functions walk. |
| The Agent-Chat key (§8) | Every test process has the person's environment already, so the code under test is handed the thing it is failing to read. |
| `unluminous-cli` not on a node's `PATH` (§2) | Every test drives the window through the control channel directly. Nothing had ever *typed* the command §2 tells an agent to run into a node's own shell. |
| A node's page placed in world points (§1) | A native child view is not drawn by Unluminous, so no screenshot holds one — and nothing in the window reported where the view had been put. `UnluminousApp::browser_placements` is what this change added so a test can ask. |
| Enter in the address bar (§1) | Nothing had ever typed into the field, because a node's contents are in a transformed sublayer and `Node::click` presses at the rectangle the accessibility tree reports, which is in global points. |

Four of the six become tests in this change that fail on the code as it is: `every_kind_has_room_to_be_drawn_in_the_modal`,
`a_panel_on_a_strip_does_not_annihilate_one_on_a_column_when_the_editing_area_is_hidden`,
`the_explorer_never_paints_outside_the_clip_it_was_given`, and
`a_missing_key_names_the_variable_it_would_have_come_from`. The fifth,
`a_page_that_finished_loading_says_so_on_a_node`, fails too. The sixth — the wheel — needs the harness,
and `a_folder_node_scrolls_with_the_wheel` is it.

The four that were only visible in the installed binary each got a test as well, once there was something
to assert on: `an_address_typed_into_a_browser_node_is_opened`,
`a_browser_nodes_page_is_placed_inside_the_node`,
`a_node_agents_environment_points_at_the_command_that_orients_it`, and — for the Agent-Chat key, which no
test process can be made to lack — `a_program_is_looked_for_on_the_path_it_was_given` beside it. Every one
of those was confirmed to fail on the code it replaced.

**The last one is the reason §8 ends the way it does.** A fault that every reasonable test passes is a
fault only the installed binary shows, which is what the release step is for and what
`tasks/task-1814`'s two final findings already record: *"Neither had a failing test, and neither would
have got one: both were only visible by starting the installed binary and reading what came back."*

## 12. Tests

Four layers, as everything here has.

1. **Unit tests with no window.** `dock::regions_with`'s split across three bands and `lay_columns_out`'s guard;
   `add_modal`'s height arithmetic; the folder node's scroll delta at three zooms; `Environment` in
   `unluminous-chat`, including the harness of scripted agents for the spawned child; `space::node`'s four new
   fields written and read back through `store`; and `space here`'s reply built from a canvas.
2. **`unluminous-app` unit tests.** `a_component_never_widens_the_clip_it_was_given` across the three
   `set_clip_rect` callers; `every_question_about_a_panels_place_agrees_with_what_was_drawn`;
   `each_kind_of_node_zooms_the_number_that_decides_its_size`; the menu shapes for `Choose Folder...`.
3. **Screenshot tests**, each through `builder()` in the test file and never `Harness::builder()`, for the
   reason `task-1654` gives. Six new pictures — `space_browser_empty`, `space_folder_node_over_the_rail`,
   `space_editor_node_tabs`, `space_editor_node_zoomed`, `space_zoom_controls`,
   `space_zoom_controls_at_the_end_of_the_ladder`, `space_with_every_panel_and_no_editing_area` — and
   **four accepted pictures deliberately change**:
   - `space_add_modal`, which gains its fourth row and 64 points of height.
   - `space_nodes` and `space_working`, because a browser node now draws a toolbar.
   - `browser_tab`, because the editing area's address strip becomes a field.
   Nothing is accepted without opening the image and looking at it, which is `UPDATE_SNAPSHOTS=1`'s own
   condition.
4. **The real window.** This is the layer four of the eleven need and the layer §8 can only be answered
   at. Built, installed with `bash installer/macos/build.sh --no-dmg --install`, started **from the Dock**
   so launchd's environment is the one in the process, and then:
   - The Agent-Chat Settings page says `Ready · key set from $ANTHROPIC_API_KEY`, and a turn answers.
   - A browser node is given an address by typing in its field, and its title appears on its header.
   - A terminal node is wired to that browser node, `claude` is started in it, and asked to open a page:
     the transcript has to show it running `space here` and then `space browser … --from …`, which is the
     measurement §2 exists for. Fewer calls than the report's nine, and no shell commands.
   - A folder node is scrolled, given a different root, and dragged over the rail.
   - The editing area is toggled off with the Explorer, a plugin pane and the canvas showing, and all
     three are on the screen.

## 13. What the review found

`tasks/unluminous-issues-2.md` asks for a Codex Sol review, and it was given the whole diff, this document
and `CLAUDE.md`, and asked to be adversarial about coordinate spaces, index arithmetic, credentials and
anything where a comment claims a guarantee the code does not give. It raised thirteen things. **Nine were
real and are fixed**, and every one of them has a test that fails on the code as it was. They are recorded
here rather than only in commit messages because six of the nine are the same shape as the faults §11 lists:
something no test asked about.

- **A node was not a target for a tab drag at all.** `settle_the_tab_drag` ran between the panes and the
  canvas, so `node_tab_strips` was empty every time it was read — dragging a pane's tab onto a node had
  nowhere to land, and a drag reported *by* a node was cleared before the next frame could act on it. It is
  settled after the canvas now, which is what its own rule always said: once everything that can hold a tab
  has been drawn. The test asserts the **ordering**, because a synthesised pointer cannot reach a strip in a
  transformed sublayer.
- **Closing a node orphaned every tab on it but one.** Both `close_a_space_node` and `delete_a_space_view`
  closed only the tab that was *showing*, so the rest kept a `Home::Node` naming a node that had gone —
  reachable from nothing, drawn by nothing, and still holding whatever had been typed into them. Every tab
  now, highest index first, because `close_tab` removes from the vector.
- **A stale key could stand in for one the profile had removed.** `variable_of` fell back to this process,
  so an Unluminous started from a terminal that still held an old `ANTHROPIC_API_KEY` went on sending it
  after its owner had taken it out of their profile — and the settings page went on saying the row was ready.
  A revoked key that goes on being sent is the worst shape this could take. The fallback is gone for a
  variable and **kept for `PATH`**, and the difference is written down: a `PATH` is not a secret, and an
  environment naming none is a caller with nothing to say about where to look.
- **A spawned agent inherited what the profile had dropped.** `Command::envs` replaces the names it is given
  and leaves the rest, so the same stale key reached `claude` and `codex`. `env_clear` first — but only for
  an environment that really is one, which is `Environment::is_whole`: clearing for the little
  `login_shell::for_a_child` falls back to when the profile could not be read would start an agent with no
  `HOME`, which is a worse failure than a stale variable.
- **A half-typed address was thrown away by losing the focus.** The branch that keeps the bar showing where
  the page is asked "does nobody have the focus", so pressing Reload, clicking another node or clicking into
  a pane wiped it — and Escape is the key that is *for* putting the address back. What the field holds is the
  person's until they enter it or press Escape, which is one flag beside `typed` and in the same two homes.
- **Using a node's toolbar did not select that node.** Only a click on the page body took the focus, so
  typing into a node that did not own the one native view left it unselected — and `BrowserHost` refuses to
  drive a tab it is not pointed at, so Enter answered *"that rendered tab is not the one showing"*.
- **The wheel and the modifier wheel went to the node underneath.** Each node asked "am I under the pointer"
  as it was drawn, and the loop draws back to front, so the backmost of a stack won — and because a folder
  node clears the scroll delta, the node somebody was looking at got nothing. Which node owns the pointer is
  decided **once**, before any of them is drawn, and the chosen node wins when the pointer is inside it,
  because `move_to_top` puts a clicked node above the model's own order.
- **The one native view went to the backmost of two overlapping browser nodes.** `choose` fell back to
  `placements.first()`, and a placement is pushed as its owner is drawn — so the first is the furthest
  behind, and its native child composited above the node on top of it.
- **A node's font reached one tab and never left.** The size was remembered against the *node*, so the second
  tab shown in it was never restyled, a tab dragged back into a pane kept the node's size, and putting the
  node back to the window's own size skipped the restyle altogether — the document stayed visibly zoomed
  while the state reported the default. It is remembered against the **tab**, because a document is what
  carries a base style, and a pane puts any tab that arrives with one back.

Two more it raised are answered rather than fixed, and the reasons are worth keeping.

**A page reflows rather than scaling, and a clipped node's page reflows too.** A `WebView` is a real window:
it has no transform, so `set_bounds` alone makes a node at half the zoom lay its page out at half the width
rather than draw it half as large. What can be done is done — the camera's zoom is spent on the page's *own*
zoom, so the two compose and a zoomed node's page is genuinely smaller. What cannot is the clipping: there is
no cropping a native child, so a node hanging off the canvas has its viewport narrowed rather than cut, and a
page laid out against the viewport reflows. That is the same limitation `documentation/overview.md` already
records about photographing one, and it is stated here rather than worked around.

**And the diff it was handed does not apply to `HEAD` on its own**, which was its first finding and is true:
the working tree also holds an unrelated ticket's uncommitted work, so a diff of task-1905's files alone
references things those files do not define. That is a fact about the review's input rather than about this
change, and it is why the review was given the tree rather than only the patch.

Four of the thirteen were **not** faults, and the review says so plainly: `dock::regions_with` tiles the body
with no overlap, gap or negative rectangle at every boundary of `share_the_depth` and `fill_the_depth`; the
`drag_tab` index arithmetic is sound; `zoom_steps` is not spent twice; and nothing prints an environment's
values.

## 14. What is deliberately left out

- **A per-node MCP server.** §2 says why: the tools an agent is handed would stop being generated from the
  catalogue alone, which is the one thing `mcp::tools` exists to prevent.
- **`unluminous-cli` adding `--from` on its own** when `UNLUMINOUS_SPACE_NODE` is set. §2 says why: the
  same command line would mean two things depending on where it was typed.
- **Making the Agent-Tasks pane independent of the terminal tile.** §7 says why: two character grids in one
  strip are two grids half the size, which is `task-1683`'s rule and is a product decision rather than a bug.
  `Move to` is how a person gets both.
- **A browser node's page in a screenshot.** Unchanged and stated again because §1 makes the toolbar look
  as though it should be there: a rendered page is a native child window the operating system composites
  on top of the surface `ViewportCommand::Screenshot` captures, so no picture Unluminous takes holds one.
- **More than one tab in a **folder** or **browser** node.** §4 gives a file editor node tabs because the
  report asks for them and because `Home::Node` already made a node's tab an ordinary tab. A second folder
  in one node is a second tree, and a second page is a second native view, which a window has not got.
- **Registering an `AreaState` per node layer** to make `rect_contains_pointer` answer inside a node. §3.1
  says why: it would put every node in `layer_id_at`'s answer and change what the canvas behind them thinks
  the pointer is over. The comment naming the limitation is the cheaper and more honest answer.
- **A minimap, and node groups.** `task-1904` §14 left both out and nothing here changes that.
