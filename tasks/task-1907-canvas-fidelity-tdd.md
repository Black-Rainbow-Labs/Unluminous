# task-1907 — the canvas comes back whole, and four faults in the way of reading it

Six reports. Five are about the Base of Infinite Space and the sixth is about the panel it sits in, and
every one of them has a cause that can be pointed at rather than guessed at:

1. *"When i close and open the project, my views in Base of Infinite Space are not restored to what they
   were."*
2. *"The browser node is on example.com and if i type google.com and enter, nothing happens."*
3. *"The text in the nodes looks pixelated when I zoom in on the canvas. e.g. to 150%, 200%, etc."*
4. *"The line numbers on the file view don't shrink the same as the text to the right of them."*
5. *"My terminal sessions aren't opened back up. e.g. if i just have a view with a terminal with
   claude-code open, then quit, re-open, the terminal is there but no claude code. I want the exact
   session as though I never closed anything to begin with."*
6. *"When the infinite space is a bottom panel with agent tasks above it, i'm unable to resize by moving
   the top up, it will only go down."*

Every section says the same four things: what was reported, what is actually happening, what changes, and
what proves it. `CLAUDE.md`'s rule applies to each — the control a person uses, the same code reached by
an agent through the same path, and tests over both.

## 0. What was measured first

Read against the installed 0.39.1, built at 4:02pm on 11 September 2026, with `task-1906`'s uncommitted
work in the tree. `cargo test --workspace` is green before any of this: 553 of 554 screenshot tests pass
and `a_vector_is_drawn_as_a_vector_rather_than_as_its_bytes` fails only in the parallel run and passes
alone, which is the flake `unluminous-flaky-tests` already records.

**The divider fault is arithmetic and was reproduced as numbers.** A window 670 points tall with the
canvas alone along the bottom: the canvas asks for 560 and is drawn 550, because `share_the_depth` keeps
`EDITOR_MIN_HEIGHT` for the editing area. Dragging the divider **up** 120 points then does this:

```
up 120:        stored 560 -> 550, drawn 550 -> 550     (nothing moved)
down 120:      stored 550 -> 430, drawn 550 -> 430     (moved)
up 120 again:  stored 430 -> 550, drawn 430 -> 550     (moved, back to the wall)
```

With the Agent-Tasks board above it — the arrangement the report names — the same drag is worse, because
the two panels are being scaled to fit and the stored numbers are no longer the drawn ones:

```
board  stored 420  drawn 235.71
space  stored 560  drawn 314.29
up 120:        space stored 550  drawn 311.86
down 120:      space stored 430  drawn 278.24
```

Ten points of pointer bought three points of movement, and the first drag bought none at all.

**The gutter fault is a field read off the wrong thing.** `UnluminousApp::gutter` builds the gutter with
`font_size: self.settings.font_size` — the *window's* setting. An editor node sets its own size on the
document through `set_base_style`, so the text changes and the numbers beside it do not. On top of that
`gutter::type_size` clamps to 9 and 28 points, so the numbers stop following the text below 13 and above
39 even in a pane:

| editor points | numbers want | numbers drawn |
|---:|---:|---:|
| 8 | 5.75 | **9.00** |
| 12 | 8.62 | **9.00** |
| 16 | 11.50 | 11.50 |
| 32 | 23.00 | 23.00 |
| 48 | 34.50 | **28.00** |
| 144 | 103.50 | **28.00** |

**The pixelation is `epaint`'s, and it is not fixable by asking `egui` nicely.** A node is drawn into a
layer carrying a `TSTransform`, and `egui::LayerId`'s transform is applied to the **shape** after the
galley has been built — `egui-0.36.1/src/layers.rs:237` calls `ClippedShape::transform`, and
`epaint-0.36.1/src/shapes/text_shape.rs::transform` scales the vertices of an already-rasterised mesh:

```rust
for v in &mut mesh.vertices {
    v.pos *= transform.scaling;
}
```

`pixels_per_point` is destructured as `pixels_per_point: _` and deliberately left alone, and the glyphs
carry a `// TODO(emilk): would it make sense to transform these?`. So a galley in a transformed layer is a
magnified bitmap, and the atlas is uploaded `TextureOptions::NEAREST`, which is why it reads as blocky
rather than merely soft.

