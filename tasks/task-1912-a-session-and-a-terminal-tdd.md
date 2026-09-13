# task-1912 — the windows that were open, and a terminal that really comes back

Two reports against 0.42.0, and both are about restoring something that `task-1908` already claimed:

1. *"When I close and reopen, all projects I've ever opened are reopened, rather than just the windows I had
   opened."*
2. *"Base of Infinite space terminal node needs to restore the exact state of the terminal. Eg if I've ran 5
   commands, all 5 and their results should be shown when I reopen. Eg if I have Claude code open in terminal,
   I should have the exact session opened."*

The first is a rule that was never written: a line goes into `session.txt` when a window opens and nothing ever
takes one out, so the file is a list of every project that has had a window rather than of a session.

The second is three separate faults, and the middle one is why `task-1908` was reported fixed and is not:
**on Windows the pseudoconsole owns the screen, and anything drawn on it from outside is erased.** That was
measured rather than reasoned about, and §2 is the measurement. It also explains why the feature worked on the
machine it was built on: the Mac has no pseudoconsole.

Every section says what was reported, what was measured, what changes and what proves it.

## 0. What was reproduced first, on the installed build

**The session file on this machine, read before anything was changed:**

```text
C:\jason\dev\unluminous\_agent_output\task-1813-performance-review\v0.37.1-src
C:\jason\dev\unluminous\_agent_output\task-1839-layout-memory\clean-corpus
C:\jason\dev\unluminous\_agent_output\task-1814-inillucent
C:\jason\dev\unluminous\_agent_output\task-1839-layout-memory\corpus
C:\jason\dev\inillucent
C:\Users\jason\Downloads\inillucent-0.1.0-x86_64-pc-windows-msvc
C:\Users\jason\AppData\Local\Temp\unluminous-space-check
C:\jason\dev\unluminous
```

Eight projects, `SESSION_LIMIT` exactly, and six of them are agent scratch folders from tickets weeks old. That
is the report: a launch from the desktop opens all eight.

**And the terminal, driven through the real window.** A canvas with one terminal node, five commands run in it,
`unluminous-cli quit`, and the project opened again:

| | |
|---|---|
| `.unluminous/terminals/2.bytes` after the quit | **457 bytes** — six rows, the last screenful |
| the five commands in it | **two of them.** `ONE`, `TWO` and the `dir` output were already off the screen |
| what the node showed when the project was opened again | `PowerShell 7.6.6` and a prompt. **Nothing was restored.** |
| `2.bytes` afterwards | **gone** — read, consumed, and thrown away |

`_agent_output/task-1912-restore/` holds both screenshots and the saved bytes.

## 1. A session is the windows that were open, not every window there has ever been

**Reported.** *"all projects I've ever opened are reopened, rather than just the windows I had opened."*

**What is actually happening.** `Store::remember_open_window` appends a folder and `Store::write_session` — whose
own comment says it is *"what restoring does once it has started them all"* — **has no caller outside the
tests.** So the list only ever grows, bounded by `SESSION_LIMIT` at eight, and every desktop launch opens
whatever the last eight projects were. `task-1693` wrote the trade-off down honestly as far as it went —
*"closing one window while another is open still brings both back"* — but the file has no idea when one run of
Unluminous ends and the next begins, so the trade-off is much larger than the sentence describes.

**What other editors do**, and it is the same shape in each: they restore *the last session* rather than a
history. VS Code's `window.restoreWindows` defaults to `all`, which its documentation defines as *"restore all
windows you worked on during your previous session"* — a session, with a beginning and an end, and VS Code can
say where those are because its windows are one process. Unluminous's windows are one process **each**, which is
what `services::launcher` records as a deliberate decision, so the beginning and the end have to be derived
from the file itself.

**What changes.** A row gains the process id of the window that wrote it:

```text
49308 C:\jason\dev\unluminous
51120 C:\jason\dev\inillucent
```

and one rule uses it:

> **A window that opens while no listed window is still running is the first window of a new session, and the
> file becomes that one row.** A window that opens while another is running joins the session and appends.

A line with no process id in front of it is read as a path with a dead process, so a file written by 0.42.0 is
read by this version and resets on the first launch.

