# Tests

```sh
cargo test
```

**3,263 tests** across 36 binaries, in four layers, and a change should leave all four green.
**They run on the machine that makes the change, and nowhere else.** There is no continuous
integration here, by choice: the release scripts run the suite before they will tag anything, so a
release cannot be made from a checkout whose tests do not pass, and that is the gate.

**One run at a time on one machine.** The sample folder every rendered test copies out of is written
once per test binary and **cleared first**, so two `cargo test` runs at once clear each other's
fixture and the failures land on whichever tests happened to be copying at the time. On this machine
that reads as three restore-a-project tests failing for no reason either checkout can explain.

## 1. The crates with no window

**1,481 tests**, with no window, no graphics card and no fonts. That is not a side effect of how
they were written — it is what the dependency direction buys, and it is why most of Unluminous can be
tested at all.

| Crate | What is covered |
|---|---|
| `unluminous-core` | the Markdown parser, the syntax tokeniser, every Mermaid diagram type, folding, completion, imports, encodings, and a randomised comparison of the rope against a plain `String` over 1,500 edits with the tree invariants checked after every one |
| `unluminous-terminal` | every key in the encoding table, the sixteen named colours and the colour cube, what the screen holds after a run of escape sequences, the alternate screen, scrollback, resizing, the mouse reports, the tabs, and the folder a shell reports |
| `unluminous-git` | real repositories built in a temporary folder, with **git** asked what happened afterwards |
| `unluminous-dap` | scripted adapters with no process behind them |
| `unluminous-db` | a scripted PostgreSQL server on `127.0.0.1:0` replaying fixed bytes, and real SQLite files |
| `unluminous-chat` | a scripted server replaying fixed bytes in chunked writes, and the recorded output of real runs of both agents |
| `unluminous-cli` | the catalogue, the wire format, the documentation tests, the tool preamble budget, and what an agent equipped with two areas is given |

**Layout tests measure through a fixed width stub**, so their expected numbers are arithmetic a reader
can check by hand and are the same on every machine.

**A scripted server is evidence about a protocol rather than about a server**, and the documents say
so. The SCRAM arithmetic is pinned to the published test vector, and the scripted server checks the
client's proof by the **inverse** of the way the client made it, so it is a test of the client rather
than a recording played back.
`cargo run -p unluminous-db --example connect -- <url> <PASSWORD_VARIABLE>` is how a real server is
checked by hand.

Two of the terminal's tests start a real shell and wait for its output, which is what proves the
pseudoterminal, the reader thread and the writing work together.

## 2. The window's own logic

**1,248 tests** in `unluminous-app` itself: the file explorer and its filter, what counts as a text
file or a picture, the settings file, the project's own state, the plugins and their manifests, the
menus and their shortcuts, the panels and where they dock, the panes and the tabs in them, real font
measurement and glyph packing.

One of them is a rule rather than a behaviour: **nothing that ships names the tool Unluminous was
measured against.** Every crate, the command line and its reference, and this documentation folder are
walked, and the test fails with the file and the line, because the person it is talking to has just
written the sentence and is about to rewrite it. Say `the reference editor` instead.
`tasks/` and `_agent_output/` are deliberately not covered: they are the record of how Unluminous was
designed, and rewriting a design document to say something it did not say is worse than leaving it
alone.

## 3. The whole window, rendered

**651 tests across fourteen binaries** under `crates/unluminous-app/tests/`, each building the
entire application, feeding it real events, rendering it through the graphics card and writing a PNG
for every test.

| | |
|---|---|
| `window_and_chrome.rs` | the title bar, the rail, the panels, the modals, the settings, the themes |
| `editor_formatting.rs` | the editing area, the formatting, the preview, the pictures |
| `navigation.rs` | go to file, find in files, definitions, references, rename, completion, folding |
| `canvas_space.rs` | the Base of Infinite Space |
| `plugins_board_and_chat.rs` | Agent-Tasks and Agent-Chat |
| `plugins_database.rs` | the Database plugin |
| `syntax_and_plugins_basic.rs` | the colouring, and what each language plugin claims |
| `command_line.rs` | the channel, driven against a real window |
| `terminal.rs` | the grid, the tabs, the colours |
| `git.rs` | the Git menu against real repositories |
| `debugging.rs` | breakpoints, stepping and the tiles against scripted adapters |
| `panel_docking.rs` | dragging a panel to each edge |
| `icons.rs` | every drawn mark on one sheet per icon set |
| `agent_board.rs` | six end-to-end tests that start a real agent |

