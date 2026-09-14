# Driving Unluminous from the command line

`unluminous-cli` drives a **running** Unluminous window. Everything the menus, the keyboard and the mouse can
ask for is a command, and every command answers with either a sentence or, with `--json`, a value a
program can read.

This document is written to be handed to an AI agent whole. If you are an agent, read the next two
sections and then work from the reference at the bottom.

**If your client speaks the Model Context Protocol, you do not need this document at all.** Unluminous has
an MCP server, generated from the same list of commands, so the tools are the commands and they
cannot fall behind. `unluminous-cli mcp install claude`, or the buttons in `Settings -> Tools -> MCP`.
[mcp.md](mcp.md) is the whole of it.

## The four things to know first

1. **Always pass `--json`.** Without it you get a sentence meant for a person; with it you get
   `{"ok":true,"command":"…","message":"…","result":{…}}`, and `ok` tells you whether it worked. A
   failure is `{"ok":false,"command":"…","error":{"code":"…","message":"…"}}`.
2. **`unluminous-cli commands --json` prints every command as data** — the areas, the arguments, the
   flags, the examples. It needs no running Unluminous. It is the same list this document is generated
   from, so it can never be out of date.
3. **A command is `unluminous-cli <area> <verb>`** — the thing first, then what to do to it:
   `tab open`, `terminal send`, `modal results`, `settings set`. Six commands have no area:
   `status`, `instances`, `launch`, `quit`, `commands`, `version`.
4. **`unluminous-cli status --json` tells you where you are** — the project, the tabs, the panes, the
   terminal, the modal that is open, the settings and git, in one answer. Start there.

## Getting an Unluminous to talk to

```sh
unluminous-cli instances --json          # the Unluminous windows that are running
unluminous-cli launch C:\jason\dev\unluminous # start one, and wait until it answers
```

`launch` returns only once the new window is answering, so the next command in a script cannot be
too early.

With one Unluminous running, every command goes to it. With several, say which:
`--instance <pid>`, `--instance <port>`, or `--instance <part of the project's path>`. With several
running and none named, the command is refused and lists them rather than guessing.

## Paths

**A relative path is relative to the project folder** — not to wherever you ran the command. That is
one rule for `tab open`, `explorer expand`, `window screenshot`, `tab save-as` and every other path
argument. Give a path in full to reach something outside the project. Every reply says which
absolute path it used, so there is never any doubt.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | It worked. |
| 1 | Unluminous refused it: no such file, no such tab, nothing to undo. |
| 2 | The command line was wrong: no such command, no such flag, a missing argument. |
| 3 | No Unluminous is running, or the one named could not be reached. |
| 4 | Several Unluminous windows are running and none was named with `--instance`. |
| 5 | Unluminous was reached but did not answer in time. |

`2` is your mistake, `1` is Unluminous's answer, and `3`, `4` and `5` are about the connection.

The `error.code` in a JSON reply is the same thing in words: `not-found`, `not-applicable`, `usage`,
`unknown-command`, `refused`, `failed`, `timed-out`, `not-running`, `several-instances`.

## Flags that work on every command

| Flag | What it does |
|---|---|
| `--json` | Print the whole reply as JSON. Always pass this from a program. |
| `--instance <pid\|port\|path>` | Which Unluminous, when several are running. |
| `--quiet` | Print nothing when it worked. The exit code still says whether it did. |
| `--timeout <milliseconds>` | How long to wait for an answer. 15000 by default. Lower it to fail fast. A command that waits for something of its own — `terminal read --wait-for`, `debug start --wait-for-pause`, `git action --wait` — is still waited out in full. |
| `--dry-run`, `-n` | Print the command and arguments that *would* be sent, and send nothing. Needs no running Unluminous, so it checks a script before there is a window. |
| `--no-color` | Never colour the output. `NO_COLOR` in the environment does the same. |
| `--help` | Help for the command, or for the whole CLI. |
| `--version`, `-V` | The version. |

The single-letter forms `-h`, `-n`, `-q` and `-V` are read **only before the command name**
(`unluminous-cli -n tab open x`). After it, only the long `--` forms are Unluminous's — which is what keeps a
shell command's own `-n` safe in `unluminous-cli terminal send git log -n 5`.

## Quoting, and the commands that take the rest of the line

Some arguments swallow everything after them, so a shell command needs no quotes:

```sh
unluminous-cli terminal send git log --oneline -n 5
```

`git log --oneline -n 5` is all sent to the shell. One rule decides what is text and what is not:

> **Unluminous's own flags are recognised anywhere on the line. Anything else is text.**

So `--oneline` is text, because Unluminous has no such flag; `--json` is Unluminous's wherever it sits, so this
does what it looks like:

```sh
unluminous-cli settings set appearance.font.size 20 --json
unluminous-cli terminal send --no-enter cd ..
unluminous-cli terminal send git status --no-enter      # the same thing; --no-enter is Unluminous's
```

To send text that *is* one of Unluminous's flags, put `--` in front of it — after that nothing is a flag:

```sh
unluminous-cli terminal send -- curl --json https://example.com
```

The arguments that take the rest of the line are marked in the reference below. They are
`terminal send`'s text, `editor set-text`, `editor insert`, `explorer filter`, `modal type`,
`window message` and `settings set`'s value.

## Recipes

**Open a file and look at it.**

```sh
unluminous-cli tab open README.md --json
unluminous-cli editor text --from-line 1 --to-line 20 --json
```

**Find a file without knowing where it is.** `Go to File` matches a subsequence, so `mdrs` finds
`markdown.rs`.

```sh
unluminous-cli modal open go-to-file --query mdrs --json
unluminous-cli modal results --limit 5 --json
unluminous-cli modal accept 0 --json
```

**Search the project's text and jump to a match.** The search runs on a thread, so wait for it.

```sh
unluminous-cli modal open find-in-files --query "fn main" --json
unluminous-cli modal results --wait 5000 --limit 10 --json
unluminous-cli modal accept 0 --json
```

**Run a shell command and read what it said.**

```sh
unluminous-cli terminal show --json
unluminous-cli terminal send cargo --version
unluminous-cli terminal read --wait-for cargo --timeout 15000 --json
```

**Edit a file and save it.**

```sh
unluminous-cli tab open notes.md --json
unluminous-cli editor caret --line 1 --column 1 --json
unluminous-cli editor insert "# Notes\n\n" --json
unluminous-cli tab save --json
```

**Change how it looks, then look at it.**

```sh
unluminous-cli settings set appearance.font.size 20 --json
unluminous-cli settings set appearance.background.opacity 0.6 --json
unluminous-cli window screenshot _agent_output/unluminous.png --json
```

**Mark the passages a piece of work is about.** A highlight is a colour behind a range of text. It
stays until it is cleared, it comes back next time the project is opened, and it moves with the text
as the file is edited. The file does not have to be open.

```sh
unluminous-cli highlight add src/main.rs --from-line 40 --to-line 58 --color blue --json
unluminous-cli highlight add src/main.rs --text "unwrap()" --color pink --json
unluminous-cli highlight list --all --json
unluminous-cli highlight clear src/main.rs --json
```

**Mark many passages across many files in one call.** This is the shape to use when the places are
already worked out — one request rather than twenty, and none of the files is opened.

```sh
cat > marks.json <<'JSON'
[
  { "path": "src/main.rs",     "fromLine": 40, "toLine": 58, "color": "blue" },
  { "path": "src/parser.rs",   "fromLine": 12, "toLine": 19, "color": "yellow" },
  { "path": "docs/design.md",  "fromLine": 3,  "toLine": 3,  "color": "green" }
]
JSON
unluminous-cli highlight apply --from-file marks.json --replace --json
unluminous-cli window screenshot _agent_output/marked.png --json
```

`--replace` clears every mark in the project first, so what is applied is all there is — which is
what to use when the marks are the current state of a piece of work rather than something to add to.
A row that cannot be applied is reported against its number and the rest of the list still goes in.

**Find where a name is defined, and everywhere it is used.** Unluminous reads a project's definitions
from the same tokeniser that colours it, so the answer is instant and it is honest about what it
does not know: where two files define the same name, both are listed and neither is guessed at.
A file whose language has not said what a definition looks like has none — `.txt`, Markdown and
CSS among them — and the commands say so rather than answering emptily.

```sh
unluminous-cli editor caret --line 42 --column 9 --json
unluminous-cli editor definition --json
unluminous-cli editor definition --open --json
unluminous-cli editor references --json
unluminous-cli editor navigate-back --json
```

Each reference says whether it is code or a word inside a comment or a string. `--code-only` leaves
the textual ones out, which is usually what a script wants.

**Rename a name everywhere it is used.** Print the change set first; `--apply` makes it. An open tab
is edited as a document — one undo step, and it is left with unsaved changes rather than being
written behind your back — and a closed file is checked to still hold the old name before it is
rewritten. A file that changed since the search is skipped whole and reported by name.

```sh
unluminous-cli editor caret --line 42 --column 9 --json
unluminous-cli editor rename open_the_result --json
unluminous-cli editor rename open_the_result --apply --json
unluminous-cli tab save --json
```

The default scope follows what the name resolves to: a variable, a parameter or a name with no known
definition changes in **this file**, and a function, type, constant or module changes across the
**project**. `--scope file` and `--scope project` say so outright, and `--include comments,strings`
adds the textual matches, which are left alone by default.

**Finish a name that is half typed.** The editor offers the names the word under the caret could
become — this file's own words and definitions, the other open tabs' definitions, the project's, and
the language's own keywords — ranked best first. `--choose` applies one of them to the word being
typed, which is exactly what pressing `Enter` on that row in the popup does.

```sh
unluminous-cli editor caret --line 42 --column 9 --json
unluminous-cli editor complete --json
unluminous-cli editor complete --choose open_the_match --json
```

Matching is a subsequence and it ignores case, so `lyt` finds `layout` and `pt` finds `paint_text`.
The row equal to what has already been typed is never offered, because taking it would change
nothing. A file no plugin claims — Markdown, plain text, a picture — has no completions and says so.

**Do something with no command of its own.** Every menu entry has a name:

```sh
unluminous-cli action list --json
unluminous-cli action run toggle-line-numbers --json
unluminous-cli action run highlight-yellow --json
unluminous-cli action run clear-highlight --json
unluminous-cli action run go-to-definition --json
unluminous-cli action run find-references --json
```

## Two things the CLI will not do

**It will not open a file chooser.** `open-file`, `open-folder` and `save-as` are entries on the File
menu that ask the platform for a window somebody has to click in, which from a script is a window
nobody is looking at. `action run` refuses all three and names the command that takes the path
instead: `tab open`, `project open`, `tab save-as`.

**It will not drive the commit panel's message and file list.** `git action commit` opens the panel;
committing is done in it, or with git in a terminal.

## Adding a command

Unluminous's rule is that a new feature comes with a command and with documentation. Both are enforced
rather than remembered:

- A command is a row in `unluminous-cli/src/catalogue.rs` and an arm in
  `crates/unluminous-app/src/app/cli.rs`. The client parses against the catalogue, so a command in it is
  a command the CLI accepts.
- A **menu entry** needs nothing at all: `action list` walks the real menus, and a test in
  `crates/unluminous-app/src/app/action_names.rs` fails if a menu entry has no name.
- The reference below must then be regenerated:
  `cargo run -p unluminous-cli --example reference`. A test in `unluminous-cli/src/documentation.rs` fails
  while a command has no section here, while a section's usage line is out of date, or while a
  section describes a command that no longer exists.

---

The rest of this document is generated from the catalogue. Do not edit it by hand; run
`cargo run -p unluminous-cli --example reference` instead.

<!-- begin generated reference -->

## Commands with no area

Six commands are typed on their own, because they are about the CLI or about a whole Unluminous rather than about one part of a window.

### status

```
unluminous-cli status [--section <name>]
```

Everything about the window in one answer: its version and build date, the project, the tabs, the panes, the terminal, who holds the keyboard, the modal that is open, the settings and git. Ask for one part with --section and the answer is only that part. `--section keyboard` is the one to read when typing goes somewhere unexpected: it says which surface Unluminous gave the keys to, and whether a text box anywhere has egui's own focus - while one has, every other surface stands aside.

- `--section <name>` — One part of the answer: editor, tabs, panes, panels, explorer, terminal, keyboard, modal, settings, git, window, project or message. Several, comma-separated, for more than one. The whole answer when it is left out.

```sh
unluminous-cli status --json
unluminous-cli status --section panes --json
```

### instances

```
unluminous-cli instances
```

The Unluminous windows that are running, with the port and the project of each. Answered without talking to any of them.

```sh
unluminous-cli instances --json
```

Answered by the CLI itself; no Unluminous needs to be running.

### launch

```
unluminous-cli launch [folder] [--timeout <milliseconds>] [--no-wait]
```

Start another Unluminous on a folder and wait until it answers.

- `folder` (optional) — The project to open. The current folder when it is left out.

- `--timeout <milliseconds>` — How long to wait for the new window to answer. 20000 by default.
- `--no-wait` — Return as soon as the process starts, without waiting for it to answer.

```sh
unluminous-cli launch C:\jason\dev\unluminous
unluminous-cli launch . --timeout 40000
```

Answered by the CLI itself; no Unluminous needs to be running.

### quit

```
unluminous-cli quit
```

Close the window. Its settings and what it had open are written down first, as they are when it is closed by hand.

```sh
unluminous-cli quit
```

### commands

```
unluminous-cli commands [name]
```

Every command this CLI has, as data: the areas, the arguments, the flags and the examples. This is what to read first when a program or an agent is driving Unluminous.