Three things follow, and each is the answer to a case that was got wrong before.

- **Quitting three windows and starting again brings back three windows.** Closing takes no row out, so after
  the last one closes the file still holds all three, all dead. The first window to start restores them, and
  because nothing was alive when it registered, it also *replaces* the file with the session it is beginning.
- **A project opened once by hand and closed does not come back a week later**, because the next start of
  Unluminous is a new session and resets the file.
- **The list cannot drift while three windows start at once.** `main` knows the process id of every window it
  launches — `launcher::open_window` answers with it now — so the restoring window writes the whole session
  itself, and each restored window finds its own project already listed and writes nothing. The read-modify-write
  that `remember_open_window`'s comment worried about is not performed at all in the case it worried about.

**What is deliberately kept.** A window closed in the middle of a session still comes back at the next start,
because the last window to close cannot know which of its predecessors were deliberate. This is the trade-off
`task-1693` named, now bounded to one session rather than to the life of the settings folder, and it is what
VS Code's `all` does in the one case it can be compared to.

**And liveness is a listed instance, not a bare process id.** `instances::is_running` alone would be fooled by a
process id the operating system handed to something else, and a fooled answer keeps the file from ever
resetting — which is exactly the reported fault, returning by a side door. A row is alive when a **listed
Unluminous instance** has that id and that process is running.

**What proves it.**

- `a_new_session_replaces_the_windows_of_the_last_one` — three dead rows, a window opening, and a file holding
  one row. Fails on the code as it was.
- `a_window_that_opens_beside_a_live_one_joins_its_session` — the same three rows with one of them alive, and a
  file holding four.
- `the_windows_of_one_session_all_come_back` — three rows written, all dead, and `open_windows` answering with
  all three: the reported case, which must keep working.
- `a_session_file_from_an_older_version_is_read_and_replaced` — bare paths with no process id.
- `a_project_opened_once_and_closed_does_not_come_back_next_week` — open, close, open something else, and the
  first is gone.

## 2. On Windows the pseudoconsole owns the screen, and that is why nothing came back

This is the finding the whole of §3 rests on, so it is measured here before anything is designed.

**The first measurement was the real window.** A probe put into `start_a_space_terminal` recorded what happened
on a restore, and it said the opposite of the report:

```text
start_a_space_terminal frame 1 — node 2: 738 bytes, replay accepted: true
```

The screen was found, the replay was **accepted**, and the node was empty. So the question was not why the
replay failed but what happened to it afterwards. Reading the node's own grid frame by frame answered that:

```text
frame 2  grid 14x46  screen ["PowerShell 7.6.6", "PS …> echo EPSILON", "EPSILON", …]
frame 3  grid 14x46  screen [… the same …]
frame 5  grid 14x46  screen []
frame 8  grid 14x46  screen ["PowerShell 7.6.6"]
frame 12 grid 14x46  screen ["PowerShell 7.6.6", "PS C:\…\project-a>"]
```

The restored screen is there, and then the screen is **cleared** and the shell's own banner is drawn on the
empty grid. What was replayed is not lost — it is in the scrollback, above the viewport, which is why
`written_text` went on answering with it and why the first reading of this looked fine.

**And it is the console host, not the shell.** `cargo run -p unluminous-terminal --example replay_probe` spawns a
real shell, replays a marker, and then watches the **screen** rather than the text:

| shell | the replay is gone from the screen after | still in the scrollback |
|---|---|---|
| `pwsh.exe` (the default here) | 250 ms | yes |
| `cmd.exe` | 250 ms | yes |
| `pwsh.exe -NoLogo -NoProfile` | 500 ms | yes |
| `powershell.exe -NoLogo -NoProfile` | 1500 ms | yes |

Three different programs, two of them started with no banner and no profile, all clearing the screen. What they
have in common is the pseudoconsole. Microsoft's own documentation for `CreatePseudoConsole` says why: the
`dwFlags` value **0** is *"a standard pseudoconsole creation"*, against
**`PSEUDOCONSOLE_INHERIT_CURSOR`**, which is *"the created pseudoconsole session will attempt to inherit the
cursor position of the parent console"* — the flag that exists so a console can be handed a terminal that
already has something on it. `alacritty_terminal` 0.26's `tty/windows/conpty.rs` passes `0`.

