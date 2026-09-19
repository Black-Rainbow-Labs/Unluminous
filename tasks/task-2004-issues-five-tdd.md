# task-2004 — eight reports, and what each one really is

`task-2004` is eight reports against the installed build plus one sentence that arrived while the work
was starting. They look unrelated and three of them turn out to be the same fault seen from different
sides, so this document says what each one measured, what causes it, and what is going to change.

The reports, in the ticket's own words:

1. *"when the base of infinite space is open on windows, i can't resize the main window. I've already
   reported this before and it was allegedly fixed. recreate the issue, ensure that it is truly fixed.
   it's intermittent."*
2. *"I also have problems resizing the terminal pane to be taller. it shrinks just fine, but with Base
   of Infinite Space pane above it, i can't resize it. We need extensive tests that ensure resizability
   of our panes in different configurations, with you evaluating the screenshots to ensure they look as
   they should."*
3. *"Filter files in folder pane is too large, and the blinking cursor is too tall … ensure that the
   cursor fits the height of the input and that the 'Filter files' text is about the same size as the
   folder/file name text. Same for search settings modal input. Find other inputs that may have the same
   issue."*
4. *"File menu should have a 'Create project' option that allows me to type in a project name, show/set
   the default location, and checkbox to git init, similar to Intellij."*
5. A Background Selector on the Appearance page: a grid whose first option is the current behaviour and
   whose other options are pictures chosen from disk, copied into the application's own folder, each with
   a way to remove it, applied at once.
6. *"Agent's in the base of infinite space don't seem to have the cli, or don't understand the unluminous
   cli."*
7. *"The browser node … when i zoom in/out of the canvas, it jitters as it zooms in/out the content to
   match. We want that to be silky smooth, without breaking the functionality."*
8. *"The + sign to add a new view to base of infinite space should be on the right of the last view/tab,
   similar to Firefox browser."*

---

## 1. The window cannot be resized while the canvas is open

### What it is

`task-1945` measured that a browser node's page takes the operating system's keyboard focus, and that
`winit` then answers `Window::has_focus()` with false. `ViewportCommand::BeginResize` reaches winit's
`handle_os_dragging`, which latches a flag that only `WM_EXITSIZEMOVE` clears and returns early from
every later move and resize for the life of the process — so one refused request wedges the window
permanently. `components::resize_edges` records the measurement.

The fix that shipped was a refusal: `app::frame::show_the_resize_grips` reads
`input.viewport().focused` and sends nothing while it is false. That stops the wedge, and it is also
exactly the report. **With a browser node on the canvas holding the focus, every one of the eight grips
is dead, silently, for as long as the page has the keyboard** — and it is intermittent because it
depends on whether a browser node has been clicked since the window opened. The title bar's drag goes
the same way, because `egui-winit` refuses to forward `StartDrag` under the same condition.

So the report is not that the `task-1945` fix failed. It is that the fix answered "do not wedge the
window" and left "and the person can still resize it" unanswered.

### What changes

**The refusal is stated in terms of the thing that causes it.** The window asks its own browser host
whether a page holds the operating system's keyboard, rather than asking `winit` whether the window has
it. Those are the same answer in the case the guard exists for and different answers everywhere else: a
window that is genuinely in the background has `focused == false` too, and there the grips were dead for
no reason at all.

- `services::browser::BrowserHost::page_holds_the_keyboard() -> bool`, read off the one
  `NativeView::has_the_focus` flag the host already keeps.
- `show_the_resize_grips` sends `BeginResize` unless that is true.

**And a press outside a page hands the operating system's keyboard back.** `the_focus` decides what the
native view does about the focus from `placement.focused`, which means *this node is the chosen one*.
Pressing the title bar, a resize grip, the menu bar, the rail or the status bar does not change which
node is chosen, so the page kept the focus through all of them. The rule becomes:

> The page holds the operating system's keyboard while it is the chosen node **and** the last press
> landed on it.

A press is seen in `raw_input_hook`, before the pass, where the placements the last frame drew are
already in hand — which is where the native views are settled anyway. `services::browser::the_page_was_pressed`
is the decision and it is a pure function with the placements and the press position as its arguments.

Both halves are needed. The first makes the grip work on the frame the press happens, when winit still
believes the window has no focus; the second makes winit agree by the next frame, which is what the
title bar's own drag needs.

### How it is checked

