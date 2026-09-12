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

They are answered in a different order from the one they were reported in, because two of them turn out to be
one fault seen from two distances: the sections are the divider (§1), the browser node's address (§2), the
pixelation (§3), the line numbers (§4), what a terminal node was running (§5) and what a canvas writes down
(§6, which is §5 seen from further away).

Every section says the same four things: what was reported, what is actually happening, what changes, and
what proves it. `CLAUDE.md`'s rule applies to each — the control a person uses, the same code reached by
an agent through the same path, and tests over both. Four of the six were isolated by driving the installed
build rather than by reading, and each of those says what the run was.

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

**The first line of that is not a fault, and finding out why narrowed the whole section.** A 670 point window
keeps `EDITOR_MIN_HEIGHT` for the editing area, so 550 is the most a strip along the bottom can have — the
canvas defaults to 560 and is therefore against its maximum the moment it opens. Dragging up correctly does
nothing there. What the report is about is the case below it.

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

**And the editing area has to be counted as room, which is the half the implementation found.** The sharing
path takes what one side gains off the side facing it, and `givable` was the facing panels' spare only — so a
strip whose facing side has no panels on it could not grow at all, whatever the pointer did. That is right
with no editing area, where the two strips really are the whole window, and wrong with one, where the thing
between them is exactly what should give the room up. `from_the_editor` is the editing area's drawn size above
its own minimum, read off `panel_rects.editor` rather than recomputed, and it is added to `givable`.

**And the sharing path already handles the case where nothing is being shared**, because its first step is
to write the rendered measurements back: with the numbers already equal that step is the identity and the
rest is the ordinary take-from-the-far-side move. So the two paths could in principle be one. They are
kept separate because the plain path is what 554 accepted screenshots were taken through, and a divider
that is not against a wall should keep costing one clamp rather than a walk of every panel on the axis.

**What proves it.**

- `a_bottom_strip_dragged_smaller_can_be_dragged_back_up` — the canvas alone along the bottom, dragged down
  150 and then back up 150. This is the plain case, and it is written down 150 first because a canvas on its
  own opens against its maximum, as §0 records.
- `a_divider_under_two_panels_moves_the_pointers_distance` — the board above the canvas, which is the
  arrangement reported, asserting that the **drawn** height follows the pointer rather than a fraction of it.
  It fails on the code as it is at 2.4 points of 120.
- `two_panels_on_one_axis_always_add_up_to_the_room` — the invariant the sharing path keeps, checked after
  each of a run of drags in both directions, because a fix that moved the divider by the right amount and
  left the two sides not adding up would be a fix that leaves a gap.
- The existing `a_panel_that_has_moved_is_resized_by_the_edge_that_faces_the_document` and
  `dragging_the_terminal_divider_*` tests are unchanged, which is what says the plain path still works.

## 2. An address typed at a browser node goes there

**Reported.** *"The browser node is on example.com and if i type google.com and enter, nothing happens."*

**What is actually happening.** Two faults, and §0 has the runs that separate them. They are described
separately because they need different fixes and only one of them is what was reported.

**A bare host loses its scheme on the way to the view.** `BrowserLocation::parse` turns `google.com` into
`Remote { url: "https://google.com/" }` — `implied_address` is the one place a scheme is added — and
`send_a_space_browser_to` then hands `address.trim()` to `BrowserHost::navigate` rather than the parsed
value. `engine_url` only rewrites the `unluminous://` project origin, so a bare host reaches wry's
`Navigate` with no scheme at all, where it is refused **in silence**: no error, no `Err`, and the pane goes
on showing the old page.

It appears only once the node has a page, which is why the report is phrased the way it is. A node with no
tab yet falls through to `open_a_space_browser`, which passes the parsed `BrowserLocation` to `open_tab` and
lets `BrowserTab::new` take the address from `location.initial_url(id)` — so the first address a node is
given always works and every one after it does not. Isolated on a real window three times over.

**Nothing else about that path is broken**, and saying so narrows the fix. `BrowserTab::arrived_at` is
called from the `LoadFinished` event and pushes a new history entry, truncating whatever was ahead of it, so
a navigation that really happens updates `current_url()` with no help from the caller. Measured: a full URL
on the same node works, end to end. The address is the whole of it.

**And a node that is not the one rendering cannot be driven, while its record changes anyway.**
`BrowserHost::navigate` opens with `self.for_the_showing_tab(id)?` because a window has one native view —
which is `task-1756`'s measurement and not a thing to argue with. The refusal is honest and it reaches the
caller: driven on a real window, `space browser 3 go` answered *"That rendered tab is not the one showing."*
What is wrong is the order of the two things `send_a_space_browser_to` does: it records the address on the
node **before** navigating, so a refused navigation leaves the canvas holding an address its page never
reached, and `space.conf` is then written from it.

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

## 3. Text in a node is rasterised at the size it is seen at

**Reported.** *"The text in the nodes looks pixelated when I zoom in on the canvas. e.g. to 150%, 200%,
etc."*

**What is actually happening, and it is a documented trade-off rather than an oversight.**
`task-1904` wrote it down in as many words: *"a layer's mesh is tessellated at its own scale and then
scaled, so text at a zoom other than 1.0 is scaled pixels. At 1.0, the default and where somebody reads
code, it is exact."* Each node is drawn into an `egui` layer carrying a `TSTransform` with the camera in it,
and the transform is applied to the **shape**, after the galley has been built.
`epaint-0.36.1/src/shapes/text_shape.rs::transform` is the whole of it:

```rust
for v in &mut mesh.vertices {
    v.pos *= transform.scaling;
}
```

