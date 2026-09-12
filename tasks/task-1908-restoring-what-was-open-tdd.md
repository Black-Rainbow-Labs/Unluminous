# task-1908 — the windows come back, and what a terminal can honestly bring with it

Three reports against 0.41.1, and they are three different sizes of problem:

1. *"Now the web browser contents disappear if the node is slightly off screen."*
2. *"If I quit Unluminous, and open back up, it's supposed to open the windows I had open, but it's never
   doing that."*
3. *"It's still not restoring my terminal states to what they were. e.g. I have 2 terminals, one with claude
   code, and one with `ls` command executed. When I quit and reopen, I want both exactly restored so I see
   claude code and the contents of `ls`."*

The first is a fault in `task-1907`'s own fix. The second is a one-line condition that has been wrong on
macOS since it was written. The third cannot be delivered as asked, and most of this document is about what
can be delivered instead and why the rest is not a matter of effort.

Every section says what was reported, what is actually happening, what changes, and what proves it.

## 0. What was measured first

**The session fault was reproduced with a real application bundle.** A tiny `.app` was built whose whole
program writes down its own launch conditions, and it was opened with `open` — which is what the Dock, Finder
and Spotlight all do:

```text
cwd    = /
exe    = /private/tmp/Probe.app/Contents/MacOS/Probe
parent = /private/tmp/Probe.app/Contents/MacOS
started_from_the_desktop = false
```

`started_from_the_desktop` is *"the current directory is the folder the program itself lives in"*, which is
true of an installed `unluminous.exe` on Windows and false of every macOS launch there is. Everything gated on
it therefore never happened: `Store::open_windows` reads `session.txt`, and the answer was thrown away.

The file itself is fine and has been all along. Read from this machine:

```text
/
/Users/jason.mcaffee/dev/quill
/Users/jason.mcaffee/dev/test-project
/Users/jason.mcaffee/dev/alle-experience-bfe
/Users/jason.mcaffee/dev/inillucent-sample
/Users/jason.mcaffee/dev/ai-tools
/private/tmp/v3
```

Seven windows recorded, none of them ever restored.

**The browser fault is `task-1907`'s threshold, and a fraction was the wrong measure.** That ticket stopped
drawing a page cut to less than nine tenths of its width, because `wry` narrows a native child's *viewport*
rather than cropping it and a responsive page relays out into the narrower one. Measured against the numbers
that produced the report: a page five hundred points wide went blank at **eighty points** off the edge, which
is what *"slightly off screen"* means.

The measure is wrong in both directions, because what makes a page reflow is its viewport crossing a
stylesheet's breakpoint — Bootstrap's `sm` is 576, Tailwind's `sm` is 640 — and that is an absolute width:

| node | cut by a tenth | crosses a breakpoint? |
|---|---|---|
| 900 points | 810 | no, nothing reflows |
| 700 points | 630 | yes, Tailwind `sm` |

**And overflowing the pane instead was measured and refused.** The obvious alternative is to draw the view at
its whole width and let the window clip the overhang. Measured on a real window — 1100 by 720, with the canvas
pane at `x 36..1100, y 158..688` — that works on exactly **one** of four edges:

| edge | pane | window | safe to overflow? |
|---|---|---|---|
| right | 1100 | 1100 | **yes** |
| bottom | 688 | 720 | no, the status bar |
| left | 36 | 0 | no, the activity rail |
| top | 158 | 0 | no, the title bar and the tabs |

A native child is composited above everything `egui` draws, so three of those are a web page drawn over
Unluminous's own furniture.

**And the terminal report runs into two facts about the code as it stands.** `Session::feed` returns
immediately unless the session has a `parser`, and only a **detached** session has one — a session with a real
shell has the reader thread's parser instead, so there is no way to write bytes into a live terminal's screen
today. And `Session::written_text` answers with plain text: it walks the grid taking each cell's character, so
the colours, the bold and the cursor position are not in it.

## 1. What the tools that do this actually do, and what the report is asking for

The third report asks for two different things in one sentence, and every tool that has tried it separates
them. So this section separates them first, with the evidence, because the answer to one is "yes" and the
answer to the other is "not without a different architecture".

### 1.1 The contents and the process are not the same problem

**VS Code says so in its own words**, and the wording is deliberate:

> **Process reconnection:** When reloading a window (for example, after installing an extension),
> **reconnect** to the previous process.
>
> **Process revive:** When restarting VS Code, a terminal's content is restored and the process is
> **relaunched** using its original environment.