**Unluminous's own atlas has no such limit**, and that is what makes §3 possible at all.
`TextRenderer::glyph` keys on `quarter_points: (style.size * 4.0).round()` and rasterises through
`PxScale::from(style.size)`, so it will rasterise at any size it is asked for.

**The browser fault is two faults, and each was isolated on a real window rather than reasoned about.**
`send_a_space_browser_to` parses the address to validate it and then hands the **raw typed text** to the
view:

```rust
let location = crate::services::browser::BrowserLocation::parse(address, self.tree.root())?;
let remote = location.source_path().is_none();
if let (true, Some(tab)) = (remote, self.space.live.browser(node).map(|tab| tab.id)) {
    self.browser.navigate(tab, address.trim())?;      // `address`, not `location`
```

The doc comment directly above it states the rule this breaks: *"Handing the typed text straight to the
view is what a live window refused with 'Class not registered': wry passes an unknown scheme to
`Navigate`, which refuses it."* Driven against a real window, one node on a canvas:

```
# no tab yet, so this takes the open-a-tab path, which parses properly
$ space browser 2 go --url example.com    ->  space browser 2 url  ->  https://example.com/

# the node now has a tab, so this takes the navigate path
$ space browser 2 go --url google.com     ->  space browser 2 url  ->  https://example.com/   FAILS
$ space browser 2 go --url https://example.org/ -> space browser 2 url -> https://example.org/  works
```

So the first fault is exactly and only the missing scheme, and it appears only once a node has a page —
which is precisely the report: *"the browser node is on example.com and if i type google.com and enter,
nothing happens."*

**The second fault is that a node which is not the one rendering cannot be driven at all, and says so
where nobody is looking.** `BrowserHost::navigate` opens with `self.for_the_showing_tab(id)?`, because a
window has one native view. With two browser nodes on a canvas, asking the one that is not rendering to go
somewhere answers, in full:

```
$ space focus 2
$ space browser 3 go --url https://example.org/
That rendered tab is not the one showing.
```

That is an honest refusal and it reaches the command line. What it does **not** do is stop the node's own
record being changed, because `send_a_space_browser_to` records the address before it navigates — so the
canvas ends up permanently disagreeing with itself:

```
$ space list          ->  *3  browser  ...  https://example.org/
$ space browser 3 url ->  https://google.com/
```

One is the state that will be written to `space.conf` and the other is the page on the screen, and nothing
reconciles them. A window reopened from that file goes to `example.org`, which is a page the person never
managed to reach.

**The terminal fault is that nothing records what is running.** A node's `command` is what the *node* was
given, and `claude` typed into the shell is not that. Measured on a real window: add a terminal node, type
into it, and the node reads back

```json
{ "command": "", "folder": "/tmp/space-probe", "session": "", "kind": "terminal" }
```

which is what the reporter's own `.unluminous/space.conf` holds — a `terminal` node with a `folder`, a
size and a position, and no `command` and no `session` at all. `task-1906` gave a node the ability to
resume a conversation and it only reaches a node whose **command** is an agent; a node running a shell
that somebody then typed `claude` into is invisible to all of it.

**And the views report is the same fault seen from further away.** `store::save` writes a fresh `Values`
each time, so `space.conf` is the whole truth about a canvas — and the reporter's file holds exactly one
view with one terminal node in it. Nothing was lost in the reading; what was never written is everything
about that node except where it is.

## 1. A divider between two panels moves both ways

**Reported.** *"When the infinite space is a bottom panel with agent tasks above it, i'm unable to resize
by moving the top up, it will only go down."*

**What is actually happening.** Two numbers describe one panel and they have come apart. A panel's
*stored* height is what it asked for, in `panes.<panel>.height`; its *drawn* height is what
`dock::regions` gave it, which is the stored one scaled down by `share_the_depth` whenever the two strips
together ask for more than there is. `act_on_a_panel_divider` adds the pointer's movement to the
**stored** number and clamps it against the room:

```rust
let room = panes.height() - dock::EDITOR_MIN_HEIGHT;
self.panes.resize(panel, drag.delta * sign, room);
```

So on the measured 670 point window the canvas is stored at 560, clamped against a room of 550, and
dragging its divider up asks for 680 and gets 550 — which it is already drawn at. The divider does not
move, and no amount of dragging will move it, because the panel is against a wall the stored number
reached before the drawn one did. Dragging **down** works because it reduces the stored number back into
the range where the two agree.

