# The terminal

A tile along the bottom of the window with tabs, opened with `Ctrl` and backtick, from the `View`
menu, or at start with the `--terminal` switch. It can be moved to any edge like any other panel, and
a side can be made to run its whole length. See [The window](the-window.md#the-panels-and-moving-them).
`View -> New Terminal Tab`, `Close Terminal Tab` and `Rename Terminal Tab...` are the tab commands.
A window that is waiting for a project starts its terminal in the home folder.

## Which shell

Each tab runs a shell in the folder the explorer is showing: `$SHELL` on macOS, and on Windows
`pwsh.exe` when it is installed and `powershell.exe` otherwise. `terminal.shell` in the settings, on
`Settings -> Terminal`, is how to ask for something else, and empty means "what this machine says".
`terminal.font.size` is the size of the grid's text, and zooming over any terminal tile changes it.

**`COMSPEC` is not the shell**, and reading it is the fault this replaced. It names the interpreter
that runs a batch file and says `cmd.exe` on every Windows there is, so a terminal that read it never
held the commands in a person's PowerShell profile. `pwsh` and `powershell` read **different**
profiles, so choosing between them is not a preference between two spellings of one shell.

**A path handed to a shell is plain.** `std::fs::canonicalize` on Windows gives back a verbatim path —
`\\?\C:\jason\dev\unluminous` — and every Rust file call takes one happily, so nothing inside
Unluminous notices while it travels: into `recent.txt`, onto the explorer's root, and from there to
the directory a shell is started in. `cmd.exe` is where it stops, because two leading backslashes are
a network share as far as it is concerned; it says so and starts in `C:\Windows` instead, which is a
terminal that opens, works, and is quietly in the wrong folder. The prefix is taken off in three
places, because the window is not the only thing that hands a directory over and a list of the places
that have to remember is a list whose next entry will be the one that forgot.

## What the emulator handles

Colour including 24 bit colour, bold, italic, underline, strikethrough, inverse and dim. Wide
characters. The alternate screen a full screen program draws on. Ten thousand lines of scrollback.
Selecting with the mouse, copying, and mouse reporting for a program that asked for it. A click with
`Ctrl` or `Cmd` held on a web address in the output opens it, and only `http` and `https` addresses open.

The grid's colours follow the theme. The ground, the text and the cursor are the theme's own editor
colour, text colour and accent, and the sixteen colours follow whether the theme is dark or light. A
terminal moves onto a new palette the next time it is drawn, and a program that asks the terminal for
its background colour is told the theme's.

`alacritty_terminal` supplies the escape sequence emulation and the pseudoterminal; the session, the
screen the painter reads, the colour palette, the key encoding, the mouse reports and the decision
about which shell to start are Unluminous's.

The seam is a **screen**: a snapshot of the grid with no locks in it and nothing borrowed. A frame
takes one while holding the emulator's lock and then draws from it with the lock released, because
drawing touches the font atlas and the graphics device, and holding the lock across all of that would
stall the thread reading the shell.

## The tabs

A tab is named after the title the program set, so a tab running `claude` says so. A tab can be
renamed by hand from its right click menu or `View -> Rename Terminal Tab...`, and a name a person
typed **beats a name a program set**: `claude` sets a title on every prompt, so a rename written into
the title would appear to work and then quietly undo itself the next time the program spoke. An empty
name puts the tab back to being named after its program.

**A name a person typed is never numbered.** The numbering that tells two tabs running the same
program apart counts the names the sessions give rather than the names already worked out — which was
a real fault: `powershell.exe 2` does not end with `powershell.exe`, so the third shell was a second
`powershell.exe 2`.

A tab is dragged along the strip to reorder it. There is one strip of terminal tabs, so the strip a
tab is picked up from is the strip it is dropped on, and nothing outside it could know better where it
landed.

## The keys

**What a menu calls a key is not what a terminal sends.** The menus spell the punctuation as words —
`Backslash`, `OpenBracket` — because `Ctrl+Backslash` reads better in a menu. A word is not one
character, so asking the menus meant `Ctrl+]`, `Ctrl+\` and `Ctrl+Space` were sent as nothing at
all — and `Ctrl+]` is how a person detaches from `claude`. The terminal asks its own question instead.

A **shifted** digit or symbol is refused, because `Shift+4` is `$` here and `"` on a British layout
and there is no control code to be had from a key whose character depends on the keyboard. A letter is
untouched, since `Ctrl+Shift+C` is `Ctrl+C` in every terminal there is.