`BeginResize` goes to the window manager, so a test cannot watch a window change size. What a test can
watch is **what the window asked for**, so `UnluminousApp` records the last direction it requested and
`unluminous-cli status --section window` reports it beside `focused` and `maximised`. The tests then
drive a canvas with a browser node, tell the window the page has the focus, drag each of the eight
grips, and assert the request was made. A maximised window still asks for nothing, which is
`task-1693`'s rule and has a test of its own already.

On the real window the reproduction is `status --section window`: open the canvas, add a browser node,
press into the page, and read `focused`. Before this change it answers `false` and the grips are dead.

---

## 2. The terminal cannot be dragged taller, and four other panes cannot either

### What was measured

A sweep over every arrangement of the canvas and the terminal, dragging one divider 150 points and
reading back the rectangle the frame really drew. `crates/unluminous-app/tests/panel_docking.rs` holds
it now; this is what it reported against the code as it was:

| arrangement | divider | wanted | got |
|---|---|---|---|
| canvas and terminal in the bottom strip | terminal taller | 150 | **0** |
| canvas on the left, editing area hidden, terminal along the bottom | terminal taller | 150 | **0** |
| canvas on the right, editing area hidden, terminal along the bottom | terminal taller | 150 | **0** |
| canvas on top, terminal below | canvas deeper | 150 | **84** |
| canvas on the left | canvas wider | 150 | **0** |
| canvas on the right | canvas wider | 150 | **63** |
| canvas alone along the bottom | canvas deeper | 150 | **0** |
| terminal alone | terminal taller | 150 | 150 |
| explorer, with or without the canvas | explorer wider | 150 | 150 |

Shrinking works in every one of them, which is the report word for word.

### Cause one: the band between the strips is never asked for room

`app::panels::move_a_divider_by_sharing` grows a side by taking the room off the side facing it, and —
since `task-1907` — off the editing area as well. It asks for nothing else. With the editing area
**hidden** it therefore has two sources and both are empty: there is no facing side, and
`from_the_editor` is zero by construction.

But with the editing area hidden and a panel docked left or right, there is a **band** between the two
strips holding those columns, and `dock::COLUMN_BAND_MIN` is all that has to be kept in it. Measured: a
670 point body with the canvas as a left column 410 points deep and the terminal 260 points along the
bottom. The terminal could take 290 points and took none, because nothing in the arithmetic knew the band
was there.

`from_the_editor` becomes `from_the_middle`, and it answers about whatever is between the strips:

- the editing area showing — what it is drawn at above `EDITOR_MIN_HEIGHT`, as before;
- hidden, with columns in the band — the band's own height above `COLUMN_BAND_MIN`;
- hidden, with nothing in the band — nothing, because `dock::fill_the_depth` has already given the
  strips the whole height and there is genuinely no more.

The band's height is **read off the rectangles the frame drew** rather than computed a second time,
which is `the_sizes_are_being_shared`'s own rule: a column is laid out over the whole band, so the
deepest showing column is the band.

### Cause two: the editing area's floor is 120 points and a drag cannot pass it

`dock::EDITOR_MIN_HEIGHT` is 120 and it is two things at once: what `dock::regions_with` keeps for the
editing area when the panels ask for more than there is, and — because the layout would take it straight
back — the limit a person's drag can reach. A 120 point editing area is a 32 point tab strip and about
five lines, and with the canvas along the bottom at its own default depth the window arrives at that
limit immediately: the canvas is drawn 550 points of a 670 point body and cannot grow by one point.

The two cannot be different numbers. A drag that grew a panel past what `share_the_depth` will allow
would be scaled back on the next frame, so the stored size would move and the drawn size would not,
which is the shape of the `task-1907` fault. So there is one number and it goes down.

**`EDITOR_MIN_HEIGHT` becomes the tab strip and two lines** — 72 points — which is the same promise
`size::EDITOR_PANE_MIN` makes about width, said about height: enough to still be an editor rather than a
stripe. `COLUMN_BAND_MIN` stops being defined as `EDITOR_MIN_HEIGHT` and keeps its own 120, because what
it protects is a different thing and its own comment already says so: *"enough of a panel to be worth
drawing"*.

The width is left at `size::EDITOR_PANE_MIN`. A 160 point editing area is already the narrowest a single
pane may be dragged to, and a person widening a column past it is asking for the editing area to go
rather than to be narrower — which `View -> Toggle Editor` is for. So "canvas on the left, canvas wider"
stops at that floor deliberately, and a test says so.

### Cause three: a drag that moves nothing rewrites what it could not move

`move_a_divider_by_sharing` writes every panel's **drawn** measurement into its stored one before it
works out what to give, so that the proportional share is the identity. Two things are wrong with doing
that first.