**And drawing over it later is worse than not drawing at all.** The obvious repair is to wait until the console
host has finished its one repaint and put the screen back then. Measured, with the same probe forced to draw
after a second and a half:

| what happened next | the screen afterwards |
|---|---|
| nothing | the replay is still there after two seconds |
| one command typed | `echo AFTERWARDSNE` / `AFTERWARDSINE-TWO` — the restored text half overwritten, *visibly corrupt* |
| a resize | gone |

The console host redraws the cells it believes it owns, and it believes it owns all of them. So there is no
moment at which Unluminous can put a screen back into a live terminal on Windows: early is erased, late is
corrupted by the next keystroke, and either is erased by a resize.

**What follows is the design rather than a workaround.** If the screen belongs to the pseudoconsole, then what
is restored has to *come out of* the pseudoconsole — it has to be printed by a program inside the console, like
any other output. Then the console host holds it, its repaints keep it, a resize reflows it, and the lines that
scroll off reach Unluminous's own scrollback exactly as a real command's output does.

## 3. A terminal node comes back by printing what was on it, and then becoming the shell

**What changes.** A node with a screen to restore starts a **shim** in its pseudoterminal instead of the shell.
The shim writes the remembered bytes to its standard output and then becomes the shell:

```text
unluminous-cli --replay-screen <file> -- <shell> [args…]
```

`unluminous_cli::restore` owns both ends of that — the command line it builds and the function that runs it —
because a protocol split across two crates is a protocol with two chances to disagree.

**And it is `unluminous-cli` rather than the window's own binary, which was measured rather than chosen.** The
first version of this used `unluminous.exe`, and the node came back perfectly blank: not the restored screen,
not the shell's prompt, nothing at all. `alacritty_terminal` creates a pseudoconsole's child with
`STARTF_USESTDHANDLES` and every handle left **null** — its own comment says that is so the child inherits
nothing from the editor — and Windows fills a **console** subsystem program's standard handles in from the
console it is attached to while a **windows** subsystem program's stay null. So the shim printed into nothing,
and the shell it started inherited the same null handles and printed into nothing as well.
`unluminous-cli` is a console program, is installed beside the window, and is already the program a node's own
environment points at.

`SessionSettings` gains one field, and it is not the restore: `name`, what to call the tab when the program
that is started is not the program that matters. Three things follow:

- **the tab is still named after the shell.** `Session::name` also discounts a title that says no more than
  the name of the program that was started, because a pseudoconsole sets its title to the program it was
  created with — so without that the node came back with `…\unluminous-cli.exe` across its header;
- **nothing changes for a node with nothing to restore**, which is every node except the first start after a
  window was closed;
- **a shim that is not there is not fatal.** If the file or the program is missing the shell is started
  plainly, because a terminal that will not open is worse than a terminal that opens empty.

**And `Session::replay` is gone with it.** `task-1908`'s way of putting a screen back — writing the bytes into
the emulator before the program had written — works on macOS and cannot work here, so leaving it as a public
function would leave a second way in that quietly does nothing on Windows. `Session::draw_over_the_terminal`
stays, because `examples/replay_probe` is what measures §2 and needs to draw over a live terminal on purpose.

**On Unix the shim replaces itself.** `CommandExt::exec` means the process that printed *is* the shell, so
nothing is added to the process tree, `Master::foreground` reads what it always read, and the exit code is the
shell's by construction. On Windows there is no `exec`, so the shim spawns the shell, waits for it and exits
with its code — and the two places that care are answered in §5.

**The file is taken away by whoever used it.** The shim deletes it once it has printed it, and a node that
starts with no restore deletes any file that is lying there, so a canvas that failed to come back does not
replay a week-old screen for ever. That is `task-1908`'s rule, kept, and moved to the two places that now know.

**And it is one mechanism on both platforms**, though only Windows needs it. Two mechanisms would mean the
screenshot tests on one platform proved nothing about the other, which is exactly the hole this ticket fell
into: `task-1908` was verified on macOS, where a shell writes into a terminal that keeps what it was given.