`pixels_per_point` is destructured as `pixels_per_point: _` and deliberately left alone, and the glyphs
carry a `// TODO(emilk): would it make sense to transform these?`. The uv coordinates go on pointing at the
atlas entry rasterised at the original size, so what is drawn at 200% is a bitmap magnified two-to-one. The
atlas is uploaded `TextureOptions::NEAREST`, which is why it reads as blocky rather than merely soft.

**There is no per-layer `pixels_per_point` to ask for.** `epaint::text::Fonts::with_pixels_per_point`
exists and the layout cache is keyed on it, so a galley *can* be rasterised at another scale — but
`egui::Context::fonts` and `fonts_mut` both hardwire `ctx.pixels_per_point()`, and `Context::tessellate`
takes one number for the whole frame. `layers.rs::drain` carries only the transform map. So nothing a caller
can reach changes this for a galley.

**Unluminous's own atlas has no such limit**, and that is the whole of why this is fixable at all.
`TextRenderer::glyph` keys on `quarter_points: (style.size * 4.0).round()` and rasterises through
`PxScale::from(style.size)`, so it will rasterise at whatever size it is asked for and cache that size
beside the others.

**Which text is which matters, so it is written down rather than summarised.** Inside a node body:

| What | Drawn by | Sharp at a zoom? |
|---|---|---|
| Terminal grid glyphs | Unluminous atlas — `terminal_panel::paint` | **Fixable** |
| Editor document body | Unluminous atlas — `editor_view::paint_text` | **Fixable** |
| Gutter line numbers | egui galley, `FontId::monospace` | Not by the atlas |
| Node header and title | egui galley, `FontId::proportional(12.0)` | Not by the atlas |
| Folder node rows | egui galley, `FontId::proportional(view.at(…))` | Not by the atlas |
| Editor node tab strip | egui galley | Not by the atlas |
| Inline debug values | egui galley | Not by the atlas |

So the two things somebody actually **reads** in a node — the code and the terminal — are the two that go
through the atlas, and they are what this section fixes. The furniture is dealt with separately below.

### 3.1 The atlas is asked for the size the glyph is seen at

One value reaches the two painters: how many pixels a point is worth in this layer. It is the camera's zoom,
and it is passed rather than read, because a component takes what it draws.

```rust
/// How many pixels one point is worth where this text will be composited.
///
/// **1.0 everywhere but on the canvas.** A node is drawn into a layer carrying the camera, and `epaint`
/// applies that transform to the finished shape — so a glyph rasterised at 12 points and composited at 200%
/// is a bitmap magnified two-to-one, which is what `task-1907` reports. The atlas will rasterise at any size
/// it is asked for, so what is asked for is the size the glyph is *seen* at, and the quad it is drawn into
/// is divided by the same number so the layout is unchanged.
///
/// **Quantised**, because a continuous zoom would otherwise ask the atlas for a new size every frame of a
/// pinch and clear it when it filled — see [`RASTER_STEPS`].
pub struct Crispness { … }
```

Three rules in it, and each is the difference between this working and this being slower and no sharper.

**The layout is unchanged and only the raster size moves.** `style.size` decides where every glyph goes, so
multiplying it would relayout the document — which is the one thing `task-1904` promises a zoom does not do.
What changes is the size the atlas is asked to rasterise at, and the quad is then `glyph.size / scale`, so
the ink lands in exactly the rectangle it landed in before.

**The pixel snapping moves into screen space.** Both painters `.round()` a glyph's position to whole points,
because *"a glyph drawn on a fraction of a pixel is resampled, which softens every letter in the grid"*. At a
zoom, a whole point is not a whole pixel, so the round has to be `(x * scale).round() / scale`. Left as it
is, the snapping fights the scaling and the letters come out unevenly spaced, which is worse than the
blur it was written to prevent.

**And the terminal's cell metrics stay at world size.** `cell_metrics` rounds to whole points and the grid's
rows and columns are derived from it, so scaling the metrics would change the cell count — a relayout of the
terminal, and a resize sent to the program on the far side. Metrics at world size, raster at screen size.

**`RASTER_STEPS` is why this does not thrash the atlas.** The atlas is one 1024 by 1024 texture that is
cleared and started again when it fills, so a distinct raster size per frame of a pinch would clear it
repeatedly and cost more than the blur. The scale is snapped to quarter steps, which is the granularity the
glyph key already has — `quarter_points` — so between `MIN_ZOOM` of 0.25 and `MAX_ZOOM` of 2.5 there are ten
sizes rather than an unbounded number, and a pinch settles onto one of them.

### 3.2 The furniture is drawn at a fixed size on the screen

The gutter, the node header, the tab strip and a folder node's rows are galleys, and §3's opening says why
no atlas trick reaches them. Two answers were weighed.

**Scaling the `FontId` and dividing the layout back** works — `FontImpl::styled_metrics` multiplies the font
size by `pixels_per_point`, and the glyph cache key includes the scale factor, so `FontId::proportional(12.0
* zoom)` really does rasterise at the composited size. It is rejected for the header and the tab strip
because every measurement taken from the galley then has to be divided by the same number, in each of the
places that lay one out, and a place that forgot would be a control drawn at twice its size. That is a list
whose next entry is the one that forgets.

**Drawing them in screen points instead** is what is done, and it is the precedent the canvas already set
twice. A node's *decoration* is recorded in screen points into the pane's own `Chrome` for exactly this
reason, and a browser node's native child is converted to screen points and given the camera as its own page
zoom because *"a native child cannot be transformed"*. A node's header is furniture of the same kind: it is
Unluminous's own chrome rather than the node's content.

So the header's text is painted into the pane's layer at the screen rectangle the header occupies, at a fixed
point size. Two things follow, and both are improvements rather than costs. A header stops being
unreadable at 0.25, which is the other end of the same complaint. And a header stops being drawn at 30 points
at 2.5, which was taking room from the node's own contents.