- `name` (optional) — One command, such as `terminal send`, instead of all of them.

```sh
unluminous-cli commands --json
unluminous-cli commands "modal open" --json
```

Answered by the CLI itself; no Unluminous needs to be running.

### version

```
unluminous-cli version
```

What version this command line tool is. The version and build date of the Unluminous editor it is talking to are in `status`, and `modal open about` shows them in the window.

```sh
unluminous-cli version
```

Answered by the CLI itself; no Unluminous needs to be running.

## window — the window itself

`window screenshot` is how to see what a command did. The picture is of the real window, so it is evidence rather than a description.

### window screenshot

```
unluminous-cli window screenshot <file> [--timeout <milliseconds>]
```

Write what the window is showing to a PNG file. The picture is of the real window, so it is how what a command did can be looked at.

- `file` — Where to write the PNG. A folder that is not there is made.

- `--timeout <milliseconds>` — How long to wait for the picture. 5000 by default.

```sh
unluminous-cli window screenshot _agent_output/after.png
```

### window focus

```
unluminous-cli window focus
```

Bring the window to the front and give it the keyboard.

```sh
unluminous-cli window focus
```

### window size

```
unluminous-cli window size [--width <points>] [--height <points>]
```

Read how large the window is, or set it. A fixed size is what makes two screenshots comparable.

- `--width <points>` — How wide to make it.
- `--height <points>` — How tall to make it.

```sh
unluminous-cli window size
unluminous-cli window size --width 1100 --height 720
```

### window position

```
unluminous-cli window position [--x <points>] [--y <points>]
```

Read where the window is on the screen, or move it.

- `--x <points>` — How far from the left of the screen.
- `--y <points>` — How far from the top of the screen.

```sh
unluminous-cli window position --x 40 --y 40
```

### window message

```
unluminous-cli window message [text]
```

Read the line the status bar is showing, or put a line of your own there.

- `text` (optional) — What to show. The line is cleared when this is left out. Everything after it on the line belongs to it.

```sh
unluminous-cli window message
unluminous-cli window message Ready for the next step
```

## browser — rendered web pages in Unluminous tabs

Rendered tabs use the operating system browser engine. Local HTML is served from a constrained project origin, so its CSS, scripts and images work without exposing the filesystem to page JavaScript. A window renders one page at a time: every rendered tab keeps its own address, title and history, and the one view follows whichever tab is showing, so these commands act on the tab that is showing and say so when it is not a rendered one.

### browser open

```
unluminous-cli browser open <address>
```

Render an HTTP address or local HTML file in a new Unluminous tab.

- `address` — An HTTP or HTTPS address, or an HTML path relative to the project folder. Everything after it on the line belongs to it.

```sh
unluminous-cli browser open https://example.com
unluminous-cli browser open examples/site/index.html
```

### browser status

```
unluminous-cli browser status
```

Read the address, title, loading state, whether this is the tab the one view is pointed at, and the history directions of the rendered tab that is showing.

```sh
unluminous-cli browser status --json
```

### browser back

```
unluminous-cli browser back
```

Go back through the rendered tab's own history, which is kept apart from every other tab's.

```sh
unluminous-cli browser back
```

### browser forward

```
unluminous-cli browser forward
```

Go forward through the rendered tab's own history.

```sh
unluminous-cli browser forward
```

### browser reload

```
unluminous-cli browser reload
```

Reload the rendered tab that is showing, including its linked local resources.

```sh
unluminous-cli browser reload
```

## tab — the files that are open

A tab holds a file. A relative path is resolved against the project folder, and every reply says which absolute path it used.

### tab open

```
unluminous-cli tab open <path> [--permanent]
```

Open a file in a tab and show it. A picture opens as a picture; anything else opens as text.

- `path` — The file. A relative path is resolved against the project folder.

- `--permanent` — Open it as a tab of its own rather than reusing the tab a single click reuses.

```sh
unluminous-cli tab open README.md
unluminous-cli tab open design/style-guide.md --permanent
```

### tab list

```
unluminous-cli tab list
```

The tabs that are open, in order, with the path, the name and whether each has unsaved changes.

```sh
unluminous-cli tab list --json
```

### tab show

```
unluminous-cli tab show <tab>
```

Show a tab that is already open.

- `tab` — Its number counting from 0, or its name, or its path.

```sh
unluminous-cli tab show 2
unluminous-cli tab show README.md
```

### tab close

```
unluminous-cli tab close [tab] [--discard]
```

Close a tab. A tab with unsaved changes is written first, which is what closing one by hand does. Closing the last one leaves an empty untitled tab rather than no tab at all.

- `tab` (optional) — Its number, name or path. The tab that is showing when it is left out.

- `--discard` — Close it without writing what was typed into it.

```sh
unluminous-cli tab close
unluminous-cli tab close notes.md
unluminous-cli tab close --discard
```

### tab reopen

```
unluminous-cli tab reopen
```

Open the last tab that was closed again, in the pane it was closed from. The ten most recent are remembered while this window is open, so running it again reaches the one before that. The file is read from the disk.

```sh
unluminous-cli tab reopen
```

### tab next

```
unluminous-cli tab next
```

Show the next tab, wrapping round at the end.

```sh
unluminous-cli tab next
```

### tab previous

```
unluminous-cli tab previous
```

Show the previous tab.

```sh
unluminous-cli tab previous
```

### tab move

```
unluminous-cli tab move <position> [--tab <tab>] [--pane <number>]
```

Move a tab along its strip, or into another pane, which is what dragging it does. The position counts the tabs of the pane it is going to, as they are on the screen now.

- `position` — Where it goes, counting from 0. Past the end means the end.

- `--tab <tab>` — Which tab to move: its number, name or path. The tab that is showing when it is left out.
- `--pane <number>` — Which pane to move it into, counting from 0. The pane it is already in when it is left out.

```sh
unluminous-cli tab move 0
unluminous-cli tab move 0 --tab notes.md --pane 1
```

### tab save

```
unluminous-cli tab save
```

Write the tab that is showing back to its file.

```sh
unluminous-cli tab save
```

### tab save-as

```
unluminous-cli tab save-as <path>
```

Write the tab that is showing to another file, and go on editing that one.

- `path` — Where to write it.

```sh
unluminous-cli tab save-as notes/copy.md
```

### tab reload

```
unluminous-cli tab reload [--discard]
```

Read the file from disk again. A tab with unsaved changes is refused unless you say to throw them away, because there is no undo for that.

- `--discard` — Reload even though the tab has unsaved changes, losing them.

```sh
unluminous-cli tab reload
unluminous-cli tab reload --discard
```

## pane — the editing area split into panes

The editing area can be split into panes side by side, each with its own tabs, which is the reference editor's split view. `pane split` moves the tab that is showing into a new pane on the right — it moves rather than copies, because two tabs on one file would be two documents over one path. A pane holding only that tab keeps it and the new pane opens empty, ready for the next file: opening a file always lands in the pane that has the keyboard.

### pane list

```
unluminous-cli pane list
```

The panes the editing area is split into, with the tabs in each, which tab is showing in each, and which pane has the keyboard.

```sh
unluminous-cli pane list --json
```

### pane split

```
unluminous-cli pane split
```

Put a pane to the right of the one with the keyboard and move the tab that is showing into it. A pane holding only that tab keeps it and the new pane opens empty.

```sh
unluminous-cli pane split
```

### pane move

```
unluminous-cli pane move <left|right>
```

Move the tab that is showing into the pane beside it.

- `direction` — left or right.

```sh
unluminous-cli pane move right
unluminous-cli pane move left
```

### pane focus

```
unluminous-cli pane focus <pane>
```

Put the keyboard in a pane, so that the next file opened lands in it.

- `pane` — Its number counting from 0, left to right.

```sh
unluminous-cli pane focus 1
```

### pane width

```
unluminous-cli pane width <pane> <fraction>
```

Set one pane's share of the editing area, which is what dragging the divider between two panes does. The other panes share what is left.

- `pane` — Its number counting from 0.
- `fraction` — Its share of the width, between 0.05 and 0.95.

```sh
unluminous-cli pane width 0 0.35
```

### pane unsplit

```
unluminous-cli pane unsplit
```

Fold the pane that has the keyboard into the one beside it, keeping its tabs.

```sh
unluminous-cli pane unsplit
```

### pane unsplit-all

```
unluminous-cli pane unsplit-all
```

Put every tab back into one pane.

```sh
unluminous-cli pane unsplit-all
```

## editor — safely rename symbols everywhere, find every use and definitions

Use this tool first for project-symbol work. If asked to find every place a name is used, call `references` with `name`; if asked where a name is defined, call `definition`; if asked to rename it everywhere, call `rename` with `name`, `new-name` and `apply: true`. Do not begin those jobs with grep, file search, reads or file edits. Unluminous's native answers combine unsaved live open tabs with the project index, distinguish code from comments and strings, and apply a role-aware project rename as one undo step per open file while safely rewriting closed files. Lines and columns count from 1.

### editor status

```
unluminous-cli editor status
```

What the tab that is showing holds: its path, how many lines, where the caret is, what is selected, whether it has unsaved changes and which view mode it is in.

```sh
unluminous-cli editor status --json
```

### editor text

```
unluminous-cli editor text [--from-line <number>] [--to-line <number>]
```

Read the text of the tab that is showing.

- `--from-line <number>` — The first line to read, counting from 1.
- `--to-line <number>` — The last line to read, counting from 1.

```sh
unluminous-cli editor text
unluminous-cli editor text --from-line 1 --to-line 20
```

### editor set-text

```
unluminous-cli editor set-text [text] [--from-file <path>]
```

Replace everything in the tab that is showing. One undo puts it back.

- `text` (optional) — The new text. Use --from-file instead for anything long. Everything after it on the line belongs to it.

- `--from-file <path>` — Read the new text from this file rather than from the command line.

```sh
unluminous-cli editor set-text # Notes
unluminous-cli editor set-text --from-file draft.md
```

### editor insert

```
unluminous-cli editor insert <text>
```

Type text at the caret, replacing the selection if there is one. It types exactly what it is given: there is no auto-indent, so a line of a function body has to carry its own leading spaces.

- `text` — What to type, exactly as it will appear. \n is a new line and \t is a tab. Nothing is indented for you. Everything after it on the line belongs to it.

```sh
unluminous-cli editor insert Hello
unluminous-cli editor insert "one\ntwo"
```

### editor caret

```
unluminous-cli editor caret [--line <number>] [--column <number>]
```

Read where the caret is, or move it. Lines and columns count from 1, which is what the status bar shows.

- `--line <number>` — The line to move to.
- `--column <number>` — The column to move to. The start of the line when it is left out.

```sh
unluminous-cli editor caret
unluminous-cli editor caret --line 42 --column 5
```

### editor select

```
unluminous-cli editor select [--all] [--none] [--from-line <number>] [--from-column <number>] [--to-line <number>] [--to-column <number>]
```

Select some of the text, all of it, or none of it.

- `--all` — Select the whole document.
- `--none` — Drop the selection, leaving the caret where it was.
- `--from-line <number>` — The line the selection starts on.
- `--from-column <number>` — The column it starts at. 1 when it is left out.
- `--to-line <number>` — The line it ends on.
- `--to-column <number>` — The column it ends at. The end of the line when it is left out.

```sh
unluminous-cli editor select --all
unluminous-cli editor select --from-line 3 --to-line 6
```

### editor indent

```
unluminous-cli editor indent [--space]
```

Indent each line the selection touches, or the line the caret is on when nothing is selected, by one tab at the start of the line — or one space with --space. This is what Tab and Space do over a selection in the editing area, and the selection stays over the text it covered.

- `--space` — Indent with a space rather than a tab, which is what the Space key does.

```sh
unluminous-cli editor indent
unluminous-cli editor indent --space
```

### editor dedent

```
unluminous-cli editor dedent [--space]
```

Remove one indent from each line the selection touches, or the caret's line when nothing is selected — one tab, or one space with --space. This is what Shift+Tab and Shift+Space do over a selection. A line with none, or indented with the other unit, is left alone.

- `--space` — Remove a space rather than a tab, which is what Shift+Space does.

```sh
unluminous-cli editor dedent
unluminous-cli editor dedent --space
```

### editor comment

```
unluminous-cli editor comment [--toggle] [--block]
```

Comment or uncomment the lines the selection touches, or the caret's line when nothing is selected, with the marker this file's language uses. It is the same command both ways. The marker comes from the plugin that claims the file, so a language that names none is refused rather than guessed at.

- `--toggle` — Use the line comment marker, such as // or #. This is what it does when neither flag is given.
- `--block` — Use the block comment markers instead, such as /* and */, wrapping the whole selection once rather than each line.

```sh
unluminous-cli editor comment --toggle
unluminous-cli editor comment --block
```

### editor lines

```
unluminous-cli editor lines <duplicate|move|join|sort> [--by <number>]
```

Edit whole lines: duplicate, move up or down, join, or sort. Each is one undo step, and each is about the lines the selection touches, or the caret's line when nothing is selected. Sort needs a selection.

- `what` — duplicate, move, join or sort.

- `--by <number>` — For move: how far and which way. -1 is up and 1 is down, which is what Alt+Up and Alt+Down send.

```sh
unluminous-cli editor lines duplicate
unluminous-cli editor lines move --by -1
unluminous-cli editor lines join
unluminous-cli editor lines sort
```

### editor bracket

```
unluminous-cli editor bracket [--offset <bytes>] [--line <number>] [--column <number>] [--go]
```

