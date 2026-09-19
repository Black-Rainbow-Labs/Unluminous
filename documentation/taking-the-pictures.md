# Taking the pictures again

`documentation/overview.md` is thirty-seven captures of the window and `documentation/database.md`
is nine of the Database plugin. **Forty-six pictures, and one command takes all of them:**

```powershell
pwsh tools/documentation/capture.ps1
```

It builds the project the pictures are of, opens a window on it, drives that window into each state,
photographs it, and writes the finished picture into `documentation/images/`. It takes about eleven
minutes for the whole gallery. `-Only 04-code,27-base-of-infinite-space` takes some of them, `-List`
prints their names, and `-KeepOpen` leaves the window running so a position can be worked out.

**It does not take the keyboard, move the pointer, switch the virtual desktop or need anything else
on the screen to be put away.** You can carry on working while it runs.

## What a capture is

A photograph of the window over a desktop — and the desktop is the whole reason the gallery exists.
Unluminous's background is translucent, so a picture cropped tight to the window cannot show that the
colour in the editing area is the thing behind it rather than a shade somebody chose. Every picture
is cropped 48 points wider than the window on every side, so the thing behind is visible both
through it and past its edge.

It is **not** a screenshot test. The fourteen binaries under `crates/unluminous-app/tests` build the
same window offscreen and write a PNG for each of their tests, and those images are the right ones
for checking that a control moved or a colour changed. They cannot show what this gallery is for,
because there is nothing behind them.

## How it is taken, and the measurement that decides it

Until `task-1994` a capture was a copy of the screen, which meant four things had to be true at once:
the window had to be brought to the front, real keys had to be pressed at it, every other window had
to be minimised, and the machine had to have a 3840 by 2160 screen. The first two are banned by
`CLAUDE.md` — a run that stops between a key going down and its coming up leaves that key held for
the rest of the session, and activating a window that is on another virtual desktop takes the desktop
with it. The third and fourth are why the gallery sat at version 0.1.0 for fifty releases.

None of them is needed, and the reason is one measurement:

**`unluminous-cli window screenshot` writes a PNG with a real alpha channel.** Photographed at
`--opacity 0.75`, the editing area comes back at `A=191`, which is 0.75 × 255, with `RGB=(25,31,37)`
against the `(26,31,38)` the same pixel reads at full opacity — so the colour is straight rather than
premultiplied. Every glyph comes back at `A=255`. The rounded corners come back at `A=0`.

The window's own capture therefore already carries exactly what a compositor needs, and

```
out = window.rgb × a + backdrop.rgb × (1 − a)
```

reproduces a copy of the screen given the same thing behind it. `tools/documentation/backdrop.jpg`
is that thing. Compositing in software rather than copying the screen removes all four requirements
at once:

| What a screen copy needed | Why it is not needed now |
|---|---|
| The window in front | `window screenshot` never needed the focus, and `unluminous --background` opens the window without becoming the foreground window |
| Real key presses and pointer moves | Every state is reached with `unluminous-cli`. The few things that are on no command — a menu, a context menu, the text options flyout — are clicked with `unluminous-cli input`, which feeds the window the same `egui::Event` a mouse produces, down the control channel |
| Every other window minimised | Nothing behind the window is photographed, so nothing behind it matters |
| A 3840 by 2160 screen | The window is sized with `unluminous-cli window size`, and what is photographed is the window rather than the screen |

It also makes the gallery one set rather than thirty-seven pictures of whatever was on one machine's
desktop on the day.

## The four files

| | |
|---|---|
| `tools/documentation/capture.ps1` | the pictures, one block of `unluminous-cli` commands each, and the compositing |
| `tools/documentation/fixture.ps1` | the project every picture is taken of |
| `tools/documentation/library-db.mjs` | the SQLite database the Database plugin's nine pictures are of |
| `tools/documentation/backdrop.jpg` | the thing behind the window |

`tools/documentation/check-links.mjs` is beside them and is not part of taking a picture: it resolves
every relative link and every backticked path in this folder, in `README.md`, in `CONTRIBUTING.md`,
in `AGENTS.md` and in `design/`, and says which ones go nowhere.

### The fixture

Built under the temporary folder rather than opened where it lies inside this repository, because
`sample/` sits inside the checkout and opening it makes the status bar say how many files happened to
be uncommitted that day. A copy with a small history of its own says `main`, which is what a reader
with a fresh checkout sees. What is in it and why is at the top of `fixture.ps1`; the parts that
matter to a picture are three commits by three authors on three widely separated dates that **all
touch the same file**, so the history dialog has three entries and the blame column has three ages to
colour; one uncommitted change and one untracked file, so the commit panel and the change bars have
something to show; and a file in each of several languages, because the colours are a plugin's doing
and a picture of one language is a picture of one plugin.