**The gutter and a folder node's rows keep their galleys and are left scaled**, which is stated plainly
rather than hidden: they are *inside* the node and have to scale with it, so drawing them in screen points
would leave the numbers not lining up with the lines they count. They gain §3.1's sharpness only when
`epaint` gains a per-layer `pixels_per_point`, and the day it does the note in `vello_canvas` about following
epaint applies here too. What §4 fixes is a different fault in the same column.

**What proves it.**

- `the_atlas_is_asked_for_the_size_a_glyph_is_seen_at` — with no window: at a camera of 2.0 a 12 point style
  asks the renderer for 24 points, and the quad drawn is the 12 point rectangle.
- `a_zoom_does_not_relayout_a_node` — the promise `task-1904` made, kept: the terminal's rows and columns and
  the document's line breaks are identical at 1.0 and at 2.0. This is the test that fails if the metrics are
  scaled by mistake.
- `a_raster_size_is_quantised_so_a_pinch_asks_for_ten_sizes_rather_than_a_hundred` — the whole zoom range
  walked in small steps, counting the distinct sizes asked for.
- `a_glyph_is_snapped_to_whole_pixels_rather_than_whole_points` — the arithmetic, because uneven letter
  spacing is the failure this rule prevents and it is invisible in a small picture.
- `space_zoomed_in` and `space_zoomed_out` — screenshots at 2.0 and at 0.5, which is the comparison the report
  makes and the only way the header decision can be looked at.
- `a_nodes_header_is_the_same_size_at_every_zoom` — asserted rather than pictured, on the rectangle the header
  text is laid out in.

## 4. The line numbers follow the letters they count

**Reported.** *"The line numbers on the file view don't shrink the same as the text to the right of them."*

**What is actually happening, and it is two faults in the same column.**

**The gutter is built from the window's font size rather than the file's.** `UnluminousApp::gutter` reads
`self.files.active()` for everything else it shows — the blame, the changed lines, the folds, the breakpoints
— and then reads the size off the settings:

```rust
font_size: self.settings.font_size,
```

An editor node has its own size. `task-1905` gave it one, and `show_an_editor_node` applies it to the tab's
**document** through `set_base_style`, remembering it as `OpenFile::sized_at`. So the letters change size and
the numbers beside them do not, which is exactly the report. It is the fault `set_the_font_everywhere` exists
to prevent, in the one place that reads the setting instead of the state.

**And the gutter's own type is clamped, so it stops following the text at both ends.**
`gutter::type_size` ends `.clamp(SMALLEST_TYPE, LARGEST_TYPE)` — 9 and 28 points — and the ratio is
`11.5 / 16`, so the numbers stop shrinking below about 13 points of text and stop growing above about 39:

| editor points | numbers want | numbers drawn |
|---:|---:|---:|
| 8 | 5.75 | **9.00** |
| 12 | 8.62 | **9.00** |
| 16 | 11.50 | 11.50 |
| 32 | 23.00 | 23.00 |
| 48 | 34.50 | **28.00** |
| 144 | 103.50 | **28.00** |

Both ends have a real reason behind them, which is why the clamp is not simply deleted.
`task-1693` put it there and said why: *"Six point text still needs a gutter somebody can read, and a hundred
and forty-four point text must not have a gutter wider than the editing area beside it."* Those are true.
What is wrong is that a **floor** on a number that is supposed to track another number is the thing that makes
it stop tracking — which is the same shape as the fault `task-1771` found in the ticket modal's heights,
where *"a floor is exactly the thing that makes a budget stop adding up."*

**What changes.** The two faults get two answers.

**The size comes from the file.** One line, and it is the line every other field in that function already
follows:

```rust
// **The size this file is really set in, not the window's setting.** An editor node on the canvas gives its
// own tab a size through `set_base_style` and records it as `OpenFile::sized_at`, so a gutter reading the
// setting drew eleven point numbers beside twenty point letters. `task-1907`. `None` is every tab in a pane,
// which is the setting, so nothing outside the canvas changes by a pixel.
font_size: file.sized_at.unwrap_or(self.settings.font_size),
```

**And the clamp becomes a clamp on the gutter's *width* rather than on its type.** What `task-1693` was
protecting against is a gutter that takes the window, and that is a statement about how wide the column is —
so it is measured there, where it can be true at every size, rather than approximated by a ceiling on the
point size. `gutter::width` already computes the column from the digit width; it now takes the room it is
being drawn in and reduces the type until the gutter fits within `GUTTER_SHARE` of it. The floor stays, at
the bottom end only, because a number nobody can read is not a number — but it is a floor on the *drawn* size
rather than on the ratio, so it engages when the type is genuinely tiny rather than at every size under 13
points.

Stated as the rule it is: **the numbers follow the letters, and the only thing that overrides that is the
gutter running out of room.**

**What proves it.**

- `the_gutters_numbers_follow_the_file_rather_than_the_setting` — with no window, on the two values: a tab with
  `sized_at` of 24 gets a gutter sized for 24 while the setting says 16. It fails on the code as it is.
- `the_gutters_type_tracks_the_editors_across_the_whole_range` — the table above, asserting the ratio holds at
  8, 12, 48 and 144 points rather than the clamped value. It fails on the code as it is at four of the six
  rows.
- `a_gutter_never_takes_more_than_its_share_of_the_pane` — what replaces the ceiling, checked at 144 points in
  a narrow pane, which is what `LARGEST_TYPE` was protecting.
- `an_editor_node_at_its_own_size_has_a_gutter_at_that_size` — through the harness, which is the report.
- `space_editor_node_gutter` — a screenshot of a node whose font size is not the window's, beside the pane
  showing the same file, which is the comparison the report makes.

## 5. A terminal node comes back running what it was running