**`Ctrl+C` is a copy on Windows and an interrupt everywhere.** The interface library asks whether a
press is a clipboard command before it pushes a key event, and its test is `modifiers.command && key
== C`. On macOS `command` is the Apple key, so `Ctrl+C` is an ordinary key press and always worked; on
Windows `command` is the control key, so every `Ctrl+C` became a copy with no key event behind it. The
choice is made the way every terminal on Windows makes it: something selected and `Ctrl+C` copies it
**and lets go of the selection** — left behind it would swallow the next press too — nothing selected
and it interrupts, `Ctrl+Shift+C` always copies, and `Ctrl+X` reaches the program as `0x18`, because
nothing in a terminal can be cut and that is how a person leaves `nano`.

## Panes, Termoil's keys and the themes

Every terminal grid in the window has what a Termoil pane has: the terminal tile, the run tile, a terminal
node on the canvas and a ticket's agent terminal. While a grid has the keyboard, Termoil's keys work in it
and come before the window's own menu keys, so `Ctrl+Shift+P` there is the terminal's command palette and
not `Find Action`. On macOS the keys are Termoil's `Cmd` keys, with `Option` added where Unluminous's menus
already hold the key: `Cmd+Option+D` splits right, `Cmd+Shift+D` splits down, `Cmd+Option+W` closes a pane,
`Cmd+Option+K` clears the scrollback, `Ctrl+Option+Tab` moves between tabs, `Cmd+Ctrl+Z` zooms, and `Cmd+Option`
with an arrow moves between panes.

| Keys on Windows and Linux | What they do |
| --- | --- |
| `Ctrl+Shift+D` | Split the pane, the new one on the right |
| `Ctrl+Shift+E` | Split the pane, the new one below |
| `Ctrl+Shift+W` | Close the pane. On a tab's first pane it closes the tab |
| `Alt` and an arrow | Give the keyboard to the pane on that side |
| `Ctrl+Shift+Alt` and an arrow | Move the divider nearest the pane that way |
| `Ctrl+Shift+Z` | Show the pane alone, or put the split back |
| `Ctrl+Shift+P` | The terminal command palette |
| `Ctrl+Shift+Y` | The theme picker |
| `Ctrl+Shift+T`, `Ctrl+Tab`, `Ctrl+Shift+Tab` | A new terminal tab, and the next or previous one |
| `Ctrl+Shift+Up` and `Down` | The previous and the next prompt the shell marked |
| `Shift+PageUp` and `PageDown`, `Ctrl+Shift+Home` and `End` | Scroll a page, or to the top or bottom |
| `Ctrl+Shift+K` | Clear the scrollback |
| `Ctrl+Shift+C` and `Ctrl+Shift+V` | Copy and paste |

`Ctrl+Shift+Z` and `Ctrl+Shift+Y` are Unluminous's own, because Termoil leaves zoom and the theme picker to its
palette. Termoil's font size keys are the window's zoom here, which already changes the terminal font size.
`unluminous-cli terminal keys` lists all of them with the action name `action run` takes for each.

**A right click on a grid** opens a menu of the same commands. `View -> Terminal Panes` has them too, and acts
on the grid that has the keyboard, or on the terminal tab that is showing when no grid does. A pane's divider
can be dragged. A pane whose program ends is closed, the way Termoil closes one.

**The theme picker** lists Termoil's 894 themes, the ones Unluminous ships first, and draws every terminal in
the highlighted theme as the list is moved through. `Enter` keeps it and `Escape` puts the old one back. The
choice is the `terminal.theme` setting, on `Settings -> Terminal`, and an empty setting follows the window's
own theme, which is what a terminal did before.

Termoil's shaders, cursor effects, search and link hints are not here. A shader is a pass over the whole
window that Termoil's renderer runs, and Unluminous draws a terminal with its own painter.

## A tab comes back showing what was on it

A terminal that was open when the window closed comes back as a fresh shell **in the folder it was
in**, with the last screenful and a bounded amount of scrollback printed back into it. What a program
was doing cannot be brought back; what is restored is the same number of shells in the same places.

This holds for the tabs of the terminal tile as well as for a terminal node on the canvas. Each tab's
name and folder are written to `.unluminous/terminal-tabs.txt` as a numbered block, and a file with no
`=` in it is the older format of one name on each line, so a project last opened by an older build
keeps its names. When the strip is shorter than it was, the screens behind it are
forgotten, so a tab opened into that place tomorrow does not replay a conversation that was never its
own.