The bracket answering the one beside the caret, as both byte offsets and both line and column positions. It tells a bracket in a comment or a string from one in the code, which a search for the character cannot. --go moves the caret to it.

- `--offset <bytes>` — Ask about this position in the file rather than about the caret.
- `--line <number>` — Ask about this line, counting from 1.
- `--column <number>` — The column on that line. 1 when it is left out.
- `--go` — Move the caret to the bracket that answers it.

```sh
unluminous-cli editor bracket --json
unluminous-cli editor bracket --go
```

### editor trim

```
unluminous-cli editor trim
```

Take the trailing whitespace off every line of the tab that is showing, as one undo step. The editor.trim setting does this on every save; this asks for it once. Refused for a Markdown file, where two trailing spaces are a line break.

```sh
unluminous-cli editor trim
```

### editor undo

```
unluminous-cli editor undo
```

Undo the last edit in the tab that is showing.

```sh
unluminous-cli editor undo
```

### editor redo

```
unluminous-cli editor redo
```

Redo the edit that was last undone.

```sh
unluminous-cli editor redo
```

### editor view

```
unluminous-cli editor view <raw|side|preview>
```

Choose how a file with a preview is shown: the source, the source and the preview side by side, or the preview. Markdown and Mermaid files have one; nothing else does, and only a file with a preview can be shown any way but raw.

- `mode` — raw, side or preview.

```sh
unluminous-cli editor view preview
unluminous-cli editor view side
```

### editor scroll

```
unluminous-cli editor scroll [--line <number>] [--to <points>] [--top] [--bottom] [--preview]
```

Read how far the tab that is showing is scrolled, or scroll it. With no flags it reports both halves of the side by side view. In side by side the other half follows, exactly as it does when you scroll with the wheel.

- `--line <number>` — Scroll so this line is at the top, counting from 1.
- `--to <points>` — Scroll to this many points down the page.
- `--top` — Scroll to the top.
- `--bottom` — Scroll to the bottom.
- `--preview` — Scroll the Markdown preview rather than the source.

```sh
unluminous-cli editor scroll --json
unluminous-cli editor scroll --line 120
unluminous-cli editor scroll --preview --top
```

### editor preview

```
unluminous-cli editor preview
```

Read the preview of the tab that is showing: a Markdown page as plain text with where its pictures, diagrams and links are, or, for a Mermaid file, what the diagram came out as. Under `--json` each link reports the bytes of its words and the address it goes to, with a reference definition already resolved, so a document's links can be read without parsing its source.

```sh
unluminous-cli editor preview --json
```

### editor preview-select

```
unluminous-cli editor preview-select [--from <bytes>] [--to <bytes>] [--all] [--none] [--copy]
```

What is selected in the Markdown preview, and selecting something in it. The preview is read only, so a selection there is for reading and copying rather than editing; the offsets are into the preview's own text, which is what `editor preview` prints.

- `--from <bytes>` — Where the selection starts in the preview's text.
- `--to <bytes>` — Where it ends. The end of the text when it is left out.
- `--all` — Select the whole preview.
- `--none` — Select nothing.
- `--copy` — Put whatever is selected on the clipboard.

```sh
unluminous-cli editor preview-select --json
unluminous-cli editor preview-select --all --copy
unluminous-cli editor preview-select --from 0 --to 40
```

### editor definition

```
unluminous-cli editor definition [name] [--offset <bytes>] [--line <number>] [--column <number>] [--open]
```

Where a name is defined, from Unluminous's live open tabs and project symbol index. Give the name directly or leave it out for the word at the caret; every candidate is printed best first and --open navigates through the editor.

- `name` (optional) — The name to find. The word at the caret when it is left out.

- `--offset <bytes>` — Ask about this position in the file rather than about the caret.
- `--line <number>` — Ask about this line, counting from 1.
- `--column <number>` — The column on that line. 1 when it is left out.
- `--open` — Go to the best candidate, opening its file as a tab.

```sh
unluminous-cli editor definition --json
unluminous-cli editor definition Rect --open --json
unluminous-cli editor definition --line 42 --column 9 --open
```

### editor find

```
unluminous-cli editor find [text] [--match-case] [--whole-word] [--next] [--previous] [--close] [--limit <number>]
```

Find text in the file that is showing, the way Ctrl+F does: the count, every match's line and column, and the one that is current selected in the window so a screenshot shows it. Use this rather than reading the whole file to locate a string. --replace turns it into an edit; without it nothing is changed.

- `text` (optional) — What to look for. The bar's current search when it is left out.

- `--match-case` — Only matches spelt with the same capitals count.
- `--whole-word` — Only matches with no letter, digit or underscore either side of them count.
- `--next` — Move to the match after the one that is current, wrapping round the end of the file.
- `--previous` — Move to the match before it, wrapping round the start.
- `--close` — Put the Find bar away, leaving the caret on the match it was on.
- `--limit <number>` — List at most this many matches. 50 when it is left out, and 0 means all of them.

```sh
unluminous-cli editor find relayout --json
unluminous-cli editor find Rect --whole-word --match-case --json
unluminous-cli editor find --next --json
unluminous-cli editor find --close
```

### editor replace

```
unluminous-cli editor replace <text> <with> [--match-case] [--whole-word] [--all] [--apply]
```

Replace text in the file that is showing. Use this for a string, a comment, a URL or a number - `editor rename` is for a symbol and is the better tool when it applies, because it knows code from comments. Without --apply it says how many matches there are and changes nothing. Applying is one undo step whether it changes one match or four hundred.

- `text` — What to look for.
- `with` — What to put in its place. Give an empty string to delete it.

- `--match-case` — Only matches spelt with the same capitals count.
- `--whole-word` — Only matches with no letter, digit or underscore either side of them count.
- `--all` — Replace every match rather than only the one that is current.
- `--apply` — Make the change. Without it the count is printed and nothing is edited.

```sh
unluminous-cli editor replace teh the --all --json
unluminous-cli editor replace teh the --all --apply
unluminous-cli editor replace 0.34.2 0.35.0 --match-case --all --apply
```

### editor references

```
unluminous-cli editor references [name] [--timeout <milliseconds>] [--code-only]
```

Use this instead of grep to find every place a name is used across the project: the file, line, column and whether it is code or a word inside a comment or string. Reads unsaved open tabs as they stand and everything else from the disk.

- `name` (optional) — The name to look for. The word at the caret when it is left out.

- `--timeout <milliseconds>` — How long to wait for the search. 10000 by default.
- `--code-only` — Leave out the matches inside comments and strings.

```sh
unluminous-cli editor references --json
unluminous-cli editor references open_the_match --json
```

### editor rename

```
unluminous-cli editor rename <new-name> [--name <text>] [--scope <file|project>] [--include <comments,strings>] [--timeout <milliseconds>] [--apply]
```

Use this instead of file edits to rename a symbol everywhere through Unluminous's role-aware references. Comments and strings stay untouched unless included; without --apply it previews. Applying edits each open tab as one undo step, leaving it with unsaved changes, and writes every file that is not open straight to the disk — the answer counts the two apart, as `wrote` and `openTabs`.

- `new-name` — What to call it. It has to be a word of this language and not one of its keywords.

- `--name <text>` — Rename this name rather than the word at the caret.
- `--scope <file|project>` — Which files to change. The default follows what the name resolves to: a variable or a name with no known definition is this file, and a function, type, constant or module is the project.
- `--include <comments,strings>` — Also change the matches inside comments or strings, which are left alone by default.
- `--timeout <milliseconds>` — How long to wait for the search that finds them. 10000 by default.
- `--apply` — Make the change. Without it the change set is printed and nothing is edited or written.

```sh
unluminous-cli editor rename open_the_result --json
unluminous-cli editor rename open_the_result --apply
unluminous-cli editor rename total --scope project --include comments --apply
```

### editor complete

```
unluminous-cli editor complete [--offset <bytes>] [--line <number>] [--column <number>] [--stem <text>] [--limit <number>] [--choose <name>]
```

The names a word could become, best first, with what each row is and where it came from. By default the word is read from the document at the caret; --stem asks hypothetically without editing the document. Inside an import the rows are what can be imported instead. --choose applies a real document row exactly as Enter would, and takes the row's name rather than its position.

- `--offset <bytes>` — Ask about this position in the file rather than about the caret.
- `--line <number>` — Ask about this line, counting from 1.
- `--column <number>` — The column on that line. 1 when it is left out.
- `--stem <text>` — Ask what this hypothetical word would offer at the position, without inserting it or changing the document.
- `--limit <number>` — Print at most this many rows. 50 when it is left out, and 0 means all of them.
- `--choose <name>` — Apply this row to the word being typed, as Enter would. It is the completion's **name**, never a row number: `--choose 0` is refused with the names there are.

```sh
unluminous-cli editor complete --json
unluminous-cli editor complete --stem ar --limit 5 --json
unluminous-cli editor complete --limit 5 --json
unluminous-cli editor complete --choose draw_frame
unluminous-cli editor complete --choose ./layout
```

### editor navigate-back

```
unluminous-cli editor navigate-back
```

Go back to where the caret was before the last jump, reopening the file if its tab was closed.

```sh
unluminous-cli editor navigate-back
```

### editor navigate-forward

```
unluminous-cli editor navigate-forward
```

Undo a navigate-back. Cleared by any new jump, exactly as a browser's forward button is.

```sh
unluminous-cli editor navigate-forward
```

## update — whether a newer Unluminous has been released

One request to the GitHub releases page, made only when this is run or when a person asks in the window. Unluminous sends nothing at startup unless the `update.check` setting says to, and this reports the version rather than installing anything.

### update check

```
unluminous-cli update check [--timeout <milliseconds>]
```

Whether a newer Unluminous has been released, and what this one is. One request to the GitHub releases page, made only when this is run or when a person asks in the window - Unluminous sends nothing at startup unless the update.check setting says to. It reports the version and never installs anything.

- `--timeout <milliseconds>` — How long to wait for the answer. 15000 by default.

```sh
unluminous-cli update check --json
```

## highlight — the passages marked in the project's files

A highlight is a colour behind a passage of text. It stays there until it is cleared, in this file and next time the project is opened, and it moves with the text as the file is edited. These work on a file whether it is open or not, so `highlight apply` can mark twenty passages across twenty files in one call.

### highlight list

```
unluminous-cli highlight list [path] [--all]
```

What is marked, in one file or across the whole project: where each passage is, what colour it is in, and the text under it.

- `path` (optional) — The file to list. The tab that is showing when it is left out.

- `--all` — List every file in the project rather than one.

```sh
unluminous-cli highlight list --json
unluminous-cli highlight list --all --json
```

### highlight add

```
unluminous-cli highlight add [path] [--from-line <number>] [--from-column <number>] [--to-line <number>] [--to-column <number>] [--text <words>] [--color <name>]
```

Mark a passage in a colour. Give it lines and columns, or --text to mark every occurrence of some words. The file need not be open.

- `path` (optional) — The file to mark. The tab that is showing when it is left out.

- `--from-line <number>` — The line the passage starts on, counting from 1.
- `--from-column <number>` — The column it starts at. 1 when it is left out.
- `--to-line <number>` — The line it ends on. The line it started on when it is left out.
- `--to-column <number>` — The column it ends at. The end of the line when it is left out.
- `--text <words>` — Mark every occurrence of these words in the file instead of a range.
- `--color <name>` — yellow, green, blue, pink, or a colour of your own as #rrggbb or #rrggbbaa. Yellow when it is left out.

```sh
unluminous-cli highlight add --from-line 12 --to-line 18
unluminous-cli highlight add src/main.rs --from-line 40 --to-line 44 --color blue
unluminous-cli highlight add src/main.rs --text "unwrap()" --color pink
```

### highlight clear

```
unluminous-cli highlight clear [path] [--from-line <number>] [--to-line <number>] [--all]
```

Take marks away: a range of lines, a whole file, or every file in the project.

- `path` (optional) — The file to clear. The tab that is showing when it is left out.

- `--from-line <number>` — The first line to clear, counting from 1. The whole file when it is left out.
- `--to-line <number>` — The last line to clear. The line it started on when it is left out.
- `--all` — Clear every file in the project.

```sh
unluminous-cli highlight clear
unluminous-cli highlight clear src/main.rs --from-line 40 --to-line 44
unluminous-cli highlight clear --all
```

### highlight apply

```
unluminous-cli highlight apply [--from-file <path>] [--json-text <json>] [--replace]
```

Mark many passages across many files in one go, from a JSON array of {path, fromLine, toLine, fromColumn, toColumn, color} objects.

- `--from-file <path>` — Read the JSON array from this file.
- `--json-text <json>` — The JSON array itself, for a short list. Quote it.
- `--replace` — Clear every mark in the project first, so what is applied is all there is.

```sh
unluminous-cli highlight apply --from-file marks.json
unluminous-cli highlight apply --json-text '[{"path":"src/main.rs","fromLine":1,"toLine":3}]'
```

## fold — the blocks collapsed in the tab that is showing

A block that can be collapsed is a function, an `if`, a bracket that spans lines, a run of comments, an indented section, or a Markdown heading — worked out from the file itself, so nothing has to be written into it. Collapsing one hides its lines; the line numbers of everything still showing are unchanged, so `fold list` and `editor caret --line` speak the same language whatever is folded. `fold others` is the one to notice: it collapses everything that does not hold a marked passage, which is how to leave only the four places you care about on the screen.

### fold list

```
unluminous-cli fold list
```