**Reported.** *"My terminal sessions aren't opened back up. e.g. if i just have a view with a terminal with
claude-code open, then quit, re-open, the terminal is there but no claude code. I want the exact session as
though I never closed anything to begin with."*

**What is actually happening.** `task-1906` built the machinery for this and it cannot reach the case the
report is about. A node's `Terminal::command` is what the **node** was given — by `space add terminal
--command claude`, or by the add modal — and `start_the_canvass_terminals` resumes a node whose recorded
`session` is not empty, which `session_for` fills in only when `takes_a_session(command)` is true.

A person does not do that. They add a terminal node, which starts a shell with an empty command, and then
they **type** `claude` into the shell. Nothing about that reaches the node's state. Measured on a real window
— add a node, type into it, read the node back:

```json
{ "command": "", "folder": "/tmp/space-probe", "session": "", "kind": "terminal" }
```

which is byte for byte the shape of the reporter's own `.unluminous/space.conf`: a `terminal` node with a
folder, a size and a position, and no command and no session at all. So the node comes back as a shell,
because a shell is all it was ever recorded as being. **The views report is the same fault seen from further
away**: `store::save` writes a fresh `Values` every time, so the file is the whole truth about a canvas, and
what is missing from it was never written rather than lost in the reading.

**What changes, and the honest shape of it.** This cannot be made complete, and the parts that cannot are
named rather than glossed. What a program is doing when a window closes is not recoverable in general — that
is `project_state`'s own promise about the terminal tile, *"what comes back is the same number of shells in
the project's folder"*. What **is** recoverable is what was started and, for an agent that answers to a
session id, which conversation it was on.

### 5.1 What is running is read from the terminal rather than declared

`Session` already knows the answer and nothing was asking it. A pseudoterminal's child is a process with a
process id, and the program in the foreground of a terminal is the foreground process group of its
pseudoterminal — which is what a shell prompt in a tab title is derived from already.
`unluminous_terminal::Session::foreground` is that question, and it is asked where the canvas is already
asked to catch up:

```rust
/// The program in the foreground of this terminal, when it can be told.
///
/// **Read from the pseudoterminal rather than declared by the caller.** A node's `command` is what the node
/// was *given*, and a person adds a plain terminal node and then types `claude` into the shell — so a canvas
/// that recorded only the command came back as a shell whatever had been running in it. `task-1907`.
///
/// `tcgetpgrp` on the pseudoterminal names the foreground process group and `/proc` or `libproc` names the
/// program; a platform that cannot answer answers `None`, which is honest and is what the shell case wants
/// anyway.
pub fn foreground(&self) -> Option<String> { … }
```

Two rules about it.

**It is a note about what to offer, not a command to run blind.** What comes back is a program name, and a
name is not a command line: the arguments, the working directory changes somebody made with `cd`, and
anything typed after it are all gone. So `Terminal::running` is recorded beside `command` rather than
replacing it, and what a restored node does with it is decided in §5.3.

**And it is read on the same schedule as everything else in `task-1906`.** `note_where_the_nodes_are_reading`
already walks the nodes on the frames after `brought_to_life` matches, comparing before it writes so the
canvas is not marked dirty sixty times a second. Reading the foreground program is one more field in that
walk, and it is compared the same way.

### 5.2 An agent gets a conversation id whether or not it was the node's command

`session_for` asks `takes_a_session(command)` and the command is empty, so a typed `claude` never got a
`--session-id` and there is nothing to resume. That cannot be fixed by reading harder: the id has to be
**given** to Claude, and by the time somebody has typed `claude` the process is running without one.

So the answer is at the other end, and it is the one the ticket's own words point at — *"the exact session as
though I never closed anything"*. `claude` writes its conversations down itself, and it can be asked for the
most recent one in a folder. `agent::last_conversation_in(folder)` is that question, and a restored node whose
`running` says `claude` and whose `session` is empty is offered `--continue` rather than nothing.

**`--continue` rather than `--resume <id>`**, and the difference is the whole of why this is honest.
`--resume <id>` names a conversation Unluminous chose and gave, which is what `services::agent_tasks` does and
what a node whose *command* is `claude` will go on doing. `--continue` asks Claude for the most recent
conversation in that folder, which is a question Claude answers about its own records rather than one
Unluminous answers from a file it wrote. It is weaker — two nodes in one folder come back on the same
conversation, and a conversation continued elsewhere since is the one that comes back — and both of those are
said in the node's own header rather than left to be discovered.

**Codex is unchanged and the reason is unchanged.** `agent::why_it_cannot_resume` already records it: Codex
names its own sessions, so an id here is a marker rather than something it answers to. A restored Codex node
starts a new conversation and says so.

### 5.3 A restored node offers rather than assumes

A node coming back with a program name and no command line cannot simply run it, and this is the decision the
section turns on.

**A shell node comes back as a shell, with its program offered on the node.** The node starts the same way it
always did — the machine's shell, in the same folder — and its header carries what was running when the window
closed, with one button: `Start claude again`, or `Continue the conversation` when §5.2 can offer that. One
press, and it goes through `start_a_space_terminal`, which is the same path `Resume session` already uses.

Three reasons it is an offer rather than an action, and the first is the one that decides it:

- **A program name is not a command line.** Running `claude` when what was running was `claude --model opus
  -p something` is running a different thing and calling it the same. An editor that silently ran a
  half-remembered command is worse than one that says what it saw.
- **A shell is somebody's shell.** A node whose terminal starts by running a program is a node in which the
  prompt somebody expected is not there, and there is no undo for a program that starts.
- **And it is the shape the canvas already has.** `Resume session` is on the node's menu because
  `task-1906` decided a restored agent may need starting by hand; this is that control given something to
  say before it is pressed.

**A node whose *command* is an agent is unchanged and still resumes by itself**, because there the command
line is known exactly and `task-1906`'s reasoning holds in full. So the two cases differ in how much is
known about them, which is the only honest basis for them to differ.

### 5.4 And what a node was left holding is written before the window goes

The three fields §5 adds — `running`, and the two things §5.2 needs — ride `Space::change`, so they mark the
canvas dirty and are written by `write_the_space_if_it_changed`, which `on_exit` calls with `f64::MAX` so a
pending write cannot be lost. That is already right and is not changed.

What is added is the guard `task-1906` §4.5 had to learn: nothing derived from the live state is written
before `brought_to_life == current_id()`, because before the nodes are running that state is empty and writing
it puts an empty list over the file. Reading a foreground program is exactly that shape — a node with no
session yet answers `None` — so it is guarded in the same place and by the same question.

**What proves it.**

- `a_terminal_node_records_the_program_running_in_it` — through the harness with a detached session, because
  `Session::detached` is what makes a terminal testable with no shell: fed bytes that leave a known foreground
  program, the node's `running` says so.
- `a_platform_that_cannot_tell_answers_nothing_rather_than_guessing` — the `None` case, so a node on a platform
  with no answer is a plain shell rather than a node claiming something.
- `a_restored_shell_node_offers_what_was_running_rather_than_running_it` — the control, and the assertion that
  the node really did start a shell.
- `a_node_whose_command_is_an_agent_still_resumes_by_itself` — `task-1906`'s behaviour, unchanged, which is what
  says this did not widen into it.
- `the_offer_is_a_continue_when_there_is_no_recorded_conversation` — which of the two commands a restored node
  builds, asserted with no process.
- `everything_a_node_was_left_holding_comes_back` — extended with `running`, on a folder of its own, which is
  `task-1906` §4.8's rule: a test that calls `restore_project` needs a folder of its own, always.
- **And the real window**, which is the layer this can only be answered at: a canvas with a terminal node
  running `claude`, closed and opened, and the offer pressed. `verify-before-saying-done` is the rule.

## 6. Everything on a canvas is written down, and the file says what is missing

**Reported.** *"When i close and open the project, my views in Base of Infinite Space are not restored to what
they were."*

**What is actually happening.** This is §5's fault seen from further away rather than a separate one, and the
reporter's own file is the evidence. `store::save` builds a fresh `Values` on every write, so
`.unluminous/space.conf` is the whole truth about a canvas — nothing is merged and nothing survives from an
earlier write. The file held:

```
space.current = 57
space.view.0.id = 57
space.view.0.name = View 2
space.view.0.node.0.kind = terminal
space.view.0.node.0.folder =
space.view.0.node.0.font = 20
space.view.0.node.0.x = 628.4      ... width, height, id
```

One view, one node, and about that node: where it is, how big it is, what folder it starts in and what size its
type is. No `command`, no `session`, no title. So the canvas came back exactly as recorded — the position, the
size and the font all restored correctly — and what was missing had never been written. **Nothing is wrong with
the reading**, which is what makes this §5 and not a bug in `store`.

`task-1906` §4 fixed the three fields that were declared and never filled — `Terminal::session`,
`Editor::caret` and `Editor::scroll` — and that work is in the tree. What it did not reach is the field nobody
had thought to declare, which is what is actually running.

**What changes, and it is one thing rather than a list.** The state a node holds is what a person can see about
it, and the way to keep that true is to stop deciding it twice. So there is one test that says so, and it is the
one worth writing before any of the code:

```rust
/// Every kind of node, given every piece of state it can hold, comes back holding all of it.
///
/// **Written from `Kind::ALL` rather than from a list of fields**, so a kind added later fails this until it is
/// written down — which is the fault `task-1907` is: `Terminal::running` was not a field anybody had forgotten
/// to write, it was a field nobody had declared, and no round trip test over the fields that existed could have
/// found it. This is `every_command_is_offered_as_a_tool_in_both_shapes` applied to a canvas.
#[test]
fn every_kind_of_node_round_trips_everything_it_holds() { … }
```

It cannot check a field nobody has declared either — nothing can — but it makes the list of what a kind holds
one place rather than three, so the next kind is written down when it is added rather than when somebody
reports it.

**And the offer in §5.3 is what makes the incompleteness visible.** A canvas that cannot bring a program back is
going to exist — a shell that had `vim` open in it, a program with arguments nobody recorded — and the answer
is a node that says what it saw rather than a node that looks restored and is not. That is the difference
between this and the report.

**What proves it.**

- `every_kind_of_node_round_trips_everything_it_holds` — above.
- `a_canvas_written_down_and_read_back_is_the_same_canvas` — the existing round trip, extended with `running`.
- `a_space_conf_written_by_the_previous_version_opens_unchanged` — a file with no `running` key in it, because
  every one of these on a real machine will be that file. `Layout::read_from`'s own rule.
- `a_hand_edited_space_conf_cannot_panic` — the rule `task-1906` established for `showing`: this is a text file
  a person can edit, so an unknown program name, an empty one and a very long one all have to answer with
  something.

## 7. What is deliberately left out

- **What a program was doing, as opposed to which program it was.** A shell's scrollback, a `vim` session's
  unsaved buffer, a REPL's variables. §5 says why: that is not recoverable, and `project_state` made the same
  promise about the terminal tile — *"what comes back is the same shell in the same folder"*.
- **A command line rather than a program name.** `tcgetpgrp` names a process and the platform names the program
  behind it; the arguments would be a second platform-specific read (`sysctl` with `KERN_PROCARGS2` on macOS,
  `/proc/<pid>/cmdline` on Linux), and a command line reconstructed from a running process is a different
  feature with its own quoting problems. The offer says the program, which is what makes it an offer.
- **Reading the foreground program on Windows.** §5.1: a ConPTY is a pipe rather than a controlling terminal, so
  there is nothing to ask. The control is absent there, which is Unluminous's own rule, and the day someone wants
  it the shape is a process tree walk from the child handle `Reaper` already holds.
- **Sharp galley text inside a node.** §3.2: `epaint` offers no per-layer `pixels_per_point`, and the two things
  somebody reads in a node go through Unluminous's own atlas and are fixed. The gutter and a folder node's rows
  stay scaled, and the note in `vello_canvas` about following epaint applies here when epaint moves.
- **A caret and a scroll per tab on a node.** `task-1906` §7 already refused it and the reason holds:
  `open-files.txt` keeps one per tab for the panes, and a second list for the nodes would be a second thing to
  keep in step.
- **A canvas that outlives its project.** `task-1906` §7 again — a canvas names a project's files, so a space
  opened elsewhere is a canvas of broken nodes.
- **Making the two divider paths one.** §1 says the sharing path would handle both and explains why they are
  kept apart: 554 accepted screenshots were taken through the plain one, and a divider not against a wall should
  go on costing one clamp.

## 8. Tests

Four layers, as everything here has.

1. **Unit tests with no window.** The divider's two-number comparison; the browser's address normalisation and
   the tab that records where it was sent; the crispness arithmetic and its quantising; the gutter's type
   against the file's size and its width against the pane; `store`'s round trip per kind; which command a
   restored node builds.
2. **`unluminous-app` unit tests.** `Session::foreground` against a detached session, which answers `None`, and
   the guard that stops a derived field being written before the canvas is alive.
3. **Screenshot tests**, each through `builder()` and never `Harness::builder()`, for `task-1654`'s reason.
   Three new pictures: `space_zoomed_in`, `space_zoomed_out` and `space_editor_node_gutter`.
4. **The real window.** The layer four of the six reports can only be answered at, and the layer they were all
   written from. A canvas with a terminal node running `claude`, a browser node, a folder node and an editor
   node at its own font size; the canvas zoomed to 150% and 200% and photographed; the divider dragged both
   ways with the board above it; then Unluminous closed and opened and every one of those checked by hand.
   `tools/drive-a-window.sh` is how, and it never takes the keyboard focus.

**And a test that calls `restore_project` gets a folder of its own, always.** `task-1906` §4.8 learned this the
hard way: `sample_folder()` is one folder behind a `OnceLock` that every test wanting a project shares, so a test
that writes a `space.conf` into it changes what every later test in the same process restores.

## 9. What the implementation changed about this design

Four things came out differently from §1 to §6, and each is recorded here rather than by quietly editing the
section above it, because each is a thing the next person would otherwise wonder about.

**§1's fix needed a second half nobody predicted.** Moving the gate from *is the editing area hidden* to *are
the stored sizes the drawn ones* was necessary and not sufficient: the sharing path takes what one side gains
off the side **facing** it, and a strip whose facing side has no panels on it had nothing to take from, so
`givable` was zero and the drag was still clamped to nothing. The editing area is what is between them and is
exactly what should give the room up, so `from_the_editor` — its drawn size above `EDITOR_MIN_HEIGHT`, read off
`panel_rects.editor` — is added to `givable`.

**And one of §0's measurements turned out not to be a fault.** The canvas alone along the bottom of a 670 point
window is drawn at 550 and asks for 560, so it is against its maximum the moment it opens and dragging up
correctly does nothing. The test written for it asserts the case below the wall instead — down 150, then back up
150 — and the report's real case is the two-panel one, which moved 2.4 points of 120 before the fix.

**§3's crispness is ambient on the renderer rather than an argument.** `paint_text` has five callers and the
terminal grid has four, and threading a number through all nine to be 1.0 in eight of them is nine chances to
pass the wrong one. It is also genuinely a property of *where the drawing lands* rather than of what is being
drawn. `TextRenderer::composite_at` and `restore_compositing` are the pair, called around
`show_a_node_body` — **not** a `Drop` guard, which was tried and cannot work: a guard borrows the renderer for
the scope and every node drawing function takes `&mut self`.

**§3 also gained a rule the design did not state: a canvas zoomed *out* changes nothing at all.**
`Crispness::at` never goes below 1.0, because rasterising smaller than the layout and magnifying back up is the
very fault being fixed. That is what makes `the_canvas_zoomed_out` — an accepted picture at a camera of 0.4 —
still match its original snapshot byte for byte, which is the strongest evidence available that the change is
confined to the case it is for.

**§4's ceiling became a limit on the column rather than on the type.** `gutter::fitted_size` takes the room the
pane has and reduces the type only when the gutter would take more than `GUTTER_SHARE` of it, so
`LARGEST_TYPE` is gone entirely. `task-1693`'s requirement is kept and is now true at every size rather than
approximated by a number that also broke the tracking at ordinary sizes.

**§5's offer types the program into the shell rather than becoming the node's command.** The node *is* a shell
— that is what it was — so replacing the shell would throw away the shell somebody had, change the node's
`command` away from what they configured, and leave nothing to come back to when the program ends. Typing it is
exactly what the person did in the first place. `space restart <node> --running` is the agent's half of the
same function, which is the rule that a thing done by hand and the same thing done by an agent are the same
thing.

**And §5.1's mechanism was measured before it was written, which is why it exists at all.** `Session::kill`
records that *"`alacritty_terminal` keeps the pseudoterminal inside its `EventLoop`, so there is no child handle
here"* — so the first question was whether the master could be reached at all. It can, in `Session::spawn`,
between `tty::new` answering and `EventLoop::new` moving the `Pty`, which is the same window `Reaper::adopt`
takes the Windows handle in. `cargo run -p unluminous-terminal --example foreground_check` is the measurement,
kept as a runnable example rather than as a number in a comment:

```text
at the prompt: Some("bash")
while sleeping: Some("sleep")
after interrupt: Some("bash")
```

## 10. What the Codex Sol review found

Ten findings, and they are recorded with their verdicts rather than only the ones that were acted on: a
finding that was checked and did not reproduce is worth as much as one that did, because the next person
will have the same worry.

**Six were real and are fixed.**

- **A shell overwrote a recorded program, and the protection against it never ended.** Two faults in one
  place. `running` was being written as `zsh` for a node at a prompt, which is a node offering to start the
  shell it is sitting in; a shell is written down as **nothing** now, so the field means "a program somebody
  ran". And the guard that stops a restored node's own shell clearing what the file held is bounded by
  `Live::has_been_used` — otherwise a program somebody deliberately quit would be offered on every restart
  from then on. The comment referred to a function that was never written, which is what a reader would have
  hit next.
- **The offer was available while the program was still running.** A node whose `claude` is running is a
  node whose terminal is Claude's, so typing `claude --continue` into it types those words *at the agent*.
  It is refused unless the terminal is at a prompt.
- **A program started in the last three quarters of a second before a window closed was never written
  down.** The reading is throttled to `WATCH_INTERVAL` and `on_exit` kills the sessions before writing, so
  there was nothing left to ask. `note_what_the_nodes_are_running_now` takes one last reading first.
- **A redirect left a browser tab loading for ever.** `arrived_at` ignored a page that finished at a
  different address from the one asked for, which is right for a *step* — there the view is reporting the
  page it is leaving — and wrong for an address somebody typed. Measured on a real window before the fix:
  `http://github.com/` was still reported as the tab's address, still loading, long after the page had
  settled on `https://github.com/`. `Awaited::typed` is what tells the two cases apart, and the destination
  replaces the address that redirected rather than being pushed after it, because a `Back` to an address
  that only ever answered with a redirect would redirect again. **This one predates `task-1907`** and §2 is
  what made it reachable from a browser node.