**The screen is printed inside its own console**, and on Windows there is no other way. The console
host clears the screen the first time the program writes and thereafter repaints the cells it believes
it owns, so a screen written into the emulator from outside is erased — and put back later it survives
until the next keystroke and then comes back visibly corrupt. So a tab with a screen to restore starts
`unluminous-cli --replay-screen <file> -- <shell>`, which prints the bytes and then becomes the shell.
What comes back is then ordinary output of the tab's own console: the host holds it, its repaints keep
it, a resize reflows it, and the rows that scroll off reach the scrollback as any command's output
does.

It is `unluminous-cli` and not the window's own binary, which is also measured: the emulator creates
the child with every standard handle null, and Windows fills a **console** subsystem program's handles
in from the console while a **windows** subsystem one's stay null — so `unluminous.exe` as the shim
printed nothing and the shell under it printed nothing either.

### Where the shell had got to

`Session::folder` reads the current directory off the shell process itself: `/proc/<pid>/cwd` on
Linux, and on Windows the current directory in the process's own parameter block.

**PowerShell's `Set-Location` does not move the process's own current directory**, measured on this
machine and still true after a native command has run — so the process is no answer for a `pwsh` tab,
while `cmd.exe`, `bash` and `zsh` come back where they were.

The one mechanism that answers for PowerShell is **the shell reporting its own folder**, which is what
every terminal with shell integration reads: `OSC 7`, which `vte.sh`, zsh and Starship already write,
and `OSC 9;9`, which Windows Terminal's own snippet writes. Unluminous reads both and prefers what the
shell said. *Do not add a prompt parser instead: a prompt is prose.* What is read is a sequence the
shell wrote on purpose to be read.

**Reading is always on and injecting is a setting**, and the split is the whole design. A shell that
already reports its folder is followed and one that does not is unchanged, so reading has no downside
and needs no switch. Making PowerShell report it means adding to the prompt somebody already has,
which is a change to their shell rather than to this editor — so `terminal.shell_integration` is
**off**, with a tick box on `Settings -> Terminal`.

What it adds is a script started with `pwsh -NoExit -File <script>`. PowerShell processes `-File` after
the profile, so the script wraps whatever prompt the person's profile installed and does not replace
it. A prompt that fails is caught and the sequence is still added, because the one thing this must
never do is leave somebody with no prompt. It applies only to a PowerShell started with no arguments of
its own: a canvas node running `claude`, or a run configuration, was asked to run a particular thing,
and a setting about a prompt does not change that.

Three things about the reading are refusals rather than features. A folder on another machine is
refused, because `file://build-box/C:/jason` would otherwise reopen a tab in this machine's own
`C:\jason`. A reported path that is not a folder here is dropped, checked once a prompt on the reader
thread rather than once a frame on the window's. And a sequence that runs past the largest one this
reads is abandoned rather than buffered, because `OSC 52` carries a whole clipboard.

## What a canvas terminal is running

On Windows a pseudoconsole has no foreground process group, so the tile and the canvas find what a
terminal is running by walking the process tree below the pseudoconsole's child. The walk stops at the
first thing that is neither a shell nor the restore shim, because `claude` starts programs for its own
tools and a terminal running it with a `bash` open under it would otherwise be recorded as running
`bash`. That is what lets a node come back and offer to resume an agent's conversation. The process
table is read once on a thread of its own every two seconds while anything asks, and never on the
window's thread, where reading it once a second per node had stopped the window for 130 to 230 ms.

## From the command line

```sh
unluminous-cli terminal show
unluminous-cli terminal new
unluminous-cli terminal send cargo test
unluminous-cli terminal read --wait-for "test result" --json
unluminous-cli terminal rename --tab 0 "the build"
unluminous-cli terminal split --direction down
unluminous-cli terminal send --pane 1 cargo watch
unluminous-cli terminal panes --json
unluminous-cli terminal theme "Material Palenight"
```

`terminal read` reads the **screen**, which is what its summary says it does. `run output` reads the
scrollback, which is what *its* summary says — a program that printed more lines than the tile is tall
has the start of its output scrolled off, which is exactly the dev server whose port had gone past.

## What it does not do

Images, the Kitty keyboard protocol, a blinking cursor, and searching the scrollback.
`tasks/unluminous-terminal-tdd.md` lists them with the reason for each.