Every block in the tab that is showing that can be collapsed: which line it starts on, which line it ends on, how many lines it hides, what kind of block it is, and whether it is collapsed now.

```sh
unluminous-cli fold list --json
```

### fold toggle

```
unluminous-cli fold toggle [--line <number>] [--regions]
```

Collapse a block that is showing, or expand one that is collapsed. The block at the caret when no line is given. The answer is how many blocks are collapsed; --regions adds the list.

- `--line <number>` — The line the block starts on, counting from 1. `fold list` says which lines those are.
- `--regions` — Also answer with the list of every block and whether it is collapsed.

```sh
unluminous-cli fold toggle
unluminous-cli fold toggle --line 42
unluminous-cli fold toggle --regions --json
```

### fold collapse

```
unluminous-cli fold collapse [--line <number>] [--all] [--recursive] [--regions]
```

Collapse one block, every block in the file, or one block and every block inside it. The answer is how many blocks are collapsed; --regions adds the list.

- `--line <number>` — The line the block starts on, counting from 1.
- `--all` — Collapse every block in the file.
- `--recursive` — With --line, collapse that block and every block inside it rather than just that block.
- `--regions` — Also answer with the list of every block and whether it is collapsed.

```sh
unluminous-cli fold collapse --all
unluminous-cli fold collapse --line 42
unluminous-cli fold collapse --line 42 --recursive
unluminous-cli fold collapse --all --regions --json
```

### fold expand

```
unluminous-cli fold expand [--line <number>] [--all] [--recursive] [--regions]
```

Expand one block, show all again, or expand one block and every block inside it. The answer is how many blocks are collapsed; --regions adds the list.

- `--line <number>` — The line the block starts on, counting from 1.
- `--all` — Expand every block in the file.
- `--recursive` — With --line, expand that block and every block inside it, opening the whole of it rather than one level.
- `--regions` — Also answer with the list of every block and whether it is collapsed.

```sh
unluminous-cli fold expand --all
unluminous-cli fold expand --line 42
unluminous-cli fold expand --line 42 --recursive
unluminous-cli fold expand --all --regions --json
```

### fold others

```
unluminous-cli fold others [--selection] [--regions]
```

Collapse everything that does not hold a marked passage, so only the marked parts of the file are left showing. Falls back to the selection when nothing is marked. The answer is how many blocks are collapsed; --regions adds the list.

- `--selection` — Keep what is selected rather than what is marked, even when there are marks.
- `--regions` — Also answer with the list of every block and whether it is collapsed.

```sh
unluminous-cli fold others
unluminous-cli fold others --selection
unluminous-cli fold others --regions --json
```

## panel — which edge of the window each panel is docked to

Unluminous has four panels — the explorer, the terminal, the run tile and the debug tile — and each of them can be docked to any edge of the window, which is what dragging its header does. A side holds an ordered row of panels laid out left to right, so `panel dock terminal left --position 1` puts the terminal beside the explorer rather than in place of it. The terminal, run and debug tiles all draw a character grid and two grids in one strip would be two half-sized grids, so showing one puts away the other tiles **on its own side** — move one somewhere else and they are both showing at once. `panel list` says where everything is, including the rectangle each occupies, which is what to read before working out where a click lands.

### panel list

```
unluminous-cli panel list
```

Every panel Unluminous has — the explorer, the terminal, the run tile and the debug tile — which edge of the window each is docked to, where in that edge, how big it is, whether it is showing, and the rectangle it occupies on screen. A pane a plugin contributes is listed by `plugins list` and can be sized and docked by name here.

```sh
unluminous-cli panel list --json
```

### panel dock

```
unluminous-cli panel dock <panel> <left|right|top|bottom> [--position <number>]
```

Move a panel to an edge of the window: the same change dragging its header makes. A side can hold more than one panel, side by side, so the terminal can sit beside the explorer down the left.

- `panel` — explorer, terminal, run, debug, or a contributed pane's <plugin>/<pane>.
- `side` — left, right, top or bottom.

- `--position <number>` — Where in that side, counting the panels already there from the left, starting at 0. The end of the side when it is not given.

```sh
unluminous-cli panel dock terminal right
unluminous-cli panel dock terminal left --position 0
unluminous-cli panel dock agent-chat/chat right
```

### panel size

```
unluminous-cli panel size <panel> [--width <points>] [--height <points>]
```

Set how wide or how tall a panel is, including a pane a plugin contributed. A panel at the left or the right is read by its width and one along the top or the bottom by its height, so both are kept and moving a panel does not lose the size it had on the other side. The same numbers are `settings set panes.<panel>.width` and `.height`.

- `panel` — explorer, terminal, run, debug, or a contributed pane's <plugin>/<pane>.

- `--width <points>` — How wide it is when it is a column at the left or the right.
- `--height <points>` — How tall it is when it is in a strip along the top or the bottom.

```sh
unluminous-cli panel size debug --width 640
unluminous-cli panel size terminal --height 320
unluminous-cli panel size agent-chat/chat --width 1150
```

### panel zoom

```
unluminous-cli panel zoom <panel> [factor]
```

Make everything in a panel bigger or smaller, which is what Ctrl/Cmd and the wheel over it does. The explorer and a pane a plugin contributed carry a multiplier of their own; the terminal, run and debug tiles are character grids and their zoom is the terminal's font size, so a zoom there walks `settings set terminal.font.size` and both say the same number.

- `panel` — explorer, terminal, run, debug, or a contributed pane's <plugin>/<pane>.
- `factor` (optional) — How much bigger than usual, between 0.5 and 3. Left out, it says what the panel is at now; `reset` puts it back to 1.

```sh
unluminous-cli panel zoom explorer 1.35
unluminous-cli panel zoom agent-chat/chat
unluminous-cli panel zoom explorer reset
```

### panel reset

```
unluminous-cli panel reset
```

Put every panel back where a new Unluminous has it: the explorer down the left, the three tiles along the bottom, each at its starting size.

```sh
unluminous-cli panel reset
```

## space — the Base of Infinite Space: a canvas of terminals, web pages, folder trees and file editors, wired together

If `UNLUMINOUS_SPACE_NODE` is set in your environment you are running inside a node on this canvas, and `space here` is the first thing to run: it says which node you are, which nodes you are wired to, and the command that drives each of them. **`unluminous-cli` is not on your PATH** - it lives inside the application - so run it as `"$UNLUMINOUS_CLI" --instance $UNLUMINOUS_INSTANCE <command>`, which are both set in your environment. You may act on the nodes you are wired to and no others, so every command you send carries `--from <your node>`. The Base of Infinite Space is a canvas you put nodes on: a terminal running a real shell, a web page, a folder tree, or a file editor with the editing area's own gutter, folding and find. A node is wired to another by connecting its output to that node's input, and a connection is what lets an agent running in a terminal node act on the node it is wired to - `space browser`, `space folder`, `space editor` and `space send` all take `--from` and are refused when there is no wire. The window's own agent passes no `--from` and may drive every node. Read `space view --json` first: everything here names a node by the id it prints. Places and sizes are in canvas points, which are screen points at a zoom of 1.

### space show

```
unluminous-cli space show
```

Show the Base of Infinite Space, the canvas of nodes, and give it the keyboard.

```sh
unluminous-cli space show
```

### space hide

```
unluminous-cli space hide
```

Put the canvas away. Everything running on it keeps running.

```sh
unluminous-cli space hide
```

### space here

```
unluminous-cli space here [--node <node>]
```

Which node this command is running inside, what that node is wired to, and the command that drives each of them. Run this **first** when `UNLUMINOUS_SPACE_NODE` is set in your environment: it is the one command that answers where you are, because the answer depends on which process is asking. Outside a node it says so and says what that means — every node is reachable and no `--from` is needed, which is the window's own agent.

- `--node <node>` — Answer about this node rather than the one this process is in. Filled in from UNLUMINOUS_SPACE_NODE when it is not given, which is the ordinary case and needs nothing.

```sh
unluminous-cli space here
unluminous-cli space here --json
```

### space view

```
unluminous-cli space view
```

The whole canvas as data: every view, every node with its kind, its title, where it is and how big it is, and every connection. This is what to read before acting on a node, because everything else here names one by the id this prints.

```sh
unluminous-cli space view --json
```

### space list

```
unluminous-cli space list
```

The nodes on the view that is showing, one a line: id, kind, title, place, size, and what each is connected to.

```sh
unluminous-cli space list
```

### space manage

```
unluminous-cli space manage
```

Open the modal that lists every canvas in this project, with a search box over it. What it lists is this project's views — `space views` is the same list as data. A canvas belongs to the project it was made in, because its nodes name that project's files, so the modal's last button opens another project in a window of its own rather than pretending to open its canvas here.

```sh
unluminous-cli space manage
```

### space views

```
unluminous-cli space views
```

Every view this project's canvas has, with which one is showing and how many nodes each holds.

```sh
unluminous-cli space views
```

### space open-view

```
unluminous-cli space open-view <view>
```

Show one of the canvas's views. A view is named by the name on its chip or by its id.

- `view` — The view's name, or its id.

```sh
unluminous-cli space open-view Rendering
```

### space new-view

```
unluminous-cli space new-view [name]
```

Make another view and show it. A name already in use is numbered rather than refused.

- `name` (optional) — What to call it. `View` when it is not given.

```sh
unluminous-cli space new-view Rendering
```

### space rename-view

```
unluminous-cli space rename-view <view> <name>
```

Call a view something else.

- `view` — The view's name, or its id.
- `name` — What to call it.

```sh
unluminous-cli space rename-view Main Rendering
```

### space duplicate-view

```
unluminous-cli space duplicate-view <view>
```

Copy a view, its nodes, its connections and its camera under new ids, and show the copy. The nodes are copies: a terminal on the copy is a second terminal rather than a second drawing of one.

- `view` — The view's name, or its id.

```sh
unluminous-cli space duplicate-view Main
```

### space delete-view

```
unluminous-cli space delete-view <view>
```

Throw a view away, stopping every program on it. Refused on the last one, because a canvas always has a view.

- `view` — The view's name, or its id.

```sh
unluminous-cli space delete-view Rendering
```

### space add

```
unluminous-cli space add <terminal|browser|folder|editor|chat|tasks> [--x <points>] [--y <points>] [--width <points>] [--height <points>] [--title <text>] [--command <text>] [--url <address>] [--root <path>] [--path <path>]
```

Put a node on the view that is showing and answer with its id. A terminal node starts the machine's own shell in the project folder, or the program named by `--command`, with UNLUMINOUS_SPACE_NODE set to its id so an agent started in it knows which node it is, UNLUMINOUS_SPACE_HINT saying what to run first, and UNLUMINOUS_CLI and UNLUMINOUS_INSTANCE saying where `unluminous-cli` is and which window it drives - it is on nobody's PATH.

- `kind` — terminal, browser, folder, editor, chat or tasks.

- `--x <points>` — Where to put it, in canvas points. The middle of what is showing when it is not given.
- `--y <points>` — The same, down the canvas.
- `--width <points>` — How wide, in canvas points.
- `--height <points>` — How tall.
- `--title <text>` — What the header says. Empty means call it after what it holds.
- `--command <text>` — A terminal node: the program to run instead of the shell.
- `--url <address>` — A browser node: the page to open.
- `--root <path>` — A folder node: which folder to show. The project when it is not given.
- `--path <path>` — An editor node: which file to open.

```sh
unluminous-cli space add terminal --command claude
unluminous-cli space add browser --url https://example.com/
unluminous-cli space add editor --path src/main.rs --x 900 --y 40
```

### space move

```
unluminous-cli space move <node> [--x <points>] [--y <points>]
```

Move a node to a place on the canvas, in canvas points.

- `node` — The node's id, from `space list`.

- `--x <points>` — Where its left hand edge goes.
- `--y <points>` — Where its top edge goes.

```sh
unluminous-cli space move 7 --x 320 --y 180
```

### space size

```
unluminous-cli space size <node> [--width <points>] [--height <points>]
```

Resize a node. It is never made smaller than its kind allows, and the reply says what size it really came out.

- `node` — The node's id.

- `--width <points>` — How wide.
- `--height <points>` — How tall.

```sh
unluminous-cli space size 7 --width 900 --height 520
```

### space title

```
unluminous-cli space title <node> <title>
```

Call a node something else. An empty name puts it back to being called after what it holds.

- `node` — The node's id.
- `title` — What the header says. Everything after it on the line belongs to it.

```sh
unluminous-cli space title 7 the agent
```

### space remove

```
unluminous-cli space remove <node>
```

Take a node off the canvas, stopping whatever was running in it. A file editor node's tab is closed, which writes it first if it was edited.

- `node` — The node's id.

```sh
unluminous-cli space remove 7
```

### space focus

```
unluminous-cli space focus <node>
```

Choose a node, bring it to the front and give it the keyboard.

- `node` — The node's id.

```sh
unluminous-cli space focus 7
```

### space connect

```
unluminous-cli space connect <from> <to> [--pipe <lines>]
```

Wire one node's output to another's input. A connection is what lets an agent in a terminal node act on the node it is wired to; with `--pipe lines` it also types each line the first node's program writes into the second node's terminal.

- `from` — The node the wire leaves.
- `to` — The node it arrives at.

- `--pipe <lines>` — Carry text as well as permission. Only into a terminal node, because only a terminal has an input to type into.

```sh
unluminous-cli space connect 7 9
unluminous-cli space connect 7 9 --pipe lines
```

### space disconnect

```
unluminous-cli space disconnect <connection>
```

Take a connection away, by its id from `space connections`.

- `connection` — The connection's id.