**Look at the images.** They are how a person or an agent confirms that bold text is bolder, that the
settings window is laid out like the design, and that the terminal's colours are right. Once accepted
they are the comparison baseline, so a later change that alters the rendering fails a test.

```sh
cargo test -p unluminous-app --test '*' --no-fail-fast
UPDATE_SNAPSHOTS=1 cargo test -p unluminous-app --test '*'
```

**`--no-fail-fast`, always.** Cargo stops at the first binary that fails, and these are fourteen
separate binaries, so a run without that flag reports one red test while two hundred are simply
untested.

A run that differs writes `{name}.new.png` and `{name}.diff.png` next to the accepted image, and
**nothing should be accepted without opening it**.

Each platform has its own accepted set, because the window is deliberately not the same on both: macOS
has the menus in the bar along the top of the screen and the window buttons at the left, Windows draws
both in Unluminous's own title bar, and the text is Arial rather than Helvetica because Helvetica is
not installed there. macOS reads `tests/snapshots` and Windows `tests/snapshots/windows`.

**A run leaves a receipt** in `_agent_output/window-suite/`, one file a binary naming the commit it
ran at, written when the binary starts and deleted the instant anything in it panics. Both release
scripts read it and refuse to publish while one is missing or names a commit HEAD is not built on. The
suite stays manual, because a graphics card and a person opening a changed image are both needed and a
script must not satisfy the second; what stops being possible is releasing from a commit the suite has
never seen.

### Three rules those tests keep

Each is the answer to a test failing for a reason that was not a fault in Unluminous.

- **Nothing builds a graphics device of its own.** Building one instance, adapter and device per
  harness and tearing it down across as many threads as the machine has killed the process with an
  access violation on about one run in nine. A small pool is built once and shared, and every harness
  comes from one builder, so a test added later cannot go back to one of its own.
- **A fixture two tests share is written once**, behind a `OnceLock`, or one test reads a file another
  has truncated a moment ago and not yet filled in. A fixture only one test uses may be written each
  time — the name is what keeps them apart.
- **A loop that waits pumps rather than runs to quiet.** Running to quiet gives the window four steps
  to settle and panics otherwise, which is right for a settled window and wrong while git or an image
  is still being worked on. Running out of steps inside one attempt is not a failure; running out of
  attempts is, and the loop says so.

**The screenshot tests pin the rasteriser's SIMD level.** The CPU rasteriser picks the widest one the
processor has and does not promise two levels are bit-identical, so every place a test builds a window
asks for the baseline — which is what the *target* guarantees and is therefore the same on every
machine of that target.

**Tests must not read or write the settings of the person running them.** `UnluminousApp::new` reads
nothing; the released binary loads the settings and a test that wants a store is given a folder of its
own.

The six end-to-end tests that start a real agent cost money and minutes and are `#[ignore]`d.
`tools/nightly.ps1 -Register` schedules them on a machine that has a key, and says plainly when it
covered nothing.

## 4. The real application

Because the first three render offscreen and cannot show that the operating system honoured the
window's transparency or drew the menu bar.

```sh
cargo run --release
bash tools/drive-a-window.sh <folder>        # macOS
pwsh tools/drive-a-window.ps1 <folder>       # Windows
```

Neither of those activates anything. `unluminous-cli --instance <pid> …` drives the window afterwards.

For the terminal:

```sh
cargo run --example terminal_capture -- --wait 10 --send "\r" --wait 10 claude
```