- **A strip equalising its panels was read as a side being shared.** `dock::regions` draws every panel in a
  strip at the deepest one's depth, so a 300 point panel beside a 420 point one is *drawn* at 420 — and an
  absolute comparison read that as scaling, sent an ordinary drag down the sharing path and rewrote the
  shorter panel's stored height. `share_the_depth` only ever scales **down**, so the question is whether a
  panel is drawn *smaller* than it asked for.
- **The gutter's cap did not hold.** `CHANGE_BAR`, `GAP` and `NUMBER_MARGIN` are fixed points that do not
  shrink with the letters, so scaling the type by the ratio the *whole* width overshot by lands short:
  measured, a 400 point pane wanting 343.5 points of gutter came out at 141.5 against a cap of 120.
  `size_that_fits` scales by the ratio the **digits** overshot by, which is the part that really is
  proportional, and lands on 120 exactly. It is a pure function so the case can be tested at a pane width no
  screenshot window can be.

**One was a real fault in the design as written, and the fix is the opposite of what shipped first.**
`Crispness::at` rounded the scale to the **nearest** quarter, and the canvas zooms in steps of 1.1 — so a
person's very first zoom step became 1.0 and the glyphs were magnified exactly as before. It rounds **up**
now, so a glyph is rasterised at at least the size it is composited at and the transform only ever scales it
down. `a_glyph_is_never_rasterised_smaller_than_it_is_composited` walks the whole 1.1 ladder.