**What proves it.**

- `the_command_line_a_restored_terminal_is_started_with` — a unit test over `restore::command_line`, with no
  process: the shim, the file, the separator and the shell with its own arguments, in that order.
- `what_the_shim_reads_is_what_the_command_line_wrote` — both ends of the protocol in one test, so they cannot
  agree today and drift tomorrow.
- `a_malformed_shim_command_line_is_not_one` and `an_ordinary_start_is_not_a_shim_invocation` — a half-written
  command line opens a window rather than starting half a shell.
- `a_restore_whose_file_has_gone_is_not_worth_trying`.
- `a_terminal_node_comes_back_showing_what_was_on_it` — through the harness: the screen written down, the
  command line the node would be started with, and the bytes that program will print read by a terminal.
- `a_screen_nobody_printed_is_not_kept_for_ever` — the other way a file stops existing.
- **And the real window**, which is the layer the report was written from, and the measurement it produced:
  `examples/replay_probe` with `PROBE_SHIM`, where the restored screen survives a typed command — new output
  landing below it, as history should — and survives a resize. Both of those destroyed every other approach.

## 4. Five commands means the scrollback, not the screen

**Reported.** *"if I've ran 5 commands, all 5 and their results should be shown when I reopen."*

**What is actually happening.** `Session::screen_to_replay` answers with `bytes_within(&self.snapshot(), …)`,
and `snapshot` is the **visible grid**. A node 380 points tall is about fourteen rows, so what was written down
was the last fourteen rows and the first commands were already gone — 457 bytes for five commands, in §0's
table.

**What changes.** What is written down is the last `REPLAY_ROWS` rows of the scrollback **and** the screen,
which is what `Session::written_text` has always read and what `bytes_of` was never given.
`Session::screen_and_history(rows)` is that reading, and it answers a `Screen` so that `replay::bytes_of` is
unchanged and still tested the way it was.

**A thousand rows**, bounded by `REPLAY_LIMIT` bytes as before, whichever binds first. It is a deliberate
multiple of what the surveyed tools keep — VS Code's `terminal.integrated.persistentSessionScrollback` is
**100 lines** by default — because a project folder can hold a quarter of a megabyte without anybody minding
and because five commands with real output is more than a hundred lines more often than not. The bound is still
applied by dropping whole rows from the top, which is `task-1908`'s own correctness rule about never cutting a
stream inside an escape sequence.

**What proves it.**

- `what_is_written_down_holds_the_scrollback_and_not_only_the_screen` — a detached session fed forty rows on a
  fourteen row grid, whose written stream holds the first row.
- `five_commands_and_their_output_all_come_back` — the report's own arithmetic, through two detached sessions.
- The existing `a_screen_written_down_and_replayed_is_the_same_screen` is unchanged, which is what says the
  round trip was widened rather than replaced.

## 5. Windows can say what a terminal node is running, and until now it could not

**Reported.** *"Eg if I have Claude code open in terminal, I should have the exact session opened."*

**What is actually happening.** `task-1907` built all of this: a node records the program running in it, a
restored node starts that program again, and `claude` is started with `--resume <id>` so the conversation
continues. `unluminous_terminal::foreground` is what reads the program — and its own module comment says
*"Windows answers nothing"*, because a ConPTY is a pipe rather than a controlling terminal and there is no
foreground process group to ask for. The same comment names the fix: *"a process tree walk from the child
handle `reap::Reaper` already holds on that platform."*

So on this machine the feature has never existed. A node where somebody typed `claude` records nothing, comes
back a bare shell, offers nothing, and resumes no conversation. That is the second sentence of the report,
whole.

**What changes.** `Master` keeps the child's process id on Windows — taken in the same window `Reaper` takes
the handle, which is the last moment either is reachable — and `foreground` walks the tree below it with
`CreateToolhelp32Snapshot`, answering with the name of the **deepest, newest descendant**. Three rules make
that a reliable answer rather than a plausible one:

- **The newest child at each level**, by creation time, because a shell that has run two programs has two
  children only while the first is still exiting.