That builds the real window offscreen, runs the program in the terminal, answers it, and writes a
picture, along with a second one after the tile has been made shorter, which is where a program that
was not told its new size draws in the wrong place. The images are not compared against a baseline,
because both programs draw something different every time they run; they exist to be looked at.

For git, `pwsh tools/build-git-demo.ps1` builds a small repository under the temporary folder — three
commits by three authors on three widely separated dates, a branch, an uncommitted change and an
untracked file — which is enough to exercise every entry on the Git menu by hand. For diagrams,
`cargo run --example mermaid_check` lays out every file in `sample-diagrams/` and says what came of
each.

## The fifth thing, which is not a layer because nothing fails it

**`tools/agent-study/` watches an agent drive a real window** through instructions phrased the way a
person speaks, and grades what happened by reading Unluminous's own state back rather than by
believing what the agent said.

The four layers prove an agent *can* reach a feature. The study is how you find out whether it *does*
— and the first run found an agent doing 24% of its work with `grep` and `bash` in a window that had a
command for every job. [For AI agents](for-ai-agents.md#reachable-is-not-the-same-as-reached) is what
came of it. **Add a scenario when you add a feature.**

## A performance change is measured, not asserted

A threshold in milliseconds would be a different number on every machine. What *is* a test is the work
itself — how many glyphs the painter placed, how many clusters the fonts were asked to measure,
whether showing a tab that was already laid out lays it out again — because those are the same
everywhere.

| | |
|---|---|
| `examples/frame_cost.rs` | what each part of a frame costs, with the real fonts of this machine |
| `examples/startup_cost.rs` | the one startup step the frame trace cannot see inside, and the memory with and without a graphics device |
| `examples/layout_memory.rs` | element sizes, lengths, capacities and accounted heap for a real file |
| `examples/symbol_cost.rs` | indexing, a hover, a reference search |
| `examples/completion_cost.rs` | a keystroke on the largest file in this repository |
| `examples/folding_cost.rs` | reading a file's foldable regions |
| `examples/vello_cost.rs` | a board's decoration, changed frame and still frame |
| `examples/replay_probe.rs` | what a pseudoconsole does with a screen written into it from outside |
| `tools/measure-release-resources.ps1` | a release build driven through a fixed ten-file corpus |

**And every one of those measures one component with no window behind it**, which is exactly why they
all said the code was fast while an idle window was using a twenty-third of a core.
`UNLUMINOUS_FRAME_TRACE=<file> unluminous` is the one that measures the **whole thing**: the real
binary, on this machine, writing what each phase of each frame cost and when startup got to each step.
Reach for that first when the question is "why is the window slow" rather than "why is this function
slow".

## What the release scripts check before they will tag

```sh
bash tools/release.sh        # macOS and Linux
pwsh tools/release.ps1       # Windows
```

`--dry-run` says what it would do and changes nothing.

| | |
|---|---|
| a clean checkout | the version bump is a commit of its own, so the history stays greppable by ticket |
| `cargo fmt --check` | |
| `cargo clippy -D warnings` | |
| `node tools/changelog.mjs --check` | the changelog is written from the ticket-prefixed commits, so it cannot fall behind |
| `node tools/contrast.mjs --check` | every ordinary-text pair in the palette is at 4.5:1 or better, and no pair got worse |
| `node tools/window-suite.mjs --check` | the window suite ran at this commit |
| the test suite | |

Then it bumps the version, rebuilds — which is what moves the build date the About box shows —
reinstalls Unluminous on this machine, tags, pushes, and publishes the release with the installer
attached.

On Windows it also runs `tools/unstick-keyboard.ps1` first, because a script that drives the real
window presses a key by sending a key-down and releases it by sending a key-up, and Windows believes
that key is held until the up arrives — for the rest of the session, if it never does. Nothing on the
screen says so, and the physical keyboard cannot clear it, because the physical key was never down.
`tools/windows-input.ps1` is the one way a script sends input, and it is built so that it cannot leave
anything held: a key is held only for the length of a block with the release in a `finally`,
everything is released when the shell exits, and everything is released when the file is **loaded**,
so a run can never inherit a key an earlier one left down.