A drag that turns out to be able to move nothing still leaves the stored sizes rewritten. Measured: the
canvas alone along the bottom, stored 560 and drawn 550, dragged up 150 — the drawn rectangle did not
move and the stored height became 550. A drag that does nothing must change nothing.

And in a **strip** every panel is drawn at the deepest one's depth, because
`dock::lay_a_strip_out` equalises them — so the terminal, stored at 260 beside a canvas at 560, had its
own height rewritten to 550 by a drag that never moved. Hide the canvas afterwards and the terminal comes
back 550 points tall. Only a panel being **shared** — drawn *smaller* than it asked for — has a stored
number that is a share rather than a size, and only those are written back. That is the same asymmetry
`the_sizes_are_being_shared` already reasons about and it is the same sentence: `share_the_depth` only
ever scales down.

### The tests the ticket asks for

*"We need extensive tests that ensure resizability of our panes in different configurations, with you
evaluating the screenshots to ensure they look as they should."*

`panel_docking.rs` grows a sweep: for the canvas docked to each of the four sides, with the editing area
showing and hidden, every divider that arrangement draws is dragged in both directions and the
**rectangle the frame really gave each panel** is read back. Growing moves the divider to the pointer, or
to a limit the test names; shrinking moves it back. Seven of them take a picture, which is what somebody
looks at.

What is asserted is the drawn rectangle rather than the stored number, for the reason cause three exists:
the two came apart and only one of them is what a person sees.

---

## 3. A field's text is the interface's size inside a box that is a fixed height

### What it is

`components::explorer` draws its filter box `view.at(24.0)` points tall and sets the words in
`TextStyle::Body`, which is `appearance.ui.font.size`. The rows below it are
`FontId::proportional(view.at(12.5))` and do not follow that setting at all. On a machine whose interface
is set large the two are wildly different sizes, which is the screenshot on the ticket — and the caret is
the row height of the larger font, so it is taller than the 24 point box it is drawn in.

`components::settings_dialog`'s search box is the same shape: 26 points tall, rows at 12.5, text at the
interface size.

The same pair — a fixed height and the interface's font — is every field built from
`controls::search_field`, which is the explorer's own filter, `Go to File`, `Find in Files`, the command
palette, the debug tile's watch box, the canvas's Add Node and Find a Space modals, the Agent-Tasks
search and the database tree's filter; and `modal::field`, which is every dialog field in the window.
`controls::field_text_rect` measures the strip with `TextStyle::Body` for all of them, so the strip and
the caret grow with the setting while the box does not.

`task-1914` already fixed one instance of this by hand — the browser node's address bar, which draws at a
size of its own and was handed a strip measured for another — and left `field_text_rect_at` behind for
callers that know their own size. What is missing is the rule.

### What changes

**A field sets its text at the size its own height can hold, and never larger than the interface's.**
One function, `controls::field_font_size(ui, height)`, and `field_text_rect` measures the strip at the
row that font really occupies, so the box, the words and the caret are all measured the same way.

```rust
/// How much of a field's height its letters may occupy.
const FIELD_TEXT: f32 = 0.52;

pub fn field_font_size(ui: &egui::Ui, height: f32) -> f32
```

0.52 is chosen so that the 24 point filter box asks for 12.5 points — exactly the size the rows beside it
are drawn at, which is what the ticket asks for in as many words — and so that a 26 point settings box
asks for 13.5 and is capped by the interface's own 12.5 at its default. Every field that already fitted
is unchanged at the default interface size, which the accepted pictures are what check.

Capping at the interface size rather than scaling with it is the point: the explorer's rows, the settings
list's rows and every other row these boxes sit above are fixed sizes that `appearance.ui.font.size` does
not reach, so a field that grew alone was the only thing in the pane that moved.

The sweep is every `TextEdit` in `components/` — twenty five of them. The shared ones,
`controls::search_field_over` and `modal::field`, cover most; the rest are named in the commit.

---

## 4. `File -> Create Project...`

A modal of its own, `components::new_project_dialog`, because the ticket asks for three controls and
`components::prompt_dialog` is one field. It is built from `components::modal` like every other dialog,
so the frame, the header, the footer, dragging, resizing and `Enter` all arrive with nothing written
here.

- **Name** — a field, with the name that will be made.
- **Location** — a field with a folder button beside it that opens the platform's folder picker. It
  starts at the parent of the project that is open, which is the folder a second project most often goes
  beside.
- **A line saying where it will go** — `Project will be created in: <location>/<name>` — which is the
  reference dialog's own line and is what makes the two fields legible together.
- **Create Git repository** — a tick box, on by default.

