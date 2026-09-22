# task-2063: Unluminous Improvements 7

The ticket asks for five things:

1. **Updates.** `Check for Updates` should offer to install the new version and restart. A check
   should run once a day on its own and raise a toast with two buttons: `Install & Restart`, and
   `Don't Ask Again`, which stops asking about **that** version and not about later ones.
2. **Zooming the Markdown preview.** `Ctrl`/`Cmd` with plus, minus or the wheel works over the source
   and does nothing over the preview.
3. **Go to definition and back.** `Ctrl`/`Cmd`+Click on a function, variable or class goes to where
   it is defined, as IntelliJ does. `Ctrl`/`Cmd`+`[` and `]` go back and forward through the places
   the caret has been: the same file, the same line and the same caret position.
4. **Mermaid in Markdown.** `<br/>` is shown as text, some parts overlap, and some formatting is bad.
   Take screenshots, look at them, and fix what they show.
5. **Resizing the window** still fails some of the time, for example from the left edge.

Each section says what is there now, what was measured or read, the design, and how it is tested.

## 1. Updates

### What is there now

`services::update` asks unluminous.com's `/releases/latest.json` and falls back to the public GitHub
repository. It answers `Newer`, `Current` or `Failed`. The answer goes to the status bar and to the
About box. `update.check` is `off` or `start`, and is `off` by default. Nothing downloads anything.

The manifest the site publishes already has what an installer needs:

```json
{
  "version": "0.54.2",
  "installer": "https://unluminous.com/downloads/UnluminousSetup-0.54.2-x64.exe",
  "installerBytes": 12875680,
  "installerSha256": "66885379…",
  "macos": "https://unluminous.com/downloads/Unluminous-0.54.2-macos.zip",
  "macosBytes": 33118584,
  "macosSha256": "d6507d0d…"
}
```

### Research

- VS Code on Windows downloads the Inno Setup installer in the background, then runs it with
  `/verysilent` when the person chooses `Restart to Update`, and the installer starts the new build.
  Its toast offers `Update Now`, `Later` and `Release Notes`.
- Inno Setup's own command line: `/VERYSILENT` hides the progress window, `/SUPPRESSMSGBOXES`
  answers every message box with its default, `/NORESTART` never reboots the machine,
  `/CLOSEAPPLICATIONS` closes programs holding files it has to replace through the Restart Manager,
  and `/ALLUSERS` or `/CURRENTUSER` choose the install mode when the script allows the override,
  which ours does (`PrivilegesRequiredOverridesAllowed=commandline dialog`).
- An installer cannot replace a running executable, so the program being updated has to exit first.
  VS Code and Electron's Squirrel both use a small separate process that waits for the old one to
  exit, runs the update, and starts the new one.

### Design

**`update.check` gains `daily`, and `daily` is the default.** This reverses `task-1804` §6, which
made the check off by default so that nothing was sent without somebody pressing something. The
ticket asks for the check to run once a day on its own, so the ticket wins. The request is unchanged:
one unauthenticated `GET` that sends nothing about the person or the project. `off` still sends
nothing. `start` still asks every time a window opens.

**The day is counted across windows.** Unluminous runs one process per window, so a clock held by one
window would check once a day per window. `update-checked.txt` in the person's settings folder holds
the time of the last automatic check. A window checks when it starts and on the heartbeat, and only
when that file is more than 24 hours old. It writes the file before it asks, so two windows starting
together do not both ask.

**`Don't Ask Again` is about one version.** `update.skip` in the settings holds the version that was
skipped. An automatic check that finds that same version raises nothing. A later version raises the
toast again. `Check for Updates` from the menu always shows what it found, skipped or not, because a
person who asks wants the answer.

**A toast can carry buttons.** `components::toast::Notice` gains a list of actions, each a label and
a `toast::Act`. A notice with actions is `Kind::Offer`: it does not fade, and it is drawn with the
accent colour on its edge. Pressing a button raises the act for the window to carry out and takes the
notice away. The cross takes it away and does nothing else, so the next daily check asks again.

The offer toast reads *"Unluminous 0.55.0 is available. This is 0.54.2."* with `Install & Restart` and
`Don't Ask Again`. On a platform with nothing to install it offers `Open Download Page` instead.

**Installing is a download, a check, and a hand over.** `services::update_install` does it on a
thread, and reports progress the window shows in the status bar and in the About box:

1. Read which file applies to this platform from the manifest: `installer` on Windows, `macos` on
   macOS. A GitHub answer has no manifest, so the install asks the site's manifest directly. Linux
   has no installer and the offer there opens the download page.
2. Download it into `%TEMP%\unluminous-update-<version>\` and check the byte count and the SHA-256
   against the manifest. A file that does not match is deleted and the install stops with a sentence
   saying so. Nothing unchecked is ever run.
3. Hand over to a helper that outlives the window, and quit.

**The helper is a copy of `unluminous-cli`.** It is started as
`unluminous-cli --apply-update <installer> --wait <pid> --relaunch <unluminous.exe>` from a copy in the
download folder, because the installer replaces `unluminous-cli.exe` in the install folder and cannot
replace a program that is running. It is a console program, so it is started with
`CREATE_NO_WINDOW` and no console appears. It waits for the window's process to exit, runs the
installer with `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS`, adding `/ALLUSERS` when
the running executable is under `Program Files`, waits for it, and starts the new `unluminous.exe` in
the install folder with no arguments. Started that way, from the folder it lives in, Unluminous opens
the windows that were open in the last session, which is `starting_folder`'s existing rule.
Inno Setup remembers the install folder and the tasks chosen last time, so a silent upgrade keeps
the PATH entry, the right click verbs and the shortcuts.

On macOS the zip holds `Unluminous.app`, and the same helper handles it: it waits for the process to
go, unzips with `ditto -x -k` into the download folder, moves the running bundle aside and the new one
into its place, puts the old one back if the second move fails, and starts the bundle with `open`.
`ditto` rather than `unzip`, because it keeps a bundle's signature and extended attributes whole. This
half is written but was neither built nor run on a Mac for this ticket.

**The window quits properly before the helper runs anything.** It is `ViewportCommand::Close`, so
`on_exit` writes what the project remembers exactly as closing the window by hand does.

**Everything is reachable by an agent.** `update install` downloads and installs the newest version
(`--no-restart` downloads and checks it without handing over, which is what the tests use).
`update skip [version]` records a version as skipped, and `update status` reports the last answer,
the skipped version, whether an install is running and how far it has got. `action run
check-for-updates` already exists.

### Tests

- `update::read` reads the installer fields; a manifest without them is still a release.
- `UpdateCheck::parse` reads `daily`; the default is `daily`.
- `is_due` decides a daily check from a time and a file, pure, with no clock.
- An automatic check that finds the skipped version raises no toast; a later version does.
- A toast with actions does not fade, and pressing a button reports the act and removes the notice.
- The download is checked against a scripted server: a good file passes, a wrong length and a wrong
  hash are both refused and the file is deleted.
- `unluminous-cli --apply-update` is tested with a fake installer that writes a marker file, and a
  relaunch target that writes a second one.

## 2. Zooming the Markdown preview

### What is there now

A gesture is claimed by whichever pane the pointer is over (`ZoomClaim`). The source pane claims it
in `claim_a_zoom_over_the_editor`. The preview never claims it. In the Preview view mode no source
pane is drawn at all, so the gesture is claimed by nobody and nothing happens. In the side by side
mode, a gesture over the preview is offered to the pane with the keyboard, which zooms the source
about the top of its view, and the preview moves somewhere else.

The keys go where the keyboard is. The preview never takes the keyboard, so after clicking in the
preview the keys zoom whichever panel had it last, often the explorer.

### Design

The preview claims the gesture when the pointer is over it, in `show_markdown_preview`, the same way
the source pane does. The size is the same setting, `appearance.font.size`, because the preview is
laid out from the source's own base style; two sizes would be two numbers meaning one thing. What
differs is the anchor. The preview sets `preview_anchor` at the pointer before the size changes, so
the line under the pointer stays under the pointer. The source's anchor is taken from the top of its
view by `set_the_font_everywhere` as it already is.

For the keys, `the_pane_the_keys_zoom` answers the editing area when the last press was in the
preview (`reading_preview`) and the preview is showing, and the preview is anchored at the top of its
view.

### Tests

A screenshot test opens a Markdown file in the Preview mode, sends `Ctrl`+wheel over the page, and
checks that `appearance.font.size` went up and that the paragraph under the pointer is still under
it. A second test does the same in the side by side mode and checks that the source was not the one
anchored at the pointer.

## 3. Go to definition and back

### What is there now

`Ctrl`/`Cmd`+Click already goes to a definition (`task-1675`). Two things stop it from working the
way the ticket describes:

- It only works in the pane that already has the keyboard. After using the explorer or the terminal
  the first `Ctrl`+Click only moves the keyboard and does not jump. IntelliJ jumps on the first click.
- It only finds a name that a definer declares: `fn`, `let`, `const`, `class`, and so on. A function
  parameter, a closure parameter, a `for` variable, a `match` binding or a struct field has no keyword
  in front of it, so a click on one says *"No definition found"*.

Back and forward exist as `Navigate Back` and `Navigate Forward` on `Ctrl`/`Cmd`+`Alt`+`Left` and
`Right`. The back stack is only written by a jump to a definition, so opening a file, going to a
line, a search hit or a reference does not add a place to go back to.

### Design

- **A click with the modifier held jumps in any editing pane under the pointer.** The underline is
  drawn for the pane under the pointer rather than for the pane with the keyboard; there is only one
  pointer, so there is still only one underline.
- **When no definer declares a name, the first place it is written in the file is the answer.** For
  a parameter, a local and a field, the first occurrence in the file is almost always the declaration.
  It is marked `Confidence::Likely`, and the status bar says *"No declaration for 'x'; went to where
  it is first written in this file."* A click on that first occurrence itself lists the references,
  which is what a click on a definition already does.
- **`Ctrl`/`Cmd`+`[` goes back and `Ctrl`/`Cmd`+`]` goes forward.** These are IntelliJ's macOS chords
  and VS Code's `Go Back` chord on every platform is `Ctrl+Alt+-`; the ticket asks for the brackets on
  both platforms. `Ctrl`/`Cmd`+`Alt`+`Left` and `Right` keep working as a second chord, so nothing a
  person already uses stops working. **The brackets are not taken from a terminal.** `Ctrl+[` is
  `Escape` in a terminal and `Ctrl+]` is how a person detaches from `claude`, so while a terminal,
  the run tile or a terminal node has the keyboard the chord is left for the program.
- **More moves are remembered.** A place is pushed before every jump that moves the caret far:
  opening a file from the explorer, `Go to File`, `Go to Line`, a `Find in Files` hit, a reference and
  a tab switch. A place is the file and the caret offset, so going back lands on the same line and
  column. Two places closer than a few lines in one file are one place, which is IntelliJ's rule and
  stops ordinary caret movement filling the stack.

### Tests

Unit tests over a Rust and a TypeScript file: a click on a parameter goes to the parameter; on a field
goes to the field's declaration; on a function goes to the `fn` as before. `Navigate Back` after a
`Go to Line` returns to the line and column. `action_for_key` answers `Navigate Back` for `Ctrl+[`,
and the frame does not run it while the terminal has the keyboard.

## 4. Mermaid in Markdown

### What is there now

`unluminous_core::mermaid` parses and lays out twenty diagram types in Rust. `source::label` turns
`<br>`, `<br/>` and `<br />` into line breaks, but only the text that goes through it. Some places read
text with their own code.

### Method

Every `mermaid` fence in this repository's `tasks/` and in `ai-service/tasks` is collected into
Markdown files, opened in a real window in the Preview mode, and photographed with
`unluminous-cli window screenshot`. Each picture is looked at. Each fault found gets a test in
`unluminous-core` and a fix, and the pictures are taken again. The findings table is in
`_agent_output/task-2063-improvements-seven/mermaid-findings.md`.

### What the photographs showed

457 diagrams in 211 documents: 278 flowcharts, 159 sequence diagrams, 15 `graph`s and 5 state
diagrams. `examples/mermaid_audit.rs` counted 7 refused, 60 with markup left in, and 499 pairs of text
drawn over each other. The pictures showed where the overlaps came from.

### Design

Every fix keeps `mermaid::check::properties`: nothing outside the scene, no two node boxes overlapping,
every number finite and every label present.

1. **`<br/>` followed by a character of more than one byte stayed as text.** `strip_break` sliced the
   next six bytes, which is no slice when byte six is inside a character. Compared a byte at a time.
2. **HTML's entities**, `&lt;` `&amp;` `&quot;` `&#91;`, are decoded beside Mermaid's own `#lt;`.
3. **Tags that only change how words look**, `<b>` `<i>` `<span>` and the rest, come off and leave their
   words. A tag it does not know, such as the `<id>` in `GET /jobs/<id>`, is kept as written.
4. **`&` inside a node's brackets no longer splits the statement**, which refused seven diagrams.
5. **A quoted label that runs over several lines** is joined until its quote closes.
6. **An edge's label takes room of its own.** Most of the 499 overlaps were two edges between
   neighbouring nodes putting their labels at the same middle point. `layered.rs` does what dagre does:
   when anything is labelled, every edge spans twice the ranks and a labelled edge's middle dummy is the
   size of its label, so the ordering and placement keep labels apart like nodes. The gap between ranks
   is halved at the same time.
7. **An edge into a subgraph runs in along a clear line.** It was joined to its node from the frame by one
   straight segment that crossed other nodes. It now turns just outside the frame and runs in from the
   side with nothing in the way, going round the nearer end of the frame when it has to.
8. **A diamond is twice its words and a margin**, so a two line decision stays inside its points.
9. **A sequence message is drawn wrapped**, as it was measured, the gap beside a participant widens for a
   message to itself, and each arrow sits under its own words.

After all nine: 0 refused, 50 left with `<...>` in them (all placeholders a person meant to show), and
the overlapping pairs down from 499 to 23 before the ninth fix. The findings and the pictures are in
`_agent_output/task-2063-improvements-seven/`.

## 5. Resizing the window

### What is there now

The window has no operating system frame. `components::resize_edges` adds eight invisible egui grips
inside the window's edge and sends `ViewportCommand::BeginResize` when one is dragged. winit turns
that into a posted `WM_NCLBUTTONDOWN` and sets a private `dragging` flag, which only
`WM_EXITSIZEMOVE` clears.

### What goes wrong

Three ways, and each one explains *"from time to time"*:

1. **The grip only fires once egui decides a drag started**, which is after the pointer has moved
   past egui's drag threshold with the button held. On the left edge that motion is away from the
   window. By the time the request is carried out on the event loop thread, a quick flick has already
   let go of the button, and `WM_NCLBUTTONDOWN` for a released button starts no size loop.
2. **A request that starts no size loop latches winit's `dragging` flag for the rest of the
   process**, because only `WM_EXITSIZEMOVE` clears it and no size loop means no `WM_EXITSIZEMOVE`.
   Every later resize and every title bar drag then does nothing, until Unluminous is restarted.
3. **Anything egui draws above the pane layer over the edge takes the pointer from the grip**: a
   popup, a context menu, a toast, a canvas node's layer.

### Research

Frameless windows that resize reliably on Windows (Chromium, Electron, Windows Terminal, Tauri's
`decorations: false`) do not detect the drag themselves. They answer `WM_NCHITTEST` with `HTLEFT`,
`HTTOPLEFT` and the rest for the pixels near the edge. Windows then does everything a framed window
does: it shows the cursor, starts the size loop on the press itself, supports Aero Snap, and never
involves the application's drag state.

### Design

**On Windows the window answers the hit test itself.** `services::windows_resize` subclasses the
window with `SetWindowSubclass`. For `WM_NCHITTEST` it asks the default procedure first. When that
answers `HTCLIENT`, the window is not maximised, and the point is within `resize_edges::EDGE` points
of an edge (converted to pixels with `GetDpiForWindow`), it answers the matching `HT*` value, with
`CORNER` points at each corner. Nothing about resizing goes through egui or through winit's
`dragging` flag any more, so faults 1 and 3 cannot happen, and 2 cannot happen for a resize.

**The latch is also cleared for the title bar's drag.** `StartDrag` still goes through winit. When a
drag or resize was asked for, and a moment later the thread is not in a move or size loop
(`GetGUIThreadInfo`, `GUI_INMOVESIZE`) and the left button is up, the window posts
`WM_EXITSIZEMOVE` to itself. winit then clears `dragging`. Posting it when nothing is latched only
clears a flag that is already clear.

The egui grips stay, for macOS and Linux, and on Windows they are not added once the hit test is
installed, because a press there never reaches egui.

`status --section window` gains `nativeResize: true` on Windows, so a test and an agent can see which
of the two is in use.

### Tests

- `hit_for(point, size, border, maximised)` is pure: every edge and corner, the inside, the maximised
  window.
- In the real window: `tools/windows-input.ps1` presses on each edge and corner and drags, and
  `status --section window` reports the new size. Measured before and after.

## What is deliberately not done

- **Background downloads before anybody asked.** VS Code downloads as soon as it finds an update.
  Here the download starts when `Install & Restart` is pressed, because a download nobody asked for
  is the thing `task-1692` drew the line at, and the daily check is already one step past that line.
- **A delta update.** The installer is 12 MB.
- **Linux installs.** There is no installer to run.