*Reconnect* on a reload; *relaunched* on a restart. On a full quit what survives is the buffer text and enough
to spawn a new process — not the process. Anything mid-execution is gone.
(<https://code.visualstudio.com/docs/terminal/advanced>)

**iTerm2 restores both, and it is explicit about how.** Its jobs do not run as children of the editor at all:

> Session restoration works by running your jobs within long-lived servers rather than as child processes of
> iTerm2. … If iTerm2 crashes or upgrades, the servers keep going.

On relaunch it "searches for running servers and connects to them". And the limits are stated in the same
place: *"If you reboot, your jobs will terminate and not be restored. The window contents should be
restored"*, and *"Quitting iTerm2 with Cmd-Q will terminate your jobs"* unless an advanced setting says
otherwise. A session whose text came back without its process is marked with a reverse-video **Session
Restored** banner — the tool tells you which of the two you got.
(<https://iterm2.com/documentation-restoration.html>)

**tmux is the same architecture with the seam in a different place.** The manual:

> In tmux, a session is displayed on screen by a *client* and all sessions are managed by a single *server*.

A session is *"a single collection of pseudo terminals under the management of tmux"*, held by the server, and
*"each session is persistent and will survive accidental disconnection … or intentional detaching"*. The
client is a display and input conduit. That is why detaching costs nothing: the pseudoterminal and its child
were never the client's. (<https://man7.org/linux/man-pages/man1/tmux.1.html>)

**So the shape of the answer is settled by other people's evidence.** A process survives its editor closing
only if the editor was never its parent. Unluminous starts a shell in a pseudoterminal it owns —
`Session::spawn` — and `alacritty_terminal`'s own `Drop for Pty` sends `SIGHUP` to the child by process id.
Bringing `claude` *back to life* with its conversation and its screen intact would mean Unluminous growing a
server that owns the pseudoterminals and outlives the window, which is a different program: a daemon with its
own lifetime, its own socket, its own upgrade story and its own answer to what happens when two windows want
the same session. That is not this ticket.

### 1.2 And replaying a TUI's screen is meaningless, which is not a matter of effort

The report names `claude` specifically, and `claude` is a full-screen program. The escape sequence it uses to
become one is `1049`, and xterm's own reference says what that does:

> **1049**: Save cursor as in DECSC, xterm. After saving the cursor, switch to the *Alternate Screen Buffer*,
> **clearing it first**.
>
> **Reset 1049**: Use Normal Screen Buffer and restore cursor as in DECRC.

So the alternate buffer is cleared when it is entered and abandoned when it is left, and *"nothing carries
over between sessions"*. (<https://invisible-island.net/xterm/ctlseqs/ctlseqs.html>)

Two things follow, and both are about `claude` rather than about terminals in general. A saved copy of the
alternate screen is a picture of a program that is not running — it would come back as a frozen frame of
Claude's interface with no process behind it, which is worse than an empty terminal because it looks alive.
And the scrollback *behind* it is the shell's, not Claude's: what a person would get back by replaying the
normal buffer is the prompt they typed `claude` at.

**And a process whose pseudoterminal master has closed cannot be reattached to, which is the other half of
§1.1.** It either dies on `SIGHUP` or survives with a terminal that answers `EIO` for ever, and reopening the
device by name answers `EIO` too. So there is no version of "find the old `claude` and talk to it again" that
does not begin with a server that held the pseudoterminal all along.

**Two smaller findings from the same research, both about what other tools settle for.** VS Code keeps
**100 lines** of scrollback by default (`terminal.integrated.persistentSessionScrollback`, from its 1.60 notes),
which is a useful sanity check on `REPLAY_LIMIT` — the ambition here is a screen and some history, not a
session log. And JetBrains restores *"tab names, the current working directory, and even the shell history"* and
no screen contents at all
(<https://www.jetbrains.com/help/idea/terminal-emulator.html>), which is less than this ticket delivers.

**`ls` is the opposite case and it is the one that can be answered.** Its output is in the normal buffer, it
is text, nothing is running, and replaying it is exactly what macOS Terminal and VS Code do.

### 1.3 So what this ticket delivers

| what | can it come back? | how |
|---|---|---|
| the terminal's scrollback and screen, for an ordinary shell | **yes** | saved as text with its colours, replayed into the new session |
| `ls` output, a build log, a prompt somebody was reading | **yes** | the same thing |
| a full-screen program's own screen — `claude`, `vim`, `top` | **no**, and it is offered instead | §1.2; the node says what was running and offers to start it |
| the *process*, still running | **no** | §1.1; it needs a server that outlives the window |
| an agent's conversation | **partly** | `claude --continue`, which `task-1907` already built |

**What a person gets for the reported case** is: the `ls` terminal comes back showing the `ls` output, and the
`claude` terminal comes back at a prompt with a button that says `Start claude Again` which continues the
conversation. That is less than *"both exactly restored"* and it is what is honestly available; §7 says what
the missing half would cost.

## 2. A macOS bundle opened from the Dock is a desktop launch

**Reported.** *"If I quit Unluminous, and open back up, it's supposed to open the windows I had open, but it's
never doing that."*

**What is actually happening.** §0 has the measurement. `started_from_the_desktop` is one condition — the
working directory is the folder the program lives in — and that is true of an installed `unluminous.exe` on
Windows, which is what `task-1670` wrote it for, and false of every macOS launch. So `Store::open_windows`
was read on no launch anybody makes on this platform and its answer was discarded.

The narrowness is deliberate and has to be kept: `unluminous .` typed in a folder must open that folder and
nothing else. What was missing is that the same intent has a second shape.

**What changes.** A second condition, named for what it is, and either is enough:

```rust
pub fn started_from_the_desktop(current_directory: &Path, program: Option<&Path>) -> bool {
    let beside_the_program = /* the Windows shape, unchanged */;
    beside_the_program || opened_as_a_bundle(current_directory, program)
}
```

`opened_as_a_bundle` is two questions and both have to be yes. The working directory is **exactly `/`**, which
is where `launchd` leaves a bundle it opened and nowhere a person opens an editor from; and the binary is at
`…/Something.app/Contents/MacOS/…`, which is the only layout a bundle's executable has. The first is what says
the working directory was not chosen — it is the half that keeps a person standing in a project safe, because
their working directory is that project.

**What proves it.**

- `a_macos_bundle_opened_from_the_dock_is_a_desktop_launch` — the measured launch conditions, made a test. It
  fails on the code as it was, which is what makes the report reproducible rather than reported.
- `a_bundles_binary_run_from_a_project_opens_that_project` — the other half: the same bundle, run by its full
  path from a project, opens that project rather than the last one. Without this the fix would break
  `unluminous .` for anybody who typed the path.
- The two existing tests, `started_from_the_desktop_shows_the_project_that_was_open_last_time` and
  `started_from_a_terminal_shows_the_folder_the_person_is_standing_in`, are unchanged and still pass, which is
  what says the Windows shape was widened rather than replaced.

## 3. A page is cut by the pane's edge until there is nothing worth drawing

**Reported.** *"Now the web browser contents disappear if the node is slightly off screen."*

**What is actually happening.** `task-1907` stopped drawing a page cut to less than nine tenths of its width,
and §0 has the arithmetic: a fraction is the wrong measure, because what makes a page reflow is its viewport
crossing a stylesheet's breakpoint and that is an absolute width. Eighty points off a five hundred point page
was enough to blank it.

**And the alternative was measured and refused**, which is §0's table: drawing the view at its whole width and
letting the window clip the overhang works on the right edge and on none of the other three, because the
canvas pane does not reach the window on the left, the top or the bottom — and a native child is composited
above everything `egui` draws, so a page past those edges is a page over the activity rail, the tabs or the
status bar.

**What changes.** The threshold becomes a width rather than a fraction:

```rust
/// How narrow a browser node's page may be cut to before it is not drawn at all.
const PAGE_REFLOW: f32 = 200.0;
```

Two hundred points, which is well below the narrowest breakpoint in common use. Two things follow and the
second is what the first attempt got backwards. A page still hundreds of points wide is drawn **cut**, because
it is in the same layout it was in and what is missing is the part off the edge — which is what cropping looks
like. And a page whose node is *already* narrower than this is drawn cut too, always: it is in its phone
layout because of the size somebody gave the node, and cutting it further does not change which layout it is
in, so there is nothing to protect it from.

What the threshold catches is a page reduced to a strip. There the node keeps its toolbar and says the page is
elsewhere, which is the sentence `browser_view::show` already says for a second rendered tab — reached by not
pushing a placement rather than by a second mechanism.

**What proves it.**

- `a_browser_page_is_cut_by_the_edge_until_there_is_nothing_worth_drawing` — the report and its opposite in
  one test: a node sixty points off the edge keeps its page, and one cut to a hundred and forty points of nine
  hundred does not.
- The existing `a_browser_nodes_page_is_placed_inside_the_node` is unchanged, which is what says the ordinary
  case is untouched.

## 4. A terminal comes back showing what was on it

**Reported.** *"I have 2 terminals, one with claude code, and one with `ls` command executed. When I quit and
reopen, I want both exactly restored so I see claude code and the contents of `ls`."*

§1 is the honest scope: the `ls` terminal can come back showing the `ls` output, and the `claude` one cannot
come back running `claude`. This section is how the first half is built.

### 4.1 What is saved is bytes, not text

`Session::written_text` exists and is the wrong thing to save. It walks the grid taking each cell's
**character**, so what comes back is `total 48` in the editor's foreground colour with no bold, no green
directory names and no cursor anywhere in particular. A restored terminal that lost its colours would be a
restored terminal somebody can see is not the one they left.

So what is written down is what a terminal is written to with in the first place: a byte stream, in the same
escape sequences a program would have sent. `unluminous_terminal::replay` is that, and it has one function
each way — `Screen` and the scrollback into bytes, and bytes into a session.

**Only the normal buffer**, and §1.2 is why: the alternate screen is cleared when it is entered and left, so a
program's own full-screen display is not a thing that can be saved. A session sitting in the alternate buffer
when the window closes has its **normal** buffer written down, which is the shell's — the prompt the person
typed the program's name at.

**And the cursor's position is part of it**, because a prompt with the cursor at the start of the line is not a
prompt. The last thing the stream does is put the cursor where it was.

### 4.2 A live session can be written to, which it could not be before

`Session::feed` returns immediately unless `self.parser` is `Some`, and only `Session::detached` has one — a
session with a shell has the reader thread's parser instead. That is right for input, because two parsers over
one stream would interleave, and wrong for this: replaying is not input, it is putting a screen back before
anything has been read.

So `Session::replay` writes to the `Term` directly, through a parser of its own, and is refused once anything
has been read from the shell. That is the whole of the rule and it is what makes it safe: a replay before the
first byte cannot interleave with anything, and a replay afterwards is refused rather than mixed in.

### 4.3 Where it is written down, and why not in `space.conf`

A screen is kilobytes and `space.conf` is a settings file somebody can read and edit. So each terminal node's
bytes go in a file of their own, `.unluminous/terminals/<node>.bytes`, written when the window closes and read
when the node starts. Three rules:

- **Written on exit only.** A screen changes on every keystroke and `Space::is_dirty` exists to stop the canvas
  being written sixty times a second. What a person wants back is the last state, so it is written once, in
  `on_exit`, beside the canvas.
- **Bounded, by dropping rows and never by cutting bytes.** `REPLAY_LIMIT` caps what is kept per node, because
  a build log is megabytes and a project folder is not a place to put megabytes without saying so. The first
  version of this took the last `REPLAY_LIMIT` bytes, and that is a real bug rather than an approximation: the
  stream is mostly `\x1b[0;38;2;232;235;241;48;2;26;31;38m`, so a byte cut lands inside a sequence and the tail —
  `35;241m` — is **printed** rather than obeyed. `bytes_within` drops whole rows from the top instead, which
  keeps every stream a valid stream, and `a_bounded_stream_is_still_a_valid_stream` asserts the property rather
  than the mechanism.
- **Thrown away when it is used.** A file read into a node is deleted, so a canvas that fails to restore does
  not replay yesterday's screen for ever.

### 4.4 And what cannot come back says so on the node

A node that was running a full-screen program comes back at a prompt with its scrollback replayed and the offer
`task-1907` already built — `Start claude Again`, which continues the conversation. What is added here is that
the two cases are told apart: a node whose program was still running when the window closed is **marked**, so
the person can see that the screen they are looking at is the shell's rather than the program's. iTerm2's
reverse-video *Session Restored* banner is the same idea, and §1.1 quotes it.

**What proves it.**

- `a_screen_written_down_and_replayed_is_the_same_screen` — with no shell, through two detached sessions: build
  a screen with colours and a cursor position, turn it into bytes, feed a fresh session, and compare the
  `Screen` values cell for cell. This is the whole of §4.1 and it needs no window.
- `a_replay_keeps_the_colours_and_the_cursor` — asserted separately, because a test comparing only characters
  would pass on the thing `written_text` already did.
- `only_the_normal_buffer_is_written_down` — a session put into the alternate buffer, whose replay is the
  normal one. §1.2's rule, made a test rather than a comment.
- `a_session_that_has_read_from_its_shell_refuses_a_replay` — §4.2's rule, which is what keeps the mechanism
  from interleaving with a program's own output.
- `a_terminal_node_comes_back_showing_what_was_on_it` — through the harness: a node fed known bytes, written,
  read into a second window, and the screen matches.
- **And the real window**, which is the layer the report was written from: two terminal nodes, one running
  `claude` and one having run `ls`, closed and opened.

## 5. Two things reported later, and only one of them is answered here

### 5.1 A page spilling into a pane on the right

**Reported.** *"The web browser node content spills over into other panes that are on the right."*

**What is known.** The placement handed to the view is clipped to the canvas pane — `whole.intersect(body)` —
so the bounds are right, and the node's own toolbar is visibly clipped at the pane edge in a screenshot of the
reported arrangement (the canvas docked left at `x 161..603` with Agent-Chat to its right). **What a screenshot
cannot show is the page**, because a rendered page is a native child the operating system composites above the
surface `ViewportCommand::Screenshot` captures — `documentation/overview.md` records that and `task-1904`
measured it.

So this is not diagnosed. What it is **not** is the bounds being unclipped, which is the obvious explanation
and is ruled out by the placement arithmetic and by the toolbar. What is left to check is the two places a
placement can be stale rather than wrong: the reconciliation in `raw_input_hook` runs against the placements
the *last* frame recorded, so a pane that moved this frame has a view still at last frame's bounds for one
frame; and `set_bounds` is only called when the answer moved, so a view whose pane changed while the node did
not may keep bounds nothing recomputed.

Both are one-frame or one-change effects, which is consistent with *"spills over"* being seen while a pane is
being dragged. Neither is confirmed, and this section says so rather than shipping a guess.

### 5.2 The pane arrangements need testing of their own

**Reported.** *"We need thorough testing of pane arrangement and show/hide functionality. I'm seeing issues
where I can move agent chat to the right of base of infinite space."*

That is a ticket rather than a section: five panels, four sides, an order within each side, a show and a hide
each, and a plugin pane that is not in `panel list` at all — which is itself a finding from driving this, since
`panel list` reports the five built-in panels and says nothing about the panes a plugin contributes, so an
agent cannot see the arrangement the person is describing. The permutations want generating rather than
writing out by hand, which is what a table-driven test over `dock::Side::ALL` and `Panel::all` is for.

**Not attempted here**, and the reason is scope rather than difficulty: this ticket is three reports about
restoring and one about a native child, and a systematic pass over the docking layout is a larger piece of work
that deserves its own measurements. §7 records it.

## 6. Tests

Four layers.

1. **Unit tests with no window.** The two launch shapes; the replay round trip, its colours, its cursor, the
   alternate-buffer rule and the refusal after a program has written; the trimming of blank rows and the keeping
   of coloured spaces.
2. **`unluminous-terminal` against a real shell.** `a_session_that_has_read_from_its_program_refuses_a_replay`
   spawns a real `bash`, because the flag it tests is set by the *reader thread* and a detached session has none
   — a test that used a detached session would have passed on a rule that was never applied.
3. **Screenshot tests.** `a_terminal_node_comes_back_showing_what_was_on_it` writes a node's screen, reads it
   back and asserts the `ls` output is in the fresh session; and it runs on **a folder of its own**, which is
   `task-1906` §4.8's rule about any test that calls `restore_project`.
4. **The real window**, which is where all three reports came from and where two of them were confirmed fixed:
   a terminal node given a command, the window quit through `unluminous-cli quit`, and the node reopened showing
   the command, its output and its prompt — photographed.

**And one thing about the real window that is worth writing down**, because it cost twenty minutes: `on_exit`
runs on a **clean quit** and not on `kill`. A screen written down there is not written when the process is
killed, so a test of this that kills the window measures nothing. `unluminous-cli quit` is the way to close one.

## 7. What is deliberately left out

- **The process itself.** §1.1: a program outlives its editor only if the editor was never its parent, which
  means a server that owns the pseudoterminals and has its own lifetime, socket and upgrade story. iTerm2 does
  exactly this and says so; tmux is the same shape. It is a different program, not a bigger version of this one.
- **A full-screen program's own screen.** §1.2: the alternate buffer is cleared when it is entered and abandoned
  when it is left, so there is nothing to save. What a node does instead is say what was running and offer to
  start it, which is `task-1907`'s control.
- **The scrollback above the screen.** What is written down is the screen. The history behind it is bounded by
  `SCROLLBACK` at ten thousand lines and would be megabytes in a project folder; a later ticket can widen
  `REPLAY_LIMIT` to include some of it if anybody asks for it.
- **A page that spills into another pane.** §5.1 says what is known and what is ruled out. It is not diagnosed
  and is not guessed at.
- **A systematic pass over the pane arrangements.** §5.2, and it needs its own ticket. The one thing found while
  looking at it is worth carrying into that: `panel list` reports the five built-in panels and nothing about a
  plugin's pane, so the arrangement a person is describing is one an agent cannot read back.