`task-1771` fixed this once, for the case where the editing area is hidden, and
`move_a_divider_with_no_editor` says why in as many words: *"a panel's stored measurement is then a share
rather than a size, so ten points of pointer became three points of movement."* Its two steps are exactly
right — **write the rendered measurements back**, so the stored numbers add up to the room and the
proportional share is the identity, then **take what this side gains off the side facing it**. What was
missed is that `share_the_depth` scales the panels *whenever they ask for more than the room*, which
happens with the editing area showing too. The fix that shipped was gated on `self.editor_visible`, and
the fault it was written for is not.

**What changes.** The gate moves from *is the editing area hidden* to *are the stored numbers the drawn
ones*, which is the question that was always being asked. `move_a_divider_with_no_editor` is renamed
`move_a_divider_by_sharing` and reached whenever a side is being scaled, and
`act_on_a_panel_divider` asks one question:

```rust
/// Whether the panels on `panel`'s axis are being drawn at the sizes they asked for.
///
/// **The question `act_on_a_panel_divider` has to ask before it adds a drag to a stored number.**
/// `dock::share_the_depth` scales a side down whenever the two strips together want more than there is,
/// so a panel's stored height is then a *share* rather than a size — and adding ten points of pointer to
/// a share that is already against its clamp moves the divider not at all. `task-1771` found this with
/// the editing area hidden and fixed it there; the scaling happens with the editing area showing too,
/// which is what `task-1907` reports. Comparing the two numbers is the honest form of the question, and
/// it costs one subtraction a panel.
fn the_sizes_are_being_shared(&self, panel: dock::Panel) -> bool { … }
```

Two things about it are decisions rather than details.

**It compares the drawn rectangle against the stored number rather than re-deriving the scale.** The
rectangles are already on `self.panel_rects`, put there by the frame that drew them, and a second
computation of `share_the_depth` here would be a second place for the two to disagree — which is the
`follow_the_open_file` rule about a derived answer beating a reported one.

**And the sharing path already handles the case where nothing is being shared**, because its first step is
to write the rendered measurements back: with the numbers already equal that step is the identity and the
rest is the ordinary take-from-the-far-side move. So the two paths could in principle be one. They are
kept separate because the plain path is what 554 accepted screenshots were taken through, and a divider
that is not against a wall should keep costing one clamp rather than a walk of every panel on the axis.

**What proves it.**

- `a_bottom_strips_divider_moves_up_as_well_as_down` — the measurement above, made a test: the canvas
  alone along the bottom of a 670 point window, dragged up 120 and then down 120, and the stored height has
  to move both times. It fails on the code as it is with `560 -> 550`, which is 10 points of 120.
- `a_divider_between_two_panels_in_one_strip_moves_the_pointers_distance` — the board above the canvas,
  which is the arrangement reported, asserting that the **drawn** height follows the pointer rather than a
  fraction of it. It fails on the code as it is at 2.4 points of 120.
- `two_panels_on_one_axis_always_add_up_to_the_room` — the invariant the sharing path keeps, checked after
  each of a run of drags in both directions, because a fix that moved the divider by the right amount and
  left the two sides not adding up would be a fix that leaves a gap.
- The existing `a_panel_that_has_moved_is_resized_by_the_edge_that_faces_the_document` and
  `dragging_the_terminal_divider_*` tests are unchanged, which is what says the plain path still works.

## 2. An address typed at a browser node goes there

**Reported.** *"The browser node is on example.com and if i type google.com and enter, nothing happens."*

**What is actually happening.** Two faults, and §0 has the runs that separate them. They are described
separately here because they need different fixes and only one of them is what was reported.

**A bare host loses its scheme on the way to the view.** `BrowserLocation::parse` turns `google.com` into
`Remote { url: "https://google.com/" }` — `implied_address` is the one place a scheme is added — and
`send_a_space_browser_to` then hands `address.trim()` to `BrowserHost::navigate` rather than the parsed
value. `engine_url` only rewrites the `unluminous://` project origin, so a bare host reaches wry's
`Navigate` with no scheme at all, where it is refused **in silence**: no error, no `Err`, and the pane goes
on showing the old page.