`Create` makes the folder, runs `git init` in it when the box is ticked, and opens it **in a window of
its own** through `services::launcher::open_window`, which is what `Open Folder` and `Recent Projects`
already do: a project is a window. If a second process cannot be started the folder takes this window
instead, which is `Open Folder`'s own fallback.

Refusals are sentences rather than a dialog that does nothing: a name that is empty, a name with a path
separator in it, a location that does not exist, and a folder that is already there and is not empty.
`git init` is `unluminous_git`'s own worker, so a machine with no git says what git said.

**The agent's half** is `unluminous-cli project new --name <name> --location <folder> [--git]`, which
reaches the same function, and `modal open create-project` for driving the dialog itself. The command is
in the catalogue, so the MCP tool and the documentation section come with it.

---

## 5. A background picture, chosen from a grid

### What it is

`appearance.background.opacity` fades the window so the desktop shows through, and that is the only thing
the window can have behind it. The ticket asks for a second answer: a picture, chosen from a grid,
kept by the application, and applied at once.

### What changes

**One new setting**, `appearance.background.image`, whose value is a file name inside the application's
own backgrounds folder. Empty means *show the contents underneath*, which is what every Unluminous that
has never chosen one says by saying nothing — `terminal.shell`'s rule and `appearance.theme`'s.

**The library is a folder**, `<the person's settings folder>/backgrounds/`, and it is the whole of the
state: what the grid shows is what is in it. A picture chosen from disk is **copied** in, under a name
derived from the file it came from with a number added if that name is taken, so the picture survives the
original being moved or deleted. Removing one deletes the copy — a file this application made, in a folder
this application owns, which is the one kind of deletion the house rule allows.

**The grid is a modal**, opened by a `Background...` button beside the opacity slider on
`Settings -> Appearance`. The first cell is `Show Contents Underneath`, drawn as the window's own ground;
the rest are the pictures, each with a trashcan at its top right; and a cell that says `Add a picture...`
opens the platform's file picker. The chosen cell is drawn with the accent ring every chosen row in
Unluminous already has. Pressing a cell applies it immediately, because the setting is what the window
paints from and the window paints every frame.

**Where it is painted.** `app::frame::lay_the_frame_out` paints the window's own rounded rectangle first
and everything else on top of it. The picture goes between: the rounded rectangle is filled with the
picture, scaled to cover and clipped to the corners, and then the faded ground is painted over it exactly
as it is today. So the opacity setting goes on meaning what it means — at 100% the panes hide the picture,
and below that it shows through, which is the same relationship the desktop has with it now.

Decoded once and kept as a texture, keyed on the file name and the file's own modified time, so a frame
where nothing changed costs a comparison. `services::picture::upload` is what shrinks a picture to the
card's largest texture, and it is used here for the reason it exists: egui panics when handed a bigger
one and a desktop wallpaper is exactly that size.

**Nothing is fetched.** A file is read from the disk the person pointed at and from nowhere else, which
is the rule the Markdown preview already keeps.

**The agent's half** is `unluminous-cli background list | add | use | remove`, plus
`settings set appearance.background.image`, which already worked through the generic settings command and
goes on working.

---

## 6. An agent on the canvas cannot reach `unluminous-cli`

### What it is

A terminal node's shell is started with four variables: `UNLUMINOUS_SPACE_NODE`, a sentence in
`UNLUMINOUS_SPACE_HINT`, and `UNLUMINOUS_CLI` and `UNLUMINOUS_INSTANCE` saying where the client is and
which window it drives. The catalogue's own `space` preamble says **"`unluminous-cli` is not on your
PATH"** and tells the reader to run `"$UNLUMINOUS_CLI" --instance $UNLUMINOUS_INSTANCE …`.

That is a true sentence and it is also the whole problem. An agent types `unluminous-cli space here`,
gets `command not found`, and concludes the feature is not there. It never reads
`UNLUMINOUS_SPACE_HINT`, because nothing makes it run `env`. The report is exactly this: *"don't seem to
have the cli, or don't understand the unluminous cli."*

### What changes

**`unluminous-cli` goes on the node's `PATH`.** The folder holding it is prepended to the child's `PATH`,
so the name an agent guesses is the name that works. It is prepended rather than appended so that the
client beside *this* window wins over a different Unluminous installed elsewhere.

**`--instance` defaults to `UNLUMINOUS_INSTANCE`.** The client already reads `UNLUMINOUS_SPACE_NODE` for
`space here`, for the same reason and with the same narrowness: it is a variable only Unluminous ever
sets, on a shell only Unluminous ever starts, and a caller that passed `--instance` is left alone. Without
it, a machine with two windows open makes every command an agent sends ambiguous.