**The window is given a settings folder of its own**, through its own `APPDATA`. Without it the
pictures would carry whatever font size, opacity and explorer width the person running them has set,
and taking them would leave the fixture's project state in their real settings.

### The backdrop

Generated on this machine through the AI service's Krea 2 endpoint, which is the recipe the bundled
plugin icons already use and already record. Generated rather than borrowed, because a wallpaper
taken off a machine is somebody else's picture and a gallery in a public repository cannot carry one
whose licence nobody can name.

It has to earn its place rather than be a gradient. What the gallery exists to show is that the colour
in the editing area is the thing behind the window, and that only reads when the thing behind has real
variation in it — light, colour and structure that visibly continues underneath the window and out
past its edge.

## Four things that went wrong the first time it ran, and what each one is now

All four were found by opening every picture, which is the one step this file cannot automate.

- **A right click menu from one picture was drawn over the next twenty.** A context menu and the text
  options flyout are `egui` popups rather than modals, so `modal cancel` does not see them.
  `input key Escape` does, and it is the first thing `Reset-Window` sends.
- **The readme left bold and centred by the formatting picture arrived at every picture after it.**
  The loop that closed the tabs stopped when one was left, because closing the last leaves an empty
  untitled tab — so the last file stayed open, and formatting is not a text change, so nothing
  discarded it. It closes that one too now, and the formatting picture runs last.
- **`space add editor --path src/theme.rs` moves that tab onto the canvas node.** A tab that lives on
  a node is in no pane, so the picture after the canvas one opened an empty untitled tab. Every node
  is removed on reset.
- **The run configuration added by the run and debug pictures widened the widget at the right of the
  title bar**, which moved the `F` button — and the `F` button is one of the few things clicked by
  position. It is removed on reset, which also keeps the title bar the same in every picture.

## The one thing `Reset-Window` cannot put back

The Database plugin's workspace. A grid and a console are its own tabs and no command closes them, so
four pictures of one table in a row come out with four tabs called `album [library]`. The four that
open one are named in `$Fresh`, and the script starts the window again before each of them. The board
and the data source both survive that, because one lives in the project and the other in the settings
folder.

Two of the nine are clicked rather than commanded, and each for its own reason.
`plugins run database ddl` **answers with the statement and opens nothing**, which is right for an
agent asking a question — the modal is what a person gets, and it is on the tree's own right click
menu. And `modal open settings --page` names the five pages Unluminous itself has, so a page a plugin
contributed is reached the way a person reaches it, by choosing it in the list.

## The one picture whose explorer is different, and why

`28-agent-chat` is taken **last**, and its explorer has two rows no other picture has: `app.rdb` and
its log. They are not Unluminous's. The `claude` on this machine is configured with an MCP server of
its own, `inillucent-mcp --db app.rdb --root .`, and a relative path resolves against the folder the
agent was started in — which is the project the window has open, because running the agent there is
what makes its answer about your code.

It is left in rather than tidied away, because it is the pane doing exactly what
[the agent panes](agent-panes.md#agent-chat) says it does. What the ordering buys is that it is in
one picture rather than in the fourteen that came after it, which is what the first run of this
produced.

## Adding a picture

Add a block to `$Pictures` in `capture.ps1`: a name, and the `unluminous-cli` commands that put the
window into the state. Reach for `input click` only for something that is on no command, and keep the
position in `$Menu`, `$FButton`, `$Flyout` or `$ExplorerRow` beside the others — they are all for a
window of exactly 1800 by 1160 points, which is what the script sizes it to.

Then run it, **open the picture and look at it**, and add a row to `overview.md`. A picture nobody has
opened is a picture nobody knows is right: two of the four faults above passed every check the script
makes and were obvious the moment somebody looked.

## What has no picture, and why

**A rendered web page.** A browser tab and a browser node are a native child window the operating
system composites on top of the surface `window screenshot` reads back, so the capture comes back with
an empty rectangle where the page is. Measured on 0.39.0 and still true. `tools/capture-window.ps1` is
`PrintWindow` with `PW_RENDERFULLCONTENT`, which does hold the page — but it holds no alpha either, so
what it produces cannot be composited and would be the one opaque picture in a gallery about
translucency. A browser picture needs a real copy of the screen, and that is the one thing this
harness deliberately cannot do.

## The next thing

Inillucent has a test that fails while a page in its documentation folder is not in its index, so the
index cannot drift from the directory. This folder has no equivalent, and it should. It needs a test
crate that reads `documentation/` and a decision about where that test lives, which is a code change
rather than a documentation one.