**One was already fixed while the review was running.** The block cursor's second copy of the character
under it still rounded to whole points; it uses `Crispness::snap` like the grid it sits in.

**And two did not reproduce, each checked rather than argued about.**

- **The duplicated pseudoterminal descriptor does not stop a program being killed.** The worry was exact:
  on Unix `Reaper::kill` does nothing, so if ending a program depended on the pseudoterminal closing, a
  second descriptor held open would keep it alive. Measured both ways — with the duplicate and with it
  removed — a `sleep 300` behind a session that was killed and **deliberately kept alive**, which is the run
  tile's own case, was gone both times. The mechanism is `alacritty_terminal`'s own `impl Drop for Pty`,
  which sends `SIGHUP` to the child **by process id** rather than relying on the descriptor.
  `cargo run -p unluminous-terminal --example kill_check` is that measurement, kept.
- **A real navigation failure on the showing tab is not silently lost.** The `let _ =` on
  `BrowserHost::navigate` is there for the one native view refusing a tab that is not showing, which is not
  an error the caller can act on. A genuine `load_url` failure leaves the tab `loading` with its address
  recorded as where it should be, which is what the reconciliation in `raw_input_hook` acts on when the node
  next renders — the same path a node that was not showing takes. It is weaker than reporting the failure and
  that is stated rather than hidden: what a tab knows is where it should be.