```sh
unluminous-cli space disconnect 12
```

### space connections

```
unluminous-cli space connections [--from <node>]
```

Every connection on the view that is showing: its id, which node it leaves, which it arrives at, and whether it carries lines. `--from` narrows it to one node's own, which is what an agent in a terminal node asks to find out what it may act on.

- `--from <node>` — Only the connections leaving this node.

```sh
unluminous-cli space connections --from 7
```

### space camera

```
unluminous-cli space camera [--x <points>] [--y <points>] [--zoom <factor>] [--fit]
```

Pan and zoom the canvas. With `--fit` it moves so that every node is on the screen at once, which is how to find something that has been dragged out of sight.

- `--x <points>` — The canvas point drawn at the top left corner.
- `--y <points>` — The same, down the canvas.
- `--zoom <factor>` — How many screen points one canvas point is, from 0.25 to 2.5.
- `--fit` — Put every node on the screen at once.

```sh
unluminous-cli space camera --fit
unluminous-cli space camera --zoom 0.5
```

### space send

```
unluminous-cli space send <node> <text> [--from <node>]
```

Type a line into a terminal node and press Enter. `--from` says which node is asking, and a node may only send to a node it is wired to; with no `--from` it is the window's own and may reach any of them.

- `node` — The terminal node to type into.
- `text` — The line to type. Everything after it on the line belongs to it.

- `--from <node>` — Which node is asking. It must be wired to the one it names.

```sh
unluminous-cli space send 9 cargo test
unluminous-cli space send 9 cargo test --from 7
```

### space read

```
unluminous-cli space read <node> [--tail <lines>]
```

Read what a terminal node is showing: the scrollback as well as the screen, so the commands that have scrolled above the fold are in it. This is what `terminal read` is for the terminal panel, and it is the way to check what a node printed, what it came back showing after a project was reopened, and what an agent running in a node has said.

- `node` — The terminal node's id.

- `--tail <lines>` — Answer with only the last N lines.

```sh
unluminous-cli space read 7
unluminous-cli space read 7 --tail 40
```

### space chat

```
unluminous-cli space chat <node> <new|send|stop|state|messages|last|attach|providers|use|history|open|remove|tools|view> [words] [--from <node>]
```

Drive an Agent Chat node's own conversation: `new`, `send`, `stop`, `state`, `messages`, `last`, `attach`, `providers`, `use`, `history`, `open`, `remove`, `tools` and `view`, which are the same verbs `plugins run agent-chat` has and reach the same code. The difference is whose conversation: each chat node holds one of its own, where `plugins run agent-chat` drives the pane's. Like the pane's, `send` does not wait - `state` says when the answer has arrived.

- `node` — The chat node's id, from `space list`.
- `verb` — What to do: new, send, stop, state, messages, last, attach, providers, use, history, open, remove, tools or view.
- `words` (optional) — What the verb takes: the message for `send`, the conversation id for `open`, `on` or `off` for `tools`. It is the rest of the line, so a message needs no quoting. Everything after it on the line belongs to it.

- `--from <node>` — Which node is asking. It must be wired to the one it names.

```sh
unluminous-cli space chat 7 state
unluminous-cli space chat 7 send Summarise what this project does
unluminous-cli space chat 7 last --json
```

### space restart

```
unluminous-cli space restart <node> [--resume] [--running]
```

Start a terminal node's program again in the same folder. With `--resume` it starts the agent on the conversation it named, which Claude takes and Codex does not. With `--running` it types the program the node was last seen running into the shell it already has, which is what a node comes back as when somebody typed an agent into a plain terminal rather than giving the node a command.

- `node` — The terminal node's id.

- `--resume` — Start the agent on the session it named, rather than a fresh one.
- `--running` — Type what the node was last seen running into its shell, continuing an agent's most recent conversation in that folder. What is recorded is a program name rather than a command line, so its arguments are not restored.

```sh
unluminous-cli space restart 7 --resume
unluminous-cli space restart 7 --running
```

### space font

```
unluminous-cli space font <node> [--size <points>] [--bigger] [--smaller] [--reset]
```

How big a terminal node's letters are. The ticket this canvas comes from asks for a size a node keeps for itself, so a node that has been given one follows it and one that has not follows `terminal.font.size`. With no flag at all it answers the size the node is drawn at.

- `node` — The terminal node's id.

- `--size <points>` — Any size from 6 to 96, which is what `terminal.font.size` itself takes.
- `--bigger` — One step up the list the Settings window offers.
- `--smaller` — One step down it.
- `--reset` — Follow the terminal's own setting again.

```sh
unluminous-cli space font 7 --size 16
unluminous-cli space font 7 --smaller
```

### space zoom

```
unluminous-cli space zoom <node> [--factor <number>] [--bigger] [--smaller] [--reset] [--from <node>]
```

How big one node draws what it holds. Each kind walks the number that really decides its size: a terminal and a file editor a point size, a folder view, an agent chat and the tasks board a multiplier over everything they draw, and a web browser the page's own zoom. With no flag at all it answers the factor the node is drawn at. This is what the modifier wheel over a node does, and it changes nothing about the canvas's own zoom, which is `space camera`.

- `node` — The node's id.

- `--factor <number>` — How big, as a multiplier for a folder, a chat, the board or a browser, or a point size for a terminal or an editor.
- `--bigger` — One step up.
- `--smaller` — One step down.
- `--reset` — Back to the size the window's own setting gives.
- `--from <node>` — Which node is asking. It must be wired to the one it names.

```sh
unluminous-cli space zoom 7 --bigger
unluminous-cli space zoom 9 --factor 1.5
```

### space address

```
unluminous-cli space address <node> <url> [--from <node>]
```

Type an address into a browser node's own address bar and enter it, which is what pressing Enter in that field does. `space browser <node> go` is the same navigation asked for directly; this exists so the control a person uses has a way in of its own.

- `node` — The browser node's id.
- `url` — The address, or a path to a local page in this project.

- `--from <node>` — Which node is asking. It must be wired to the one it names.

```sh
unluminous-cli space address 9 https://example.com/
```

### space browser

```
unluminous-cli space browser <node> <go|back|forward|reload|url|shot> [--url <address>] [--from <node>] [--path <file>]
```

Drive a browser node: `go` to an address, `back`, `forward`, `reload`, `url` to read where it is, and `shot` to write a picture of the node to a file. A window renders one page at a time, so the node acted on is shown first. **`shot` photographs the node as Unluminous drew it and not the page inside it**: a rendered page is a native child window the operating system composites on top, and no picture taken from inside Unluminous contains one. Use it to see the node, its address bar and where it is on the canvas; use `url` to read the address, and the agent's own tools to read what a page says.

- `node` — The browser node's id.
- `command` — go, back, forward, reload, url or shot.

- `--url <address>` — Where to go, for `go`.
- `--from <node>` — Which node is asking. It must be wired to the one it names.
- `--path <file>` — Where to write the picture, for `shot`.

```sh
unluminous-cli space browser 9 go --url https://example.com/ --from 7
unluminous-cli space browser 9 url
```

### space folder

```
unluminous-cli space folder <node> <expand|collapse|select|open|root|rows> [--path <path>] [--from <node>]
```

Drive a folder node: `expand` and `collapse` a folder in it, `select` a row, `open` a file, `root` to point it at another folder, and `rows` to read what it is showing. `open` puts the file in a File Editor node this one is wired to when there is one, and in the editing area when there is not — which is what a double click in the node does. `root` is what the node's own `Choose Folder...` menu row calls, so several folder nodes can show several different folders.

- `node` — The folder node's id.
- `command` — expand, collapse, select, open, root or rows.

- `--path <path>` — Which row, or which folder for `root`.
- `--from <node>` — Which node is asking. It must be wired to the one it names.

```sh
unluminous-cli space folder 11 expand --path crates/unluminous-app --from 7
unluminous-cli space folder 11 rows
```

### space editor

```
unluminous-cli space editor <node> <path> [--from <node>]
```

Put a file in a file editor node. A file already open somewhere else is moved into the node rather than opened twice, because two tabs on one file would be two documents over one path.

- `node` — The editor node's id.
- `path` — The file, relative to the project or absolute.

- `--from <node>` — Which node is asking. It must be wired to the one it names.

```sh
unluminous-cli space editor 13 src/main.rs --from 7
```

## input — clicking, typing and dragging in the window, without it being in front

For the things there is no other command for: a drag, a right click on a row, a press in a text box. Positions are the **window's own points** with 0,0 at its top left, which is what `window screenshot` writes out, so a position measured off a picture is the position to send. Nothing here brings the window to the front or moves the person's own pointer, and each command waits for the frames it asked for, so a screenshot taken straight afterwards shows what happened. Prefer a command that names the thing to a click that finds it.

### input move

```
unluminous-cli input move <x> <y>
```

Move the pointer to a place in the window and leave it there, which is what makes something hover. Positions are the window's own points with 0,0 at its top left corner - the same points `window screenshot` writes out, so a position measured off a picture is the position to use. Nothing about this needs the window to be in front: the person's own pointer does not move and the window is never activated.

- `x` — Across the window, in points.
- `y` — Down the window, in points.

```sh
unluminous-cli input move 300 220
```

### input click

```
unluminous-cli input click <x> <y> [--right] [--middle] [--twice] [--ctrl] [--shift] [--alt] [--cmd]
```

Click at a place in the window: the pointer moves there, the button goes down and comes up again, over three frames, which is what makes it a click rather than a flicker. The window does not have to be in front and is not brought to the front. Positions are the window's own points, which is what `window screenshot` writes out.

- `x` — Across the window, in points.
- `y` — Down the window, in points.

- `--right` — The secondary button, which opens a context menu.
- `--middle` — The middle button, which closes a tab.
- `--twice` — Two clicks, which is a double click.
- `--ctrl` — Hold control.
- `--shift` — Hold shift.
- `--alt` — Hold alt.
- `--cmd` — Hold command on macOS, control on Windows - the key a menu shortcut names.

```sh
unluminous-cli input click 300 220
unluminous-cli input click 120 96 --twice
unluminous-cli input click 300 220 --right
```

### input drag

```
unluminous-cli input drag <x> <y> [--to-x <points>] [--to-y <points>] [--steps <count>]
```

Drag from one place in the window to another: the pointer arrives, the button goes down, it is moved in steps, and it is let go. The steps are frames of their own because that is what a drag is - every drag in Unluminous is settled from the difference between two frames.

- `x` — Where the drag starts, across the window.
- `y` — Where the drag starts, down the window.

- `--to-x <points>` — Where it ends, across the window.
- `--to-y <points>` — Where it ends, down the window.
- `--steps <count>` — How many positions it is moved through. 20 when it is not given.

```sh
unluminous-cli input drag 660 110 --to-x 1250 --to-y 700
```

### input key

```
unluminous-cli input key <key> [--ctrl] [--shift] [--alt] [--cmd] [--times <count>]
```

Press a key and let it go, in whatever has the keyboard. The name is the one egui uses: a letter, a digit, `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`, `Space`, `ArrowUp`, `F2` and the rest. A key produces no text - `input text` is what types.

- `key` — The key's name.

- `--ctrl` — Hold control.
- `--shift` — Hold shift.
- `--alt` — Hold alt.
- `--cmd` — Hold command on macOS, control on Windows - the key a menu shortcut names.
- `--times <count>` — Press it more than once.

```sh
unluminous-cli input key Escape
unluminous-cli input key ArrowDown --times 3
unluminous-cli input key s --cmd
```

### input text

```
unluminous-cli input text <text>
```

Type into whatever has the keyboard, one character a frame. Each character is sent as the key press and the text a real keyboard produces, because the editing area reads one and a text box reads both.

- `text` — What to type. It is the rest of the line, so it needs no quoting. Everything after it on the line belongs to it.

```sh
unluminous-cli input text hello there
```

### input wheel

```
unluminous-cli input wheel <notches> [--across <notches>] [--ctrl] [--cmd]
```

Turn the mouse wheel where the pointer is, in notches. A negative number scrolls down the page, which is what turning the wheel towards you does. `input move` is what puts the pointer over the thing to scroll.

- `notches` — How many notches, negative for down the page.

- `--across <notches>` — Sideways, for a list that scrolls that way.
- `--ctrl` — Hold control, which is what zooms.
- `--cmd` — Hold command, which is what zooms on macOS.

```sh
unluminous-cli input wheel -3
unluminous-cli input wheel 2 --ctrl
```

## terminal — the shells along the bottom

`terminal send` types into the shell and presses Enter; `terminal read --wait-for` is how to wait for what it did. Both take `--tab` to name a tab other than the one showing, and naming a tab does not show it, so a build in one tab and a dev server in another can each be spoken to without the other being disturbed.

### terminal show

```
unluminous-cli terminal show
```

Show the terminal along the bottom, opening a shell in the project folder if there is not one already.

```sh
unluminous-cli terminal show
```

### terminal hide

```
unluminous-cli terminal hide
```

Put the terminal away. The shells keep running.

```sh
unluminous-cli terminal hide
```

### terminal toggle

```
unluminous-cli terminal toggle
```

Show the terminal if it is hidden, and hide it if it is showing.

```sh
unluminous-cli terminal toggle
```

### terminal new

```
unluminous-cli terminal new
```

Start another shell in a tab of its own, and show it.

```sh
unluminous-cli terminal new
```

### terminal list

```
unluminous-cli terminal list
```

The terminal tabs: the name of each, which one is showing, and the folder each one's shell is in, which is where it would reopen.

```sh
unluminous-cli terminal list --json
```

### terminal select