**A chat node's agent gets the same environment.** `unluminous-chat` starts `claude` or `codex` with an
`Environment`, and the same three additions are made to it, so the agent in a chat node and the agent in
a terminal node reach the window the same way.

**And the catalogue's sentence changes with the code**, because it is read by the thing it is about: the
`space` preamble says `unluminous-cli` is on the path of a terminal node and that `--instance` needs no
saying there.

`--from` is deliberately **not** filled in. The note in `unluminous-cli/src/parse.rs` weighed it and
refused it, and the reasoning holds: a command with no `--from` is the window's own agent and may reach
every node, so an agent in a node that omits it is not blocked by anything — it is only when it wants the
wiring enforced that it says so.

---

## 7. A browser node jitters while the canvas is zoomed

### What it is

A node is drawn into an `egui` layer carrying the camera, so it is genuinely scaled. A `WebView` is a
native child and cannot be transformed, so `app::space::show_a_browser_node` spends the camera's zoom on
the page's own zoom and sends the whole screen rectangle as the bounds. Between them the page's CSS
viewport is meant to stay the node's own size, so the page never reflows.

Three things break that, and all three happen on **every frame of a zoom**:

- **The zoom is sent every frame with no check.** `place` guards the bounds against the value it last
  sent; the zoom has no such guard, so `put_ZoomFactor` is called sixty times a second with values that
  are often identical. Each call is a zoom-changed event and a re-layout in the engine.
- **The two are sent separately and land separately.** The bounds are a `SetBounds` on the controller and
  the zoom is a property on it, and the page sees whichever arrives first — so for part of a frame its
  viewport is the new size at the old scale, which is a different number of CSS pixels, which is a
  reflow. `task-1945`'s `Camera::glide` made this continuous rather than occasional, which is why the
  report arrived when it did.
- **The window region is set every frame.** `clip_to_the_visible_part` is guarded on the pair of
  rectangles, and during a zoom both change every frame — so `SetWindowRgn(hwnd, …, TRUE)` is called every
  frame, redrawing the child from scratch each time, even for a node wholly inside the pane where the
  answer is "no region at all".

### What changes

**The page's zoom is derived from the bounds it was given, so its viewport is exactly the node's own
size.** The bounds are rounded to whole physical pixels by the engine whatever is sent; so the rounding
is done here, and the zoom is then `physical width / (node width in canvas points × the display's scale)`.
The page's `innerWidth` is the node's own width, to the pixel, at every camera zoom — so it does not
reflow at all while the canvas is zoomed, and only the raster scale changes, which is what the engine is
good at. The node's own page zoom multiplies into it exactly as it does today.

**Both are sent in one place and only when they change.** `NativeView` remembers the factor it last sent
beside the bounds it last sent, and `place` sends the bounds and then the zoom.

**The region comes off once.** `NativeView` remembers whether it has a region rather than which pair of
rectangles produced one, so a node wholly inside the pane sets no region on any frame after the first.

*"without breaking the functionality"*: a press inside the page is delivered by the engine in its own
coordinates and the zoom factor is the engine's own, so where a click lands is unchanged. The tests that
drive a browser node's toolbar, its address bar and `space browser` are what say so.

---

## 8. The plus sits after the last view

`components::space::view_bar` draws the plus at `area.right() - 18.0`, at the far right of the bar, past
the zoom controls. The ticket asks for the browser's arrangement: immediately after the last tab, so it
moves as tabs are added.

It moves to the pen — after the last chip, and after the `+N more` row when there is one — and the chips
stop before the zoom controls and the plus's own width rather than before the plus's old place. The zoom
controls stay at the right hand end, which is where `task-1905` asked for them and where the top right of
the canvas is.

---

## What is deliberately not here

- **A second background per project.** The picture is a person's choice about their window, like the
  theme and the interface font, so it lives in the person's settings and not in a project's `.unluminous`
  — which is the line `task-1697` drew for where the panels are.
- **Tiling, blurring or dimming a background picture.** One picture, scaled to cover. The opacity slider
  is already how much of it shows through.
- **A template on the Create Project dialog.** The reference dialog offers a language and a build system;
  Unluminous has no project model to fill one in with, and a dropdown that only ever says "empty" is a
  control that can never apply.
- **Filling `--from` in from the environment.** §6 says why.
- **Letting a drag hide the editing area entirely.** A divider dragged to the editing area's floor stops
  there; `View -> Toggle Editor` is how it goes away, and it is one keystroke.