**The review also confirmed four things by checking them**, which is worth recording because each was a
design decision rather than an accident: the ambient crispness really is restored across every ordinary
early return; the layout and measurement paths — `advance`, `line_metrics`, `cell_metrics` — do not read it,
so a zoom cannot relayout; the divider's signs are right on all four sides; and the `proc_name` extern, its
buffer and its `unsafe` are sound.

## 11. Two faults the review's own fixes introduced, found by driving the build again

Both are about *when* something is read rather than what it says, and neither would have failed a test — they
are the sixth and seventh time in four tickets that the installed binary found something no reasonable test
would.

**`taps` answers "this node exists", not "somebody has used it".** The bound put on the shell guard asked
`Live::has_been_used`, which was `taps.contains_key` — and `follow_from_here` inserts a tap the moment a
session starts, so every restored node looked used and the protection never applied at all. Measured:
`running = sleep` survived the window closing and was gone a second after it reopened. `Live::typed` is a set
of its own now, added to by `typed_into` and by nothing else, so the question is about lines somebody really
sent.

**And the offer forgot itself.** Taking it types the program, which marks the node used and so ends the
protection — and the program takes a moment to start, so the very next reading three quarters of a second
later saw a prompt and cleared the field. The offer worked exactly once and then the node had nothing to
offer again. It records the program it has just been told to start, and the ordinary reading takes over from
the next tick.