```
unluminous-cli terminal select [index] [--tab <index>]
```

Show one of the terminal tabs. The only verb that changes which tab is showing.

- `index` (optional) — Its number, counting from 0. The --tab flag when it is given.

- `--tab <index>` — Which tab to show, counting from 0.

```sh
unluminous-cli terminal select 1
unluminous-cli terminal select --tab 1
```

### terminal close

```
unluminous-cli terminal close [index] [--tab <index>]
```

Close a terminal tab. Closing the last one puts the terminal away.

- `index` (optional) — Its number. The --tab flag when it is given, the tab that is showing when both are left out.

- `--tab <index>` — Which tab to close, counting from 0. The one that is showing when it is left out.

```sh
unluminous-cli terminal close
unluminous-cli terminal close --tab 1
```

### terminal rename

```
unluminous-cli terminal rename <name> [--tab <index>]
```

Call a terminal tab something else. The name stays put when the program in the tab sets a title of its own; an empty name puts the tab back to being named after its program.

- `name` — What to call it. Everything after the verb is taken as the name, so it needs no quotes. Everything after it on the line belongs to it.

- `--tab <index>` — Which tab, counting from 0. The one that is showing when it is left out.

```sh
unluminous-cli terminal rename build
unluminous-cli terminal rename --tab 1 the long running one
```

### terminal move

```
unluminous-cli terminal move <position> [--tab <index>]
```

Move a terminal tab along the strip, which is what dragging one does.

- `position` — Where it goes, counting the tabs as they are on the screen now from 0.

- `--tab <index>` — Which tab to move, counting from 0. The one that is showing when it is left out.

```sh
unluminous-cli terminal move 0
unluminous-cli terminal move --tab 2 0
```

### terminal send

```
unluminous-cli terminal send [text] [--tab <index>] [--no-enter] [--key <name>]
```

Send a command to the shell in a terminal tab, the one that is showing when --tab is left out. Naming a tab does not show it. Enter is pressed for you unless you say not to.

- `text` (optional) — The command. Everything after the verb is taken as the command, so it needs no quotes. Everything after it on the line belongs to it.

- `--tab <index>` — Which tab to send to, counting from 0. The one that is showing when it is left out.
- `--no-enter` — Type the text and leave it on the prompt without running it.
- `--key <name>` — Send a key instead of text: enter, tab, escape, up, down, left, right, backspace, ctrl-c, ctrl-d, ctrl-l.

```sh
unluminous-cli terminal send git status
unluminous-cli terminal send --tab 1 cargo check
unluminous-cli terminal send --key ctrl-c
unluminous-cli terminal send --no-enter cd ..
```

### terminal read

```
unluminous-cli terminal read [--tab <index>] [--lines <number>] [--wait-for <text>] [--timeout <milliseconds>]
```

Read what a terminal tab has on its screen, the one that is showing when --tab is left out. Reading a tab does not show it.

- `--tab <index>` — Which tab to read, counting from 0. The one that is showing when it is left out.
- `--lines <number>` — Only the last so many lines.
- `--wait-for <text>` — Wait until this text is on the named tab's screen before answering, which is how to wait for a command to finish.
- `--timeout <milliseconds>` — How long to wait for --wait-for. 10000 by default.

```sh
unluminous-cli terminal read --lines 20
unluminous-cli terminal read --tab 1
unluminous-cli terminal read --tab 1 --wait-for "$" --timeout 15000
```

### terminal height

```
unluminous-cli terminal height [points]
```

Read how tall the terminal tile is, or set it. The same measurement dragging its top edge changes.

- `points` (optional) — How tall to make it. Read it when this is left out.

```sh
unluminous-cli terminal height 400
```

## run — the named commands the project is started with

A run configuration is a named command line, a folder and some environment variables, kept in the project. Starting one runs the program in a pseudoterminal, so `run output` is what it would have printed to a terminal — which is how to start a dev server, read the port out of its log, use it, and stop it, with nobody watching.

### run list

```
unluminous-cli run list
```

The project's run configurations: the name, the command, the folder and the environment of each, whether it is permanent, temporary or a suggestion, and what its run is doing.

```sh
unluminous-cli run list --json
```

### run add

```
unluminous-cli run add <name> <command> [--directory <path>] [--env <pairs>]
```

Keep a new run configuration in the project. The command is one line: the first word is the program and the rest are its arguments, and no shell runs it, so nothing is expanded and && is an argument. It says so when the program cannot be found on this window's PATH, and keeps the configuration anyway.

- `name` — What to call it, which is what the widget and the Run menu show.
- `command` — The command line. Everything after the name is taken as the command, so it needs no quotes. Everything after it on the line belongs to it.

- `--directory <path>` — The folder it runs in, relative to the project. The project itself when it is left out.
- `--env <pairs>` — NAME=value pairs separated by semicolons.

```sh
unluminous-cli run add "Dev server" node server.js --port 3000
unluminous-cli run add build cargo build --release --directory crates/unluminous-app
unluminous-cli run add serve npm run dev --env "PORT=3000; DEBUG=app:*"
```

### run remove

```
unluminous-cli run remove <name>
```

Take a run configuration away. One whose program is still running is stopped first.

- `name` — The configuration, as `run list` gives it.

```sh
unluminous-cli run remove "Dev server"
```

### run start

```
unluminous-cli run start [name]
```

Run a configuration, showing the run tile. Starting one that is already running stops it and starts it again rather than making a second copy. A detector's suggestion started this way is kept as a temporary configuration. A program that could not be started is a failure carrying the reason, not a reply that says nothing ran.

- `name` (optional) — The configuration. The chosen one when it is left out.

```sh
unluminous-cli run start
unluminous-cli run start "Dev server"
```

### run stop

```
unluminous-cli run stop [name]
```

Stop a run: the interrupt a program can catch, and a hard kill two seconds later or on a second stop. The tab stays, holding what the program wrote. Stopping when nothing is running is a failure that says so rather than a quiet success.

- `name` (optional) — The configuration. The chosen one when it is left out.

```sh
unluminous-cli run stop
unluminous-cli run stop "Dev server"
```

### run rerun

```
unluminous-cli run rerun [name]
```

Stop a run and start it again, whatever state it was in.

- `name` (optional) — The configuration. The chosen one when it is left out.

```sh
unluminous-cli run rerun
```

### run select

```
unluminous-cli run select <name>
```

Choose which configuration the widget's play button, the Run menu and `run start` with no name all mean.

- `name` — The configuration.

```sh
unluminous-cli run select "Dev server"
```

### run output

```
unluminous-cli run output [name] [--tail <number>] [--wait-for <text>] [--timeout <milliseconds>]
```

What a run has written, as text. It ran in a pseudoterminal, so this is what it would have printed to a terminal — colours and progress bars included, with the escape sequences already read.

- `name` (optional) — The configuration. The run that is showing when it is left out.

- `--tail <number>` — Only the last so many lines.
- `--wait-for <text>` — Wait until this text has been written before answering, which is how to wait for a server to say it is listening.
- `--timeout <milliseconds>` — How long to wait for --wait-for. 10000 by default.

```sh
unluminous-cli run output --tail 20
unluminous-cli run output "Dev server" --wait-for "Listening on" --timeout 30000
```

### run status

```
unluminous-cli run status [name]
```

Whether a run is going, and what it ended with: running, finished, stopped, or the exit code it chose.

- `name` (optional) — The configuration. The chosen one when it is left out.

```sh
unluminous-cli run status --json
unluminous-cli run status "Dev server" --json
```

## debug



### debug start

```
unluminous-cli debug start [name] [--wait-for-pause] [--timeout <milliseconds>]
```

Run a configuration under its debugger, showing the debug tile. The file that is open decides which debugger: its language names one, or the session refuses with a sentence saying what to install. Starting one replaces the session that was running.

- `name` (optional) — The configuration. The chosen one when it is left out.

- `--wait-for-pause` — Wait until the program stops somewhere before answering, so a script can set a breakpoint, start, and read a variable in three commands.
- `--timeout <milliseconds>` — How long to wait for --wait-for-pause. 30000 by default.

```sh
unluminous-cli debug start
unluminous-cli debug start "Dev server" --wait-for-pause
```

### debug stop

```
unluminous-cli debug stop
```

End the session: the polite request first, and a hard disconnect on a second stop or two seconds later. The debuggee's tab in the run tile stays, holding what it wrote.

```sh
unluminous-cli debug stop
```

### debug continue

```
unluminous-cli debug continue [--wait-for-pause] [--timeout <milliseconds>]
```

Let the program run on to the next breakpoint.

- `--wait-for-pause` — Wait until it stops again before answering.
- `--timeout <milliseconds>` — How long to wait. 30000 by default.

```sh
unluminous-cli debug continue --wait-for-pause
```

### debug step-over

```
unluminous-cli debug step-over [--wait-for-pause] [--timeout <milliseconds>]
```

Run the current line and stop on the next one, without going into any call it makes.

- `--wait-for-pause` — Wait until it stops again before answering.
- `--timeout <milliseconds>` — How long to wait. 30000 by default.

```sh
unluminous-cli debug step-over --wait-for-pause
```

### debug step-into

```
unluminous-cli debug step-into [--wait-for-pause] [--timeout <milliseconds>]
```

Go into the call on the current line and stop at its first line.

- `--wait-for-pause` — Wait until it stops again before answering.
- `--timeout <milliseconds>` — How long to wait. 30000 by default.

```sh
unluminous-cli debug step-into --wait-for-pause
```

### debug step-out

```
unluminous-cli debug step-out [--wait-for-pause] [--timeout <milliseconds>]
```

Finish the function the program is in and stop in whatever called it.

- `--wait-for-pause` — Wait until it stops again before answering.
- `--timeout <milliseconds>` — How long to wait. 30000 by default.

```sh
unluminous-cli debug step-out --wait-for-pause
```

### debug run-to

```
unluminous-cli debug run-to <path> <line> [--wait-for-pause] [--timeout <milliseconds>]
```

Run until the program reaches a line, then stop there. A temporary breakpoint, a resume, and the breakpoint taken away again — which is how every debugger builds this.

- `path` — The file, relative to the project or absolute.
- `line` — The line, counting from 1.

- `--wait-for-pause` — Wait until it stops before answering.
- `--timeout <milliseconds>` — How long to wait. 30000 by default.

```sh
unluminous-cli debug run-to src/main.rs 42 --wait-for-pause
```

### debug breakpoint

```
unluminous-cli debug breakpoint <add|remove|enable|disable|list|clear> [path] [line] [--condition <expression>] [--log <message>]
```

Where the program is to stop. `add` and `remove` take a file and a line; `list` prints every one in the project, with what the debugger said about it while a session is running. Breakpoints are kept in .unluminous/breakpoints.conf and move with the text as the file is edited.

- `action` — add, remove, enable, disable, list, or clear.
- `path` (optional) — The file. Not needed for list or clear.
- `line` (optional) — The line, counting from 1.

- `--condition <expression>` — Stop only while this is true. The debugger evaluates it, in the program's own language.
- `--log <message>` — Print this instead of stopping, which is what other editors call a logpoint. The debugger formats it, so {name} reads a variable.

```sh
unluminous-cli debug breakpoint add src/main.rs 42
unluminous-cli debug breakpoint add src/main.rs 42 --condition "attempts > 3"
unluminous-cli debug breakpoint list --json
unluminous-cli debug breakpoint remove src/main.rs 42
```

### debug frames

```
unluminous-cli debug frames [--include-subtle]
```

The call stack of the stopped thread, one frame a line: the function, the file and the line. Adapter-marked runtime frames are hidden unless --include-subtle asks for the complete stack. Answered from what the debugger has already been asked, so it costs nothing.

- `--include-subtle` — Include adapter-marked runtime frames, which are normally hidden so the stack leads with application code.

```sh
unluminous-cli debug frames --json
unluminous-cli debug frames --include-subtle
```

### debug variables

```
unluminous-cli debug variables [--frame <number>] [--expand <path>]
```

The variables of the frame that is showing. Only what has been read is printed, because a debugger reads a structure's contents only when somebody opens it; --expand asks for one row's children by name.

- `--frame <number>` — Which frame, counting from 0 at the top. The one that is showing when it is left out.
- `--expand <path>` — Read the children of this row and print them, naming it the way `variables` prints it — Locals/items.

```sh
unluminous-cli debug variables --json
unluminous-cli debug variables --expand Locals/items
```

### debug set-value

```
unluminous-cli debug set-value <path> <value>
```

Change a variable in the running program. The answer is the value as the debugger now sees it, which is not always what was typed.

- `path` — The row, as `variables` names it — Locals/count.
- `value` — The new value, in the program's own language. Everything after it on the line belongs to it.

```sh
unluminous-cli debug set-value Locals/count 7
```

### debug hover

```
unluminous-cli debug hover [--offset <bytes>] [--line <number>] [--column <number>] [--expression <text>] [--expand <path>] [--timeout <milliseconds>]
```

What a person sees when they rest the pointer on a name while the program is stopped: the expression Unluminous reads at that position, its value and type, and its children as a tree. Reads the name plus the field path in front of it, so a point on `count` in `self.items.count` asks about the whole of it. Unlike `evaluate`, the answer can be walked into with --expand.

- `--offset <bytes>` — Ask about this position in the file rather than about the caret.
- `--line <number>` — Ask about this line, counting from 1.
- `--column <number>` — The column on that line. 1 when it is left out.
- `--expression <text>` — Ask about this expression outright rather than about a position, which is how a value from `evaluate` is expanded.
- `--expand <path>` — Open this row and read its children, naming it the way the rows are printed - self.items/0.
- `--timeout <milliseconds>` — How long to wait for the debugger. 10000 by default.

