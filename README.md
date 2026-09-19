# Unluminous

**A text editor for macOS and Windows, written in Rust. The desktop shows through it while the text
stays solid, and everything a person can do in the window an agent can do too — through the same
command, reaching the same code, held to the same tests.**

[unluminous.com](https://unluminous.com) &nbsp;·&nbsp;
[Install](#install) &nbsp;·&nbsp;
[Documentation](documentation/README.md) &nbsp;·&nbsp;
[What it looks like](documentation/overview.md) &nbsp;·&nbsp;
[Command reference](unluminous-cli/docs/commands.md) &nbsp;·&nbsp;
[Black Rainbow Labs](https://blackrainbowlabs.com)

It opens any file holding text, has a file explorer with folders that expand in place, and a terminal
along the bottom with tabs. It is also an editor you can write code in — line numbers, a tab per file,
twelve language plugins, git in full, completion, go to definition, run configurations and a debugger
— and an infinite canvas holding terminals, web pages, folder trees, editors, agent chats and a ticket
board wired to each other.

[![Unluminous open on a Markdown file in the side by side view, the desktop showing through the window](documentation/images/01-unluminous-window.jpg)](documentation/overview.md)

*[Forty-five more](documentation/overview.md), each cropped with a margin of desktop round the edge,
because a picture cropped tight to the window cannot show that the colour in the editing area is the
wallpaper rather than a shade somebody chose.*

|  |  |  |
|---|---|---|
| **213 commands, 24 areas** | everything the menus, the keyboard and the mouse can ask for, `--json` on every one | [The command line](documentation/command-line.md) |
| **28 MCP tools, 27,011 tokens** | one tool an area rather than one a command, measured against 58,579 for the other shape | [For AI agents](documentation/for-ai-agents.md) |
| **Eight crates, and only one may mention a window** | which is why most of Unluminous is tested with no window, no graphics card and no fonts | [Architecture](documentation/architecture.md) |
| **16 plugins, and nothing in one is executed** | twelve languages, three panes, five themes. A plugin is a folder, so installing one is copying it | [How plugins work](documentation/plugins.md) |
| **Nothing is ever fetched** | not a plugin, not a debug adapter, not a picture in a Markdown file, not telemetry, nothing at startup | [Not included](documentation/not-included.md) |

---

## Install

**[unluminous.com](https://unluminous.com) has the Windows installer**, with its size and its SHA-256
beside it, which is the way in for somebody who does not want to build anything.

To build one yourself:

```powershell
powershell -File installer\windows\build.ps1 -Install
```
```bash
installer/macos/build.sh --install
```

On Windows that is a single `UnluminousSetup-<version>-x64.exe` that puts Unluminous in the Start
Menu, on the `PATH` and in *Open with*; on macOS an `Unluminous.app` and a disk image to drag into
`/Applications`. To build without installing, `cargo build --release`.

[Getting started](documentation/getting-started.md) covers the switches, windows and projects, and
where the settings live.

## A first window

```sh
unluminous .                 # the folder in the explorer
unluminous README.md         # that file, in the folder it is in
```

The window has no operating system frame, because rounded corners and a translucent background need
the decorations turned off — so it draws its own title bar, its own menus on Windows, and its own
eight resize grips. `Settings -> Appearance -> Background` is how far the desktop shows through, from
5 per cent to 100, and every glyph is painted at full opacity at every setting.

[What it looks like](documentation/overview.md) is thirty-seven captures of the window and
[the Database plugin](documentation/database.md) is nine more, all of this version. One command takes
all forty-six, without taking the keyboard: [Taking the pictures](documentation/taking-the-pictures.md).

## Driving it from the command line

```sh
unluminous-cli launch .                                   # start an Unluminous here and wait for it
unluminous-cli tab open README.md                         # open a file
unluminous-cli editor view preview                        # look at its Markdown preview
unluminous-cli terminal send cargo test                   # run something in the terminal
unluminous-cli terminal read --wait-for "test result"     # wait for it, and read what it said
unluminous-cli window screenshot shot.png                 # a real picture of the window
```

213 commands across 24 areas, and `--json` makes every answer machine-readable.
[The command line](documentation/command-line.md) is how it works, down to the socket;
[`unluminous-cli/docs/commands.md`](unluminous-cli/docs/commands.md) is the reference, written to be
handed to an AI agent whole.

## Giving it to an AI agent

`Settings -> Tools -> MCP`, then **Install for Claude Code** or **Install for Codex**. Restart the
agent and it can drive Unluminous: open files, read and change the text, run things in the terminal,
search the project, work the Git menu, drive the debugger, and take a screenshot of the real window
and look at it.

The tools are **generated from the same catalogue the command line is**, so a command added to
Unluminous is a tool the day it is added and there is no second list to fall behind. By default an
agent is given one tool an area rather than one a command, which names everything Unluminous does for
less than half of what the other shape costs.

[For AI agents](documentation/for-ai-agents.md) is the whole of it, including what a fixed open port
does and does not defend against — which is why it is off until you turn it on.

## Every feature is reachable by an agent, and that is a test

> **Every piece of functionality a person can reach, an agent can reach, through the same command,
> and both are covered by automated tests.**

Not a plugin bolted on, not a subset of the interesting parts. `Ctrl/Cmd+Shift+O` and
`unluminous_modal open go-to-file` are one feature. A breakpoint set by clicking the gutter and one
set by `unluminous_debug breakpoint add` are the same breakpoint.

That is a contract about *new work* as much as about what is here. A feature that ships with a menu
entry and no way for an agent to ask for it is an unfinished feature, in the same way a feature with
no test is an unfinished feature. Three mechanisms make it true rather than aspirational, and none of
them is a promise anybody has to remember to keep:

- **A menu entry needs nothing at all.** `unluminous-cli action list` is built by walking the real
  menus, so an entry added tomorrow can be run from the command line tomorrow. A test fails the day a
  menu entry has no name.
- **Anything with no menu entry is a row in the catalogue** — the one list the client parses against
  and the window dispatches on. `run_cli` is to that list what `run_action` is to the menus: the
  single place a command turns into a change, using the same path a person's click takes.
- **The MCP tools are generated from that catalogue**, and a test fails if a command is ever not
  offered.

And the documentation is a test too: one fails while a command has no section in
`unluminous-cli/docs/commands.md`, while a usage line is out of date, or while a section describes a
command that no longer exists.

**Reachable is not the same as reached**, and the difference was measured: a local model driving a
real window across 23 scenarios sent 24% of its tool calls to its own `grep`, `bash`, `read` and
`edit` for jobs Unluminous has a first-class command for.
[For AI agents](documentation/for-ai-agents.md#reachable-is-not-the-same-as-reached) is what came of
that, and it is the harder half of the contract.

## What it does

**Editing and prose.** Select, cut, copy, paste, undo and redo, with undo restoring a state rather
than applying an inverse. Bold, italic, underline, strikethrough, colour, four alignments and three
line spacings for a file that is prose, and **absent** for one that is not. A file is written back
with the line endings it was read with, so a one character edit is a one line diff. →
[Editing](documentation/editing.md)

**Markdown and diagrams.** Three view modes, a preview that is a real document rather than a second
renderer — so its text selects and copies, and scrolling either half of the side by side view scrolls
the other through the text rather than through the height. Twenty of Mermaid's thirty diagram types
are **drawn**, in Rust, with nothing fetched and nothing run; the other ten are named rather than
mis-drawn. → [Editing](documentation/editing.md#diagrams)

**Code.** Line numbers, tabs, syntax colouring from twelve language plugins, completion that offers
only what was already in memory, go to definition, find all references, rename across the project,
folding, and a split editing area. Where the mechanism cannot tell two same-named things apart it
shows both rather than guessing one. → [Writing code in it](documentation/writing-code.md)

**Git, in full.** Unluminous runs the `git` program rather than a library, so a push from Unluminous
is the same push you get in your terminal — the same credential helper, the same ssh agent, the same
hooks, the same signing — and when something goes wrong it shows **git's own message**. →
[Writing code in it](documentation/writing-code.md#git)

**Running and debugging.** A run configuration is a named command line, and no shell runs it. Pressing
the bug beside the play button starts the same configuration under a debugger: Unluminous speaks the
Debug Adapter Protocol, so one client drives every language's. Nothing is fetched — pressing Debug
with no adapter installed is one sentence naming what was looked for and the command that installs it.
→ [Writing code in it](documentation/writing-code.md#debugging)

**A real terminal.** 24 bit colour, the alternate screen, ten thousand lines of scrollback, mouse
reporting, and the shell a person actually uses rather than whatever `COMSPEC` says. A tab comes back
in the folder it was in, showing what was on it. → [The terminal](documentation/the-terminal.md)

**An infinite canvas.** Six kinds of node — a terminal, a web page, a folder tree, a file editor, an
agent chat and the task board — wired to each other, where a connection is what lets an agent running
in a terminal node act on the node it is wired to. A zoom costs a matrix rather than a relayout. →
[The Base of Infinite Space](documentation/the-canvas.md)

**Agents beside your work.** A chat pane that runs the `claude` or `codex` already installed on this
machine, so Unluminous holds no key at all and the agent brings its own tools and its own permission
model; and a task board whose tickets are worked by an agent in a terminal Unluminous owns. →
[The agent panes](documentation/agent-panes.md)

**Databases.** PostgreSQL, SQLite and Inillucent in a pane of its own: a tree, query consoles and row
editors. A row can only be changed if it can be addressed, and there is no fallback. No password is
ever written down. → [The Database plugin in pictures](documentation/database.md)

## Documentation

[**`documentation/README.md`**](documentation/README.md) is the index, in the order a new reader wants
them.

| | |
|---|---|
| [What it is](documentation/what-it-is.md) | what Unluminous is, who it is for, and the case for it |
| [What it looks like](documentation/overview.md) | thirty-seven captures of the running window |
| [Getting started](documentation/getting-started.md) | install, run, the switches, windows and projects |
| [The window](documentation/the-window.md) | every part named, the panels, the nine menus, every Settings page |
| [Editing](documentation/editing.md) | text, files, encodings, find and replace, highlights, folding, Markdown, diagrams |
| [Writing code in it](documentation/writing-code.md) | tabs, completion, navigation, git, running, the debugger |
| [The terminal](documentation/the-terminal.md) | the tile, the tabs, which shell, and how one comes back |
| [The Base of Infinite Space](documentation/the-canvas.md) | the canvas, its six node kinds, and what a connection grants |
| [The agent panes](documentation/agent-panes.md) | Agent-Chat and Agent-Tasks |
| [The Database plugin](documentation/database.md) | the tree, a grid, a console, a pending change |
| [For AI agents](documentation/for-ai-agents.md) | the contract, how it is enforced, and what a study found |
| [The command line](documentation/command-line.md) | the commands, the channel, and the MCP server |
| [Architecture](documentation/architecture.md) | eight crates, one frame, the seams, the threads, what it costs |
| [How plugins work](documentation/plugins.md) | why a plugin is data, and the manifest key by key |
| [Tests](documentation/testing.md) | the four layers, and what the release scripts check |
| [Not included](documentation/not-included.md) | everything deliberately absent, with the reason for each |
| [Taking the pictures](documentation/taking-the-pictures.md) | how the gallery is made, in one command |

## Limits

- **No language server.** Go to definition, find all references and rename are a syntactic index built
  from the token stream, which is the tier Sublime Text's goto-definition is. A definition found by a
  heuristic is marked as one all the way to the screen, and where two same-named things cannot be told
  apart both are shown.
- **Right to left and complex writing systems are not supported.** One grapheme cluster is placed
  after another from left to right, which is correct for Latin, Greek and Cyrillic and wrong for
  Arabic and Hindi.
- **No several carets at once, and no column selection.**
- **A file that is not UTF-8 opens read-only.** A UTF-16 byte order mark and Latin-1 are read and
  named in the status bar; saving one is refused rather than attempted, because re-encoding somebody's
  file into a scheme this version has not been asked to get right is a worse answer than saying no.
- **Ten of Mermaid's thirty diagram types are named rather than drawn**, and the twenty that are drawn
  are not pixel identical to `mermaid.js`, because none of it runs `mermaid.js`.
- **No screen reader support in 1.0.** The accessibility tree is on and every control has a plain
  name; `design/accessibility.md` is what is still missing and the plain answer.
- **A screenshot Unluminous takes never contains a rendered web page**, because a page is a native
  child window the operating system composites on top of the surface a screenshot captures.
- **No light theme**, and it is refused rather than half-supported: the window is drawn on a
  transparent ground and its depth is a surface lifted and a shadow of black.
- **No continuous integration**, by choice. The release scripts run the suite before they will tag
  anything, so a release cannot be made from a checkout whose tests do not pass.

[Not included](documentation/not-included.md) is the whole list with the reason for each.

## Building it

```sh
cargo build --release
cargo test
```

Then [Architecture](documentation/architecture.md) for the crate layout and
[Tests](documentation/testing.md) for the four layers and how each number here is reproduced.

## Licence, and how to take part

Dual licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option — the licence
pair Rust itself uses, so Unluminous can be vendored into either kind of project without a decision.
Any contribution you deliberately submit is under those same terms, with no further conditions.

- [`CONTRIBUTING.md`](CONTRIBUTING.md) — why a feature is three things, how to run each of the four
  test layers, which crate a change goes in, and the house style.
- [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) — be decent to people; argue about code.
- [`SECURITY.md`](SECURITY.md) — where to report a vulnerability, what is in scope and what is not.
- [`CHANGELOG.md`](CHANGELOG.md) — what changed in each release.