It appears only once the node has a page, which is why the report is phrased the way it is. A node with no
tab yet falls through to `open_a_space_browser`, which passes the parsed `BrowserLocation` to `open_tab` and
lets `BrowserTab::new` take the address from `location.initial_url(id)` — so the first address a node is
given always works and every one after it does not.

**Nothing else about that path is broken**, and saying so narrows the fix. `BrowserTab::arrived_at` is
called from the `LoadFinished` event and pushes a new history entry, truncating whatever was ahead of it, so
a navigation that really happens updates `current_url()` with no help from the caller. Measured: a full URL
on the same node works, end to end. The address is the whole of it.

**And a node that is not the one rendering cannot be driven, while its record changes anyway.**
`BrowserHost::navigate` opens with `self.for_the_showing_tab(id)?` because a window has one native view —
which is `task-1756`'s measurement and not a thing to argue with. The refusal is honest and it reaches the
caller. What is wrong is the order of the two things `send_a_space_browser_to` does: it records the address
on the node **before** navigating, so a refused navigation leaves the canvas holding an address its page
never reached, and `space.conf` is then written from it.

**What changes.** Three things.

**The parsed address is what travels.**

```rust
// **The parsed address, not the typed one.** `implied_address` is what puts a scheme on a bare host, and
// wry hands an unknown scheme to `Navigate`, which refuses it in silence — so the pane kept the old page
// while the toolbar said it had gone. `task-1907`.
let url = location.initial_url(tab);
self.browser.navigate(tab, &url)?;
```

`initial_url` already exists and is what `open_tab` uses, so both halves of the window normalise through one
function rather than two that happen to agree.

**The record follows the navigation rather than preceding it**, so a node that could not be driven does not
claim to be somewhere it is not. That is `start_a_space_terminal`'s own rule, which writes a conversation id
down only once the program really started, applied to an address.

**And a node that is not rendering is given the address rather than refused it.** This is the part that is a
decision rather than a repair, and the alternative was considered: leaving the refusal as it is would be
defensible, since it is true and it is reported. It is rejected because the thing a person does on a canvas
with two browser nodes is type into both of them, and an editor in which half the controls answer *"that
rendered tab is not the one showing"* is an editor with a rule in it nobody can hold. The limit is about the
**view**, not about the tab: what a tab knows is where it should be.

So `BrowserTab::heading_for_a_new_page` records the destination in the tab's own history without touching
the view, and the reconciliation that already runs in `raw_input_hook` — the one that points the single view
at whichever tab is showing — sends the view there when that node becomes the one rendering. That is
`pointed_at`'s existing pair, which does the same thing for a tab being switched **to** in the editing area.

```rust
/// Record an address this tab has been sent to, whether or not the shared view can go there now.
///
/// **A window has one native view**, so `BrowserHost::navigate` refuses a tab that is not the one showing.
/// Before `task-1907` that refusal threw the address away while the *node* recorded it, so a canvas with
/// two browser nodes ended up holding an address its page had never reached — and `space.conf` was written
/// from that. What a tab knows is where it should be; the view is sent there when this tab is the one
/// rendering, which is what `pointed_at` already does for a tab being switched to.
pub fn heading_for_a_new_page(&mut self, url: &str) { … }
```

**What proves it.**

- `a_bare_host_sent_to_a_node_that_already_has_a_page_is_given_a_scheme` — with no window: what
  `send_a_space_browser_to` hands the host for `google.com` is `https://google.com/`. It fails on the code as
  it is, and it is the report.
- `an_address_typed_at_a_node_that_already_has_a_page_really_moves_it` — through the harness, the §0 run made
  a test.
- `a_browser_node_that_is_not_rendering_still_records_where_it_was_sent` — the second fault, which no test
  covered because every browser test until now has had one node.
- `a_node_that_could_not_be_driven_does_not_claim_to_have_moved` — the ordering, asserted on the node's own
  record rather than on the reply.
- `a_nodes_own_record_and_its_page_agree_about_where_it_is` — `space list` and `space browser url` asked after
  one navigation, because the two disagreeing is what made this hard to attribute.
- `a_local_page_still_opens_a_tab_of_its_own` — the branch this does not change, so the `unluminous://` origin
  is not broken by the fix.