```sh
unluminous-cli debug hover --line 42 --column 9 --json
unluminous-cli debug hover --expression self.items --expand self.items/0
```

### debug set-expression

```
unluminous-cli debug set-expression <expression> <value>
```

Assign to whatever an expression names in the running program. The other half of `set-value`: that one names a row that has already been read, and this one names the target in the program's own language, so it reaches a value nothing has opened yet. A debugger that cannot compile an assignment still changes a plain variable it has already shown, and says so plainly when it can do neither.

- `expression` — What to assign to, in the program's own language - self.items.count.
- `value` — The new value, in the program's own language. Everything after it on the line belongs to it.

```sh
unluminous-cli debug set-expression self.items.count 7
```

### debug evaluate

```
unluminous-cli debug evaluate <expression> [--timeout <milliseconds>]
```

Evaluate an expression in the frame that is showing. The debugger's own answer, or its own refusal.

- `expression` — The expression. Everything after the verb is taken as it was typed, so it needs no quotes. Everything after it on the line belongs to it.

- `--timeout <milliseconds>` — How long to wait for the answer. 10000 by default.

```sh
unluminous-cli debug evaluate items.len()
```

### debug watch

```
unluminous-cli debug watch <add|remove|list> [expression]
```

Expressions re-evaluated at every stop. `add` and `remove` take one; `list` prints them with their last answers.

- `action` — add, remove, or list.
- `expression` (optional) — The expression, for add and remove. Everything after it on the line belongs to it.

```sh
unluminous-cli debug watch add attempts
unluminous-cli debug watch list --json
```

### debug output

```
unluminous-cli debug output [--tail <number>]
```

What the debugger itself has said: what it loaded, what it could not find, and why it refused something. Not the program's own output, which goes to the run tile and is read with `run output`.

- `--tail <number>` — Only the last so many lines.

```sh
unluminous-cli debug output --tail 20
```

### debug status

```
unluminous-cli debug status [--wait-for-pause] [--timeout <milliseconds>]
```

Whether a session is running and what it is doing: starting, running, paused with the file and line it stopped at, or ended with the code the program chose.

- `--wait-for-pause` — Wait until the program stops before answering, which is how a script waits for a breakpoint it has already set.
- `--timeout <milliseconds>` — How long to wait. 30000 by default.

```sh
unluminous-cli debug status --json
unluminous-cli debug status --wait-for-pause
```

### debug adapters

```
unluminous-cli debug adapters
```

Which debuggers this Unluminous drives, where each one is on this machine, what is missing, and the command that installs it. The first thing to run when a debug session will not start: it answers in fields under --json, so nothing has to be guessed at from a refusal.

```sh
unluminous-cli debug adapters
unluminous-cli debug adapters --json
```

### debug install

```
unluminous-cli debug install <lldb|node>
```

Install a debug adapter by running its own install command in the run tile, where it can be watched with `run output` and stopped. Unluminous itself downloads nothing: what runs is a package manager, or an editor's extension installer, named by `debug adapters`.

- `adapter` — Which debugger: lldb or node.

```sh
unluminous-cli debug install lldb
```

## explorer — create, move and inspect the live project tree

For requests to create, move, delete or list project paths, start here instead of using shell file operations. Unluminous updates its live tree immediately, and `new-file` also opens the file in a tab. `explorer files` leaves out `target`, `node_modules` and `__pycache__`.

### explorer show

```
unluminous-cli explorer show
```

Show the file explorer down the left.

```sh
unluminous-cli explorer show
```

### explorer hide

```
unluminous-cli explorer hide
```

Collapse the file explorer, leaving the rail of buttons.

```sh
unluminous-cli explorer hide
```

### explorer toggle

```
unluminous-cli explorer toggle
```

Show the explorer if it is hidden, and hide it if it is showing.

```sh
unluminous-cli explorer toggle
```

### explorer width

```
unluminous-cli explorer width [points]
```

Read how wide the explorer is, or set it. The same measurement dragging its edge changes.

- `points` (optional) — How wide to make it, from 150 to 620. Read it when this is left out.

```sh
unluminous-cli explorer width 320
```

### explorer filter

```
unluminous-cli explorer filter [text]
```

Read the explorer's filter box, or type into it. The tree then shows only what matches.

- `text` (optional) — What to filter by. The box is cleared when this is left out. Everything after it on the line belongs to it.

```sh
unluminous-cli explorer filter tdd
unluminous-cli explorer filter
```

### explorer expand

```
unluminous-cli explorer expand <path>
```

Open a folder in the tree, and every folder above it.

- `path` — The folder, relative to the project or absolute.

```sh
unluminous-cli explorer expand crates/unluminous-app/src
```

### explorer collapse

```
unluminous-cli explorer collapse [path]
```

Shut a folder in the tree.

- `path` (optional) — The folder. Every open folder is shut when this is left out.

```sh
unluminous-cli explorer collapse crates
unluminous-cli explorer collapse
```

### explorer tree

```
unluminous-cli explorer tree [--limit <number>]
```

The rows the explorer is showing, in order, with the depth of each and whether it is a folder.

- `--limit <number>` — At most this many rows. 200 by default.

```sh
unluminous-cli explorer tree --json
```

### explorer files

```
unluminous-cli explorer files [--limit <number>]
```

Every file in the project that Unluminous searches, which leaves out what a build wrote: target, node_modules and __pycache__.

- `--limit <number>` — At most this many paths. 500 by default.

```sh
unluminous-cli explorer files --limit 20 --json
```

### explorer select-open-file

```
unluminous-cli explorer select-open-file
```

Scroll the explorer to the file that is showing and select it, opening out the folders above it. It happens on its own when the tab changes; this asks for it by hand.

```sh
unluminous-cli explorer select-open-file
```

### explorer select

```
unluminous-cli explorer select [path]
```

Set the row the explorer's own cursor is on, which is what Delete is about, or read it when no path is given. It is not the same as the tab that is showing.

- `path` (optional) — The file or folder to select.

```sh
unluminous-cli explorer select README.md
unluminous-cli explorer select --json
```

### explorer delete

```
unluminous-cli explorer delete <path>
```

Delete a file or a folder. On Windows it goes to the Recycle Bin; everywhere else it is gone. No question is asked, because typing the command is the deliberate act the question exists to ask for.

- `path` — The file or folder to delete.

```sh
unluminous-cli explorer delete notes/old.md
```

### explorer move

```
unluminous-cli explorer move <path> <folder> [--dry-run] [--no-refactor]
```

Move a file or a folder into another folder, rewriting every import, use line and mod declaration in the project that names it. The same thing dragging a row in the explorer does.

- `path` — The file or folder to move.
- `folder` — The folder it goes into.

- `--dry-run` — Print the whole change set and change nothing at all.
- `--no-refactor` — Move the bytes and leave every reference to them alone.

```sh
unluminous-cli explorer move src/app/layout.ts src/draw
unluminous-cli explorer move src/app/layout.ts src/draw --dry-run --json
```

### explorer new-file

```
unluminous-cli explorer new-file <path>
```

Make an empty file, create its parent folders, update Unluminous's live tree and open it in a tab. The same thing New -> File on the explorer's right click menu does, without the dialog.

- `path` — Where the file goes, relative to the project or absolute.

```sh
unluminous-cli explorer new-file notes/today.md
```

### explorer new-folder

```
unluminous-cli explorer new-folder <path>
```

Make a folder and every folder above it, updating Unluminous's live tree immediately. The same thing New -> Folder on the explorer's right click menu does, without the dialog.

- `path` — Where the folder goes, relative to the project or absolute.

```sh
unluminous-cli explorer new-folder src/services
```

### explorer reload

```
unluminous-cli explorer reload
```

Read the project's folders again, so anything another program has just made appears. It happens on its own within a second; this asks for it now.

```sh
unluminous-cli explorer reload
```

### explorer reveal

```
unluminous-cli explorer reveal <path>
```

Show a path in the platform's own file manager: Explorer on Windows, Finder on macOS.

- `path` — The file or folder.

```sh
unluminous-cli explorer reveal README.md
```

## modal — every dialog, driven the same way

One set of commands drives all of them: open it, type in it, read its results, choose a row, accept or cancel. A modal added to Unluminous later is driven with these same commands.

### modal list

```
unluminous-cli modal list
```

The modals that can be opened, and which one is open now.

```sh
unluminous-cli modal list --json
```

### modal open

```
unluminous-cli modal open <command-palette|go-to-file|find-in-files|settings|about|new-file|rename> [--query <text>] [--path <path>] [--page <name>]
```

Open a modal, and put something in its box in the same breath.

- `name` — command-palette, go-to-file, find-in-files, settings, about, new-file or rename.

- `--query <text>` — Type this into the modal's box as it opens.
- `--path <path>` — The folder a new file goes in, or the file being renamed. Needed by new-file and rename.
- `--page <name>` — Which page the Settings modal shows: appearance, editor, plugins, terminal or mcp.

```sh
unluminous-cli modal open go-to-file --query mdrs
unluminous-cli modal open find-in-files --query "fn main"
unluminous-cli modal open settings --page terminal
unluminous-cli modal open about
unluminous-cli modal open new-file --path notes
```

### modal state

```
unluminous-cli modal state
```

What the modal that is open is showing: its name, what is in its box, how many results it has and which one is chosen.

```sh
unluminous-cli modal state --json
```

### modal type

```
unluminous-cli modal type [text] [--match-case]
```

Put text in the box of the modal that is open, as though it had been typed.

- `text` (optional) — What to put in the box. The box is cleared when this is left out. Everything after it on the line belongs to it.

- `--match-case` — Turn on Find in Files' match case tick box while typing.

```sh
unluminous-cli modal type unluminous-cli
unluminous-cli modal type --match-case Unluminous
```

### modal results

```
unluminous-cli modal results [--limit <number>] [--wait <milliseconds>]
```

What the modal that is open has found: the files Go to File matched, or the lines Find in Files matched.

- `--limit <number>` — At most this many. 50 by default.
- `--wait <milliseconds>` — Wait up to this long for a search that is still running to finish.

```sh
unluminous-cli modal results --limit 10 --json
unluminous-cli modal results --wait 5000 --json
```

### modal choose

```
unluminous-cli modal choose <index>
```

Move the chosen row in the modal that is open, without opening anything.

- `index` — The row, counting from 0.

```sh
unluminous-cli modal choose 2
```

### modal accept

```
unluminous-cli modal accept [index]
```

Do what pressing Enter in the modal does: open the chosen file, jump to the chosen match, or press the modal's main button.

- `index` (optional) — Choose this row first.

```sh
unluminous-cli modal accept
unluminous-cli modal accept 0
```

### modal cancel

```
unluminous-cli modal cancel
```

Shut the modal that is open without doing anything, the way Escape does.

```sh
unluminous-cli modal cancel
```

### modal move

```
unluminous-cli modal move [--x <points>] [--y <points>]
```

Drag the modal that is open to a place on the window, the way its header does.

- `--x <points>` — How far from the left of the window its left edge goes.
- `--y <points>` — How far from the top of the window its top edge goes.

```sh
unluminous-cli modal move --x 60 --y 60
```

### modal size

```
unluminous-cli modal size [--width <points>] [--height <points>]
```

Resize the modal that is open, the way its edges do.

- `--width <points>` — How wide to make it.
- `--height <points>` — How tall to make it.

```sh
unluminous-cli modal size --width 900 --height 600
```

### modal reset

```
unluminous-cli modal reset
```

Put the modal that is open back in the middle at the size it asked for, the way a double click on its header does.

```sh
unluminous-cli modal reset
```

## settings — Edit -> Settings, by the names in the settings file

The names are the ones in Unluminous's own `settings.conf`, so there is one vocabulary rather than two. A change takes effect at once, in every tab, and is written to the file.

### settings list

```
unluminous-cli settings list
```

Every setting, with its value, what it means and what it will accept. The names are the ones in Unluminous's own settings file.

```sh
unluminous-cli settings list --json
```

### settings get

```
unluminous-cli settings get <key>
```

Read one setting.

- `key` — The name, such as appearance.font.size.

```sh
unluminous-cli settings get appearance.font.size
```

### settings set

```
unluminous-cli settings set <key> <value>
```

Change one setting. It takes effect at once, in every tab, and is written to the settings file.

- `key` — The name, such as appearance.background.opacity.
- `value` — The new value. Everything after it on the line belongs to it.

```sh
unluminous-cli settings set appearance.font.size 20
unluminous-cli settings set appearance.background.opacity 0.5
unluminous-cli settings set editor.line_numbers false
unluminous-cli settings set terminal.shell cmd.exe
unluminous-cli settings set terminal.shell_integration true
unluminous-cli settings set appearance.font.family "Courier New"
```

### settings reset

```
unluminous-cli settings reset [key]
```

Put a setting, or every setting, back to what an Unluminous that has never been run has.

- `key` (optional) — The setting. All of them when it is left out.

```sh
unluminous-cli settings reset appearance.font.size
unluminous-cli settings reset
```

### settings fonts

```
unluminous-cli settings fonts [--limit <number>]
```

The font families this machine has that the editor can be set to.

- `--limit <number>` — At most this many. 100 by default.

```sh
unluminous-cli settings fonts --json
```

## theme — the colours the whole window is painted in