**One thing verification confirmed rather than found, and it is the limitation §5 already states.**
`space restart --running` on a node that was running `sleep 200` starts `sleep` with no argument, which exits
at once — because what is written down is a program **name**. The same cycle with `cat`, which needs no
arguments, works end to end: recorded, survived a close and reopen, restarted by the offer, still recorded
afterwards. That is the honest shape of the feature and it is why the control is an offer that names the
program rather than something a restore does silently.

## 12. Three reports against 0.41.0, and the first one is the whole of §5 defeated by a file name

Driving the released build found three more things. The first is the sharpest lesson in the ticket.

### 12.1 macOS has three names for a process and two of them are wrong

**Reported.** *"It's still not opening my terminal session back up when I quit and reopen."*

**What was actually happening.** §5 worked. `space.conf` held, in the reporter's own project:

```
space.view.1.node.0.running = 2.1.269
```

A version number, offered as a program. Claude Code's executable is
`~/.local/share/claude/versions/2.1.269`, and `proc_name` answers with the **executable's file name** — so the
one program the whole section exists for was the one program it could not name. Everything else about §5 was
right: the reading, the persistence, the offer, the guard. The name was wrong.

**Two wrong answers before the right one**, which is why they are written down as a table rather than as a
sentence. Measured against a real `claude` on this machine:

| asked for | answers | what it really is |
|---|---|---|
| `proc_name` | `2.1.269` | the executable's file name |
| `p_comm`, which is `ps -o ucomm=` | `2.1.269` | the same name, truncated to sixteen characters |
| **argv[0]**, which is `ps -o comm=` | `claude` | the word that was typed |

So `argv_zero` reads `KERN_PROCARGS2`, which is where `ps` reads it from. Its buffer begins with a count of
the arguments, then the executable path, then NUL padding, then argv[0] — and the last component of that is
taken, because a command line very often names a program by its path and `-zsh` is a login shell wearing a
dash.

**Two things about the reading were measured rather than reasoned about.** `proc_bsdshortinfo` is **64** bytes
with `pbsi_comm` at offset 16 and a length of 16, not 17 — a byte out made the struct 68, `proc_pidinfo` wrote
64, the length check refused it and every node answered that nothing was running. And `KERN_PROCARGS2` fills
its buffer with the arguments *and the whole environment* and refuses with `ENOMEM` rather than truncating, so
4,096 bytes was enough for a `/bin/zsh` (1,004) and not enough for a `cargo test` binary. It asks
`kern.argmax`, which is what `ps` does.

**The lesson worth keeping is about the test.** A test that checked "some name came back" passed on all three
answers. What found this was reading the reporter's own `space.conf` — and what would have found it earlier is
asking the question about `claude` specifically, which is the program the feature is for.

### 12.2 A page followed to a new address was not what the node came back on

**Reported.** *"I clicked and navigated to a url from hacker news, but when it reopened it was back at hacker
news."*

**What was actually happening.** Two values, and only one of them was written down.

| | value | lives in | moves on a link click? | written to `space.conf`? |
|---|---|---|---|---|
| the tab | `BrowserTab::current_url` | `space.live.browsers`, dropped on exit | **yes**, through `arrived_at` | no |
| the node | `Browser::url` | `space.space`, the model | **no** | **yes** |

`Browser::url` was written only when something *sent* the node somewhere, and then went stale for the life of
the window while the tab moved underneath it. The toolbar and `space browser url` both read the **tab**, so
everything looked right until a restart — which is why this read as a save fault rather than a navigation one.

**And `note_where_the_nodes_are_reading` had `Kind::Browser => {}`**, in a function whose own doc comment
states the rule it was breaking: *"derived rather than reported… a list of the places that have to remember to
write it down is a list whose next entry is the one that forgets."* Browser was the entry that forgot.
`note_where_a_node_is_browsing` is that arm, derived the same way the caret and the folder scroll are — so a
redirect, a `Back` and a `Forward` are all covered by one reading rather than three that have to remember.

It joins the `on_exit` pass too, which is now `note_what_the_nodes_hold_now` rather than
`…_are_running_now`: a terminal's program and a browser's address are both read from something live, and the
last click before a window closes can land after the last frame that read it.

### 12.3 A page cut into by the pane's edge was reflowed rather than cropped

**Reported.** *"If the node itself is 50% off the page/view, the full browser page is shown but resized to 50%
width."*

**What is actually happening, and it is a limit rather than a mistake.** `wry` offers `set_bounds` and nothing
else — there is no clipping a native child and no handle to clip it with, on either platform. So a node
hanging off the edge has its view's **viewport** narrowed, and a page laid out against the viewport reflows
into it. `task-1905` recorded this as the honest cost of a native child; `task-1907` reports what it actually
looks like, which is a page redrawn at a width nobody chose.

**So past `PAGE_CROP` the page is not drawn.** Nine tenths of its width, and the node keeps its toolbar and
says the page is showing elsewhere — which is the sentence `browser_view::show` already says for a second
rendered tab, reached by not pushing a placement rather than by a second mechanism. Nine tenths rather than
all of it, because a node a few points off the edge going blank as it is dragged would be worse than one that
goes when it is genuinely being cut into.

**And it is a page rather than a picture of a page that is being given up**, which is worth being plain about:
this trades a reflowed page for no page. The reflowed one was not a picture of the page the node is on, and a
node that says where its page is can be dragged back; there is no third answer while `set_bounds` is the only
lever.