- **The walk stops at the first thing that is neither a shell nor the shim**, which is the job the terminal
  started. Going all the way down was the first version and it is wrong in exactly the case the feature exists
  for: `claude` starts programs of its own for its tools, so a node running `claude` with a `bash` open under
  it would have been recorded as running `bash`. It walks *through* a shell, though, because a program is
  often reached by one.
- **The shim is stepped over**, so a node restored through §3 reports its shell rather than `unluminous-cli`.
- **A shell is still written down as nothing**, which `services::space::launch::is_a_shell` already decides, so
  a node at a prompt offers nothing to restart. The Windows answer goes through the same filter as the Unix one
  rather than beside it.

**What proves it.**

- `a_real_program_is_seen_running_under_a_shell` — a real shell with a real long-running program under it, and
  the name of that program. It has a tree of its own rather than walking from the test binary, which is what
  makes it its own: the first version walked from this process and found whatever shell another test in the
  same binary had just started.
- `a_process_with_nothing_under_it_is_running_nothing`.
- `the_shim_is_not_what_a_node_is_running`.
- And the real window: a node given `ping`, whose `space.conf` records `running = PING` — which on this
  platform it never has.

## 6. An agent can read a terminal node, which it could not before

`space view` answers with a node's command, folder, size and session id, and there is **no way at all** to read
what a terminal node is showing — `terminal read` reads the terminal *panel*. So the one thing this ticket is
about is the one thing an agent cannot check, which is the rule this repository opens with turned on its head,
and it is why every measurement above had to be a screenshot of a window.

`unluminous-cli space read <node> [--tail <lines>]` answers with the node's text, through
`Session::written_text` — the same reading `run output` uses, and the same distinction: the scrollback as well
as the screen, because a restored node's whole point is what is above the fold. The MCP tool follows from the
catalogue with no further work, and `documentation.rs` requires the section in `commands.md`.

## 7. Tests

Four layers, which is the house arrangement.

1. **Unit tests with no window.** The session file's four rules; the restore command line; the scrollback
   capture and its bounds; the replay round trip, unchanged.
2. **`unluminous-terminal` against real processes.** The foreground walk, which cannot be a unit test because a
   detached session has no process to ask; and `examples/replay_probe`, which is how §2's table is measured
   again on a machine that disagrees.
3. **Screenshot tests.** A restored node, through the harness, on a folder of its own.
4. **The real window.** Both reports: three projects opened, quit and reopened; and a node given five commands,
   quit and reopened, read back with `space read` and photographed.

## 8. What is deliberately left out

- **The process itself.** A program outlives its editor only if the editor was never its parent.
  `task-1908` §1.1 settles this with tmux's and iTerm2's own documentation and nothing here changes it: what
  comes back for `claude` is the conversation, through `--resume`, and not the process.
- **A full-screen program's screen.** `task-1908` §1.2: the alternate buffer is cleared when it is entered and
  abandoned when it is left, so there is nothing to save. Such a node restores by starting the program again.
- **Patching `alacritty_terminal` to pass `PSEUDOCONSOLE_INHERIT_CURSOR`.** It is the flag this problem was
  invented for, and it would mean carrying a fork of the terminal emulator. It also requires answering the
  console host's cursor request on a background thread, and Microsoft's own documentation says that *"failure
  to do so may cause the calling application to hang"* — a hang in the one path that starts every terminal.
  The shim needs no fork and no answer.
- **Restoring the scrollback above what is printed.** What the shim prints becomes the console's own output, so
  the rows that scroll off reach Unluminous's scrollback naturally. Nothing is put there by hand.
- **Trimming the banner a restart leaves behind.** Each quit and reopen prints the remembered screen and the
  new shell then prints its own greeting, so a node restarted four times shows four greetings in its history.
  That is what a scrollback *is*, it is bounded by `REPLAY_ROWS`, and recognising a shell's greeting in order
  to drop it would be a list of shells inside the restore.
- **The 180 screenshot comparisons that fail on this machine.** Measured on a clean checkout of `main` as well
  as on this branch, with the same count either way, so they are a baseline that drifted before this ticket and
  are its own piece of work.