A theme says what every name in Unluminous's own palette means, which drawn icon set the rail and the explorer use, and — when it names all nine token colours — how code is coloured in every language at once. Unluminous's own theme names none of the nine, so each language plugin's scheme is what colours its files until a theme is chosen. `settings set appearance.theme` reaches the same code; these exist because a setting cannot say what themes there are.

### theme list

```
unluminous-cli theme list
```

Every theme that can be chosen, with the plugin it came from and the six colours it is most recognisable by, and which one the window is painted in now.

```sh
unluminous-cli theme list --json
```

### theme show

```
unluminous-cli theme show [theme]
```

One theme in full: every colour in Unluminous's palette by name, the nine token colours, and which drawn icon set it uses. The one the window is painted in when no theme is named.

- `theme` (optional) — Its key, `themes-bundle-1/dracula`, or its name, `Monokai Pro`. The active one when it is left out.

```sh
unluminous-cli theme show --json
unluminous-cli theme show "Monokai Pro" --json
```

### theme set

```
unluminous-cli theme set <theme> [--accent <colour>] [--icons <set>]
```

Paint the window in a theme. It takes effect at once, in every tab and every pane, and is written to the settings file.

- `theme` — Its key, `themes-bundle-1/dracula`, or its name, `Material Deep Ocean`. Everything after it on the line belongs to it.

- `--accent <colour>` — One colour for everything the accent means, as #RRGGBB. `none` puts it back to the theme's own.
- `--icons <set>` — Which drawn icon set to use: material, classic, or `follow` for whichever the theme names.

```sh
unluminous-cli theme set themes-bundle-1/dracula
unluminous-cli theme set "Monokai Pro" --icons material
unluminous-cli theme set unluminous/dark --accent none
```

## plugins — the languages Unluminous colours, and the panes it draws

A plugin is a folder of data. A `language` one describes a language: its extensions, its keywords and a colour per kind of token. A `ui` one names code that shipped in the binary and says what it contributes — a pane, a tab, a menu, a Settings page — which is how the Agent-Tasks board, the Agent-Chat pane and the Database explorer are reached. Nothing in a plugin is executed and nothing is fetched over a network.

### plugins list

```
unluminous-cli plugins list
```

The plugins Unluminous has, which of them are switched on, and what each one claims: a language it colours, or a pane, a tab, a menu and a Settings page it contributes. They ship with Unluminous; nothing is fetched.

```sh
unluminous-cli plugins list --json
```

### plugins install

```
unluminous-cli plugins install <id>
```

Write a plugin out into the settings folder, so its files can be read and changed.

- `id` — The plugin's id, as `plugins list` gives it.

```sh
unluminous-cli plugins install rust
```

### plugins enable

```
unluminous-cli plugins enable <id>
```

Switch a plugin on, so it colours the files it claims.

- `id` — The plugin's id.

```sh
unluminous-cli plugins enable rust
```

### plugins disable

```
unluminous-cli plugins disable <id>
```

Switch a plugin off. Its files stay where they are.

- `id` — The plugin's id.

```sh
unluminous-cli plugins disable rust
```

### plugins show

```
unluminous-cli plugins show <id>
```

One plugin in full: its manifest, whether it describes a language or draws, what it contributes, and every command it answers.

- `id` — The plugin's id, as `plugins list` gives it.

```sh
unluminous-cli plugins show agent-tasks --json
```

### plugins reload

```
unluminous-cli plugins reload
```

Read every plugin manifest from disk again, so one changed by hand takes effect with no restart. One that will not parse is skipped with its reason.

```sh
unluminous-cli plugins reload --json
```

### plugins pane

```
unluminous-cli plugins pane <pane> [--show] [--hide] [--side <side>]
```

Show, hide or move a pane a plugin contributed: what its rail button and a drag on its header do.

- `pane` — The pane, as `<plugin id>/<pane id>` — `agent-tasks/board`.

- `--show` — Show it, building the plugin's own state the first time.
- `--hide` — Put it away.
- `--side <side>` — Dock it to left, right, top or bottom.

```sh
unluminous-cli plugins pane agent-tasks/board --show
unluminous-cli plugins pane agent-tasks/board --side bottom
```

### plugins tab

```
unluminous-cli plugins tab <tab> [--open] [--close]
```

Open or close a plugin's own tab in the editing area: a tab with no file behind it.

- `tab` — The tab, as `<plugin id>/<tab id>` — `agent-tasks/board`.

- `--open` — Open it, or show it if it is already open.
- `--close` — Close it.

```sh
unluminous-cli plugins tab agent-tasks/board --open
```

### plugins run

```
unluminous-cli plugins run <id> <command> [arguments]
```

Run one of a plugin's own commands, down the same path its menu entry and its buttons take. `plugins show` lists them.

- `id` — The plugin's id.
- `command` — The command, as `plugins show` lists it.
- `arguments` (optional) — The rest of the line, handed to the command as it stands. Everything after it on the line belongs to it.

```sh
unluminous-cli plugins run agent-tasks board --json
unluminous-cli plugins run agent-tasks new-task Rewrite the importer
unluminous-cli plugins run agent-tasks start task-27
unluminous-cli plugins run database tables public --json
unluminous-cli plugins run database query select count(*) from member
unluminous-cli plugins run database new-table public.shelf id:integer:pk title:text:notnull
unluminous-cli plugins run database set 1 title Kind of Green
unluminous-cli plugins run database submit
```

### plugins view

```
unluminous-cli plugins view <id>
```

What a plugin's pane holds, as data rather than pixels: for Agent-Tasks the sprint, the four lanes, their counts and their cards. A screenshot cannot answer how many tickets are in progress; this can.

- `id` — The plugin's id.

```sh
unluminous-cli plugins view agent-tasks --json
```

## git — status, changed files and the Git menu

Use this tool first when asked for git status, uncommitted work, changed files or a diff in the open project; do not begin by running git in a shell. Call `status` for the branch and exact staged, unstaged and untracked file list, then `action` with `name: show-diff` and `path` to open a changed file's diff in Unluminous. These still run the machine's real git with its credential helper, SSH agent, configuration and hooks, on a thread; `wait` holds the answer open.

### git status

```
unluminous-cli git status
```

What the machine's real git says about the project: the branch, whether a merge or rebase is unfinished, and what the last command returned, using the same credentials, SSH agent, configuration and hooks as the terminal.

```sh
unluminous-cli git status --json
```

### git actions

```
unluminous-cli git actions
```

Everything on the Git menu, by the name `git action` takes.

```sh
unluminous-cli git actions --json
```

### git action

```
unluminous-cli git action <name> [--path <path>] [--wait <milliseconds>]
```

Run one of the entries on the Git menu. Git runs on a thread, so the answer says it was asked for, and --wait holds on for what came back.

- `name` — The entry, such as commit, push, pull, fetch, branches or annotate.

- `--path <path>` — The file it is about. The file that is showing when it is left out.
- `--wait <milliseconds>` — Wait up to this long for git to answer before returning.

```sh
unluminous-cli git action fetch --wait 20000
unluminous-cli git action annotate
unluminous-cli git action show-history --path README.md
```

### git branches

```
unluminous-cli git branches
```

The branches of the project, saying which one is checked out and which live on a remote. This is what the branch button in the title bar lists, read from the status git already reported rather than by running a git command.

```sh
unluminous-cli git branches --json
```

### git switch

```
unluminous-cli git switch <branch> [--wait <milliseconds>]
```

Move to a branch that already exists, which is what choosing one from the title bar's branch button does. It goes through the machine's real git on a thread, so uncommitted work that would be overwritten stops it with git's own message rather than being discarded; `git action new-branch` is how a branch is started.

- `branch` — The branch to move to, as `git branches` names it.

- `--wait <milliseconds>` — Wait up to this long for git to answer before returning.

```sh
unluminous-cli git switch main --wait 20000
```

## action — every menu entry there is

The escape hatch, and the guarantee: every entry on every menu has a name here, and the list is built by walking the real menus, so a menu entry added to Unluminous tomorrow can be run from the command line tomorrow.

### action list

```
unluminous-cli action list [--menu <name>]
```

Every entry on every menu, with the name `action run` takes, the menu it is on, its keyboard shortcut and whether it can be used just now. A new menu entry appears here without anybody adding it. Ask for one menu with --menu and the answer is only that menu.

- `--menu <name>` — Only the entries on this menu, by the name it is shown under; submenus name their own rows. Several, comma-separated, for more than one. Every menu when it is left out.

```sh
unluminous-cli action list --json
unluminous-cli action list --menu view --json
```

### action find

```
unluminous-cli action find [text] [--limit <number>]
```

Menu entries whose wording matches some text, best first - the Find Action palette's own list as data. Use it rather than reading all of `action list` when you roughly know the name: the letters match as a subsequence, so `tln` finds Toggle Line Numbers, and the menu counts, so `git commit` finds Commit.

- `text` (optional) — What to look for. Every entry, in menu order, when it is left out. Everything after it on the line belongs to it.

- `--limit <number>` — Print at most this many rows. 20 when it is left out, and 0 means all of them.

```sh
unluminous-cli action find line numbers --json
unluminous-cli action find comment --json
```

### action run

```
unluminous-cli action run <name> [--path <path>]
```

Run a menu entry by name. This is the way to reach something with no command of its own; the entries that would open a file chooser are refused, and the answer says which command to use instead.

- `name` — The entry, as `action list` gives it, such as toggle-line-numbers.

- `--path <path>` — The file or folder the entry is about, for the ones that take one.

```sh
unluminous-cli action run toggle-line-numbers
unluminous-cli action run about
```

## project — the folder this window is showing

A project is a window. Opening a second project is `unluminous-cli launch <folder>`, which starts a second Unluminous; `project open` changes the folder this window is showing.

### project open

```
unluminous-cli project open <folder>
```

Show another folder in this window. What was open in the project being left is written down first.

- `folder` — The folder to show.

```sh
unluminous-cli project open C:\jason\dev\unluminous
```

### project recent

```
unluminous-cli project recent
```

The projects that have been open, newest first.

```sh
unluminous-cli project recent --json
```

## mcp — the server an AI agent drives Unluminous through

The Model Context Protocol server, which is how an AI agent discovers and drives Unluminous without being handed a document first. Its tools are generated from this same catalogue, so a command added to Unluminous is a tool the day it is added.

### mcp serve

```
unluminous-cli mcp serve [--transport <stdio|http>] [--port <number>] [--tools <grouped|every>] [--instance <which>]
```

Run the Model Context Protocol server, which is how an AI agent drives Unluminous. Over stdin and stdout by default, which is what an agent that launches it wants; over HTTP with `--transport http`.

- `--transport <stdio|http>` — How the client talks to it. `stdio` by default.
- `--port <number>` — Which port to listen on, for `--transport http`. 7345 by default.
- `--tools <grouped|every>` — One tool per area, or one tool per command. `grouped` by default.
- `--instance <which>` — Which running Unluminous to drive, when several are running.

```sh
unluminous-cli mcp serve
unluminous-cli mcp serve --transport http --port 7345
```

Answered by the CLI itself; no Unluminous needs to be running.

### mcp status

```
unluminous-cli mcp status
```

What this Unluminous is doing about MCP: whether it is serving over HTTP, on which port, in which tool shape, how many tools that is, and where an agent's configuration should point.

```sh
unluminous-cli mcp status --json
```

### mcp install

```
unluminous-cli mcp install <claude|codex|both> [--transport <stdio|http>] [--port <number>] [--scope <user|project>] [--name <name>] [--remove]
```

Write Unluminous's MCP server into an agent's own configuration, so it is there next time the agent starts.

- `client` — `claude`, `codex`, or `both`.

- `--transport <stdio|http>` — Which way the agent should talk to it. `stdio` by default, which needs no port.
- `--port <number>` — The port to point at, for `--transport http`.
- `--scope <user|project>` — `user` for every project, `project` for this folder only. `user` by default.
- `--name <name>` — What the server is called in the agent's configuration. `unluminous` by default.
- `--remove` — Take it out again rather than putting it in.

```sh
unluminous-cli mcp install both
unluminous-cli mcp install claude --scope project
```

Answered by the CLI itself; no Unluminous needs to be running.

### mcp config

```
unluminous-cli mcp config [claude|codex] [--transport <stdio|http>] [--port <number>] [--name <name>]
```

Print the configuration to paste into an agent that has no button of its own: the JSON an `mcpServers` block wants, and the TOML Codex wants.

- `client` (optional) — `claude` or `codex`. Both when it is left out.

- `--transport <stdio|http>` — Which way to describe. `stdio` by default.
- `--port <number>` — The port to name, for `--transport http`.
- `--name <name>` — What to call the server. `unluminous` by default.

```sh
unluminous-cli mcp config
unluminous-cli mcp config codex --transport http
```

Answered by the CLI itself; no Unluminous needs to be running.

### mcp tools

```
unluminous-cli mcp tools [--tools <grouped|every>] [--areas <list>] [--count]
```

The tools the MCP server offers, exactly as it would answer `tools/list`. This is how to see what an agent will be given, and how the cost of the two shapes is compared. With --areas it says what equipping an agent with only some of the catalogue would cost, which is the number `mcp serve --areas` is chosen against.

- `--tools <grouped|every>` — Which shape to print. `grouped` by default.
- `--areas <list>` — Only the areas named, separated by commas. All of them when it is left out.
- `--count` — Print how many tools and how large the list is, rather than the list.

```sh
unluminous-cli mcp tools --json
unluminous-cli mcp tools --tools every --count
unluminous-cli mcp tools --count --areas editor,git
```

Answered by the CLI itself; no Unluminous needs to be running.


<!-- end generated reference -->
