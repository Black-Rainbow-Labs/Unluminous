# Getting started

## Install it

**[unluminous.com](https://unluminous.com)** has the Windows installer and the macOS archive, each with
its size and its SHA-256 beside it, which is the way in for somebody who does not want to build
anything. `https://unluminous.com/releases/latest.json` holds the same addresses, sizes and hashes for
a script to read.

The Windows installer installs for the current user into `%LOCALAPPDATA%\Programs\Unluminous` with no
administrator prompt. It offers these tasks:

| Task | What it does |
|---|---|
| `desktopicon` | A shortcut on the desktop. |
| `addtopath` | Puts `unluminous` and `unluminous-cli` on the `PATH`. |
| `contextfile` and `contextfolder` | `Open with Unluminous` on the right click menu of a file and of a folder. |
| `associate` | Offers Unluminous in `Open with` for text, Markdown and source files. |
| `connectagents` | Runs `unluminous-cli setup`, which connects Claude Code, Codex, OpenCode and Pi to Unluminous. On by default. |
| `voice` | Checks microphone access for speech to text. |

For macOS, unzip the archive, move `Unluminous.app` to `/Applications`, and run
`/Applications/Unluminous.app/Contents/MacOS/unluminous-cli setup`.

The installers ship the speech model and ONNX Runtime, so dictation downloads nothing. A silent
Windows install is `UnluminousSetup-<version>-x64.exe /VERYSILENT /SUPPRESSMSGBOXES /CURRENTUSER`,
which runs the default tasks.

To build one yourself:

```powershell
powershell -File installer\windows\build.ps1 -Install
```
```bash
installer/macos/build.sh --install
```

`installer/` builds a real installer for each platform out of the same drawn icon: on Windows a single
`UnluminousSetup-<version>-x64.exe` that puts Unluminous in the Start Menu, on the `PATH` and in
*Open with*, and on macOS an `Unluminous.app` and a disk image to drag into `/Applications`.
`installer/README.md` says what each switch does.

To build without installing:

```sh
cargo build --release
```

That produces `unluminous` and `unluminous-cli` in `target/release`.
[Repository and building](architecture.md#building-it) is the rest.

## Run it

```sh
unluminous .
unluminous README.md
cargo run --release -- sample/welcome.md
```

The argument is a folder to show in the explorer, or a file to open, in which case the explorer shows
the folder that file is in. With no argument the explorer shows the current directory — except when
the current directory is the folder `unluminous.exe` itself lives in, which is what a desktop shortcut
gives you and nobody chose, and then it reopens the windows that were open last time.

### The first window, with no project

A window started from the desktop, with no path named and no recent project that still exists, would
otherwise open the folder it is standing in: the root of the file system for a macOS bundle, the
installation folder on Windows. Walking and indexing that raises permission errors and uses a great
deal of memory, so such a window opens on `No Folder Open`, an empty folder inside the settings folder.
It is not added to the recent projects, restores nothing, hides the explorer and starts a terminal in
the home folder. The title bar says `No folder open`.

In that window the **Set Up Unluminous** dialog opens by itself, whatever it was told before. It has a
Project card above the steps, with the folder chosen, `Open Folder...` and `Create Project...` (which
offers a folder in the home folder). Below it is one row for each step: the command line tool, each
coding agent it found, the speech model, the microphone, and the MCP server last. `Run Setup` takes the
ticked steps, and unticking one declines it. The project opens in a window of its own once setup has run
with no failed step, at once when there is nothing left to run (the button then says `Open Project`),
and at once after `Not Now`. `Open Folder`, `Create Project` and `Recent Projects` all go through the
same place while the window waits.

On a window that already has a project, the dialog opens by itself only while setup has never been
finished, and only in one window a session. After setup has been finished, a new gap such as an agent
installed later is a notice with buttons to run setup or decline it. `Unluminous -> Set Up Unluminous...`
opens the dialog by hand, and `Don't show this again` turns the automatic opening off. The choices are
kept in `setup.conf` in the settings folder. `unluminous-cli setup status|open|run|decline|close` is the
command line version of the dialog, and `unluminous-cli setup --check` says what is and is not set up
without changing anything.

### Connecting an agent

`unluminous-cli setup` puts `unluminous-cli` on the path, connects Claude Code, Codex, OpenCode and Pi
to Unluminous's MCP server, names the API keys that are set, and starts `mcp serve` to check it
answers. It uses each agent's own command to add the entry, so a file the agent rewrites is never
edited by hand while the agent runs. An entry that points at another program is left alone unless
`--force` is given, and running it twice changes nothing. Restart an agent to see the tools it was
given. `unluminous-cli setup --remove` takes the entries out again.

### The switches

All of them exist so a starting state can be chosen without clicking, which is what makes it possible
to capture the window in a particular state.

| Switch | What it does |
|---|---|
| `--opacity N` | The starting background opacity, from 0.05 to 1.0, and it switches the background on. The same setting as `appearance.background.opacity` in the Background card on `Settings -> Theme`. |
| `--view raw\|side\|preview` | Which of the three ways of looking at a Markdown file it starts on. |
| `--terminal` | Open the terminal at the bottom straight away. |
| `--background` | Open the window **without making it the foreground window**, so neither the keyboard nor the virtual desktop moves away from whatever is in front. `--no-activate` is the same switch. A window started this way does not join the session that `session.txt` remembers. |
| `--menu-bar native\|in-window` | Where the menus are drawn. macOS uses the bar along the top of the screen and everything else uses Unluminous's own title bar; naming it is how the bar inside the window can be looked at on a Mac. |
| `--control on\|off` | Whether this window listens for the command line. On unless it is turned off, which closes the channel `unluminous-cli` drives it down. |
| `--replay-screen <file>` | Print the bytes in that file and then become the program named after `--`. This is how a terminal node comes back showing what was on it; it is not meant to be typed. |
| `--print-menus` | Print the menus and their shortcuts, and stop. |
| `--setup-voice` | Download the speech model when this build did not come with it, ask about the microphone, and stop. The Windows installer's `voice` task runs it. |
| `--version` | Print the version and the build date, and stop. The same two facts `Unluminous -> About Unluminous` shows. |
| `--help` | Print the usage and stop. |

An argument beginning with a dash that is not one of those is **refused**, and Unluminous exits with
status 2 rather than opening anything. It used to be taken for the folder to open, so a mistyped
switch got a window on a folder of that name — and, because a project's state is kept beside the
project, the folder was then created. If you have a file or folder whose name really does start with a
dash, put a folder in front of it: `unluminous ./-notes`.

A program in the windows subsystem has no console, so `--version`, `--help` and `--print-menus` borrow
the calling terminal's console on Windows. A window does not, because one that attached itself would
write the graphics library's chatter over somebody's prompt for the rest of its life.

## Windows and projects

Several Unluminous windows can run at once, each on its own project. `File -> New Window` opens another
on the same project, `File -> Open Folder` opens one on a folder you choose, and
`File -> Recent Projects` opens one on a folder that has been open before. **Each is its own process**,
so they share nothing but the settings file — which is why a second window is a second process rather
than a second window of one process, and why the geometry a project remembers is the project's rather
than the person's.

What each project had open is kept in a `.unluminous` folder **beside the project**, next to `.idea`
and `.vscode` rather than in your own settings folder, so copying the project copies its state and two
people on one folder do not fight over one file. What is in it:

| File | What it holds |
|---|---|
| `workspace.conf` | which panels were showing, the window's position and size, the run configuration that was chosen |
| `open-files.txt` | the tabs, which pane each was in, where each was scrolled and where its caret was |
| `terminal-tabs.txt` | each terminal tab's name and the folder its shell had got to |
| `expanded-folders.txt` | which folders in the explorer were opened out |
| `browser-tabs.conf` | the back and forward list of each browser tab in the editing area |
| `designs.conf` | the Designer pane's tabs and the device a design was last viewed at |
| `highlights.txt` | every marked passage in the project, as `<start> <end> <#rrggbbaa> <path>` |
| `breakpoints.conf` | every breakpoint, as the byte offset of its line's start |
| `run-configurations.conf` | the run configurations that were kept |
| `realms/<path>.conf` | for each realm, what one person was doing in it: the camera, the chosen node, each terminal's session, where each sound or video was paused |
| `space.conf` | the canvas as builds before realm files kept it. Read once, to make the realm files, and never written or deleted. |

`session.txt` in the settings folder lists the windows that were open, so starting Unluminous from the
desktop reopens them. A session has a beginning: a window that opens while no listed window is still
running starts a new session. A window closed by hand takes its line out of the list unless it was the
only window open, and a window the operating system closes or one that crashes keeps its line.

Paths are written relative to the project wherever they are inside it, so a project that moves still
opens the files it was left with. Only the released binary reads or writes any of it: a test must not
touch the settings of the person running it, and a `.unluminous` folder written into a test's sample
project would change what the explorer draws in the middle of a test.

**A terminal comes back as a fresh shell in the folder it was in**, showing what was on its screen
when the window closed. What a program was doing cannot be brought back; what is restored is the same
number of shells, in the same folders, with the last screenful and a bounded amount of scrollback
printed back into them.

## Where your own settings live

`~/Library/Application Support/Unluminous` on macOS and `%APPDATA%\Unluminous` on Windows, in plain
text files that can be read and edited by hand.

| | |
|---|---|
| `settings.conf` | everything on the Settings pages, in `name = value` lines |
| `recent.txt` | the projects that have been opened |
| `session.txt` | the windows that were open during the last session, as a process id and a project |
| `setup.conf` | what was chosen in the Set Up Unluminous dialog |
| `modals.conf` | where each dialog was left, and its zoom |
| `ai-endpoints.conf` | the list of endpoints on `Settings -> AI Configuration` |
| `backgrounds/` | the pictures the window can be drawn on, and `.thumbnails/` for the grid |
| `update-checked.txt` | the day the last update check was made, shared by every window |
| `telemetry/` | usage statistics waiting to be sent, and what was sent when debugging is on |
| `plugins/` | the plugins installed by hand, each a folder that shadows a bundled one of the same id |
| `instances/<pid>.conf` | the port a running window is listening on and the token a request has to carry |

A setting Unluminous does not recognise is kept and ignored rather than making the file refuse to load,
and a file with a stray line in it starts with defaults rather than not starting.

## Keeping it up to date

`update.check` is `daily` by default: Unluminous asks unluminous.com once a day, across every window,
whether a newer version has been released, and shows a notice with two buttons. `Install & Restart`
downloads the installer, checks its size and its SHA-256, and refuses and deletes it when either does
not match. It then closes the window the ordinary way, so what each project remembers is written
first, runs the installer from a copy in the download folder, and starts the new `unluminous` with no
arguments, which reopens the windows of the last session. The helper writes `apply-update.log` beside
the installer, because it has no window. On macOS it unzips the archive and swaps the bundles.
`Don't Ask Again` stops the daily check offering that one version, and a later version is offered.

`start` asks every time a window opens and `off` asks nothing until `Unluminous -> Check for Updates`
or `unluminous-cli update check` is used. What is sent is one request that says nothing about the
person or the project. A copy that no installer put where it is, such as `target/release`, offers
`Open Download Page` instead of `Install & Restart`, because installing from there would install
somewhere else. `unluminous-cli update install`, `update skip` and `update status` do the same from
the command line.

## The first five commands

Everything the menus, the keyboard and the mouse can ask for is a command, and `--json` makes every
answer readable by a program.

```sh
unluminous-cli launch .                                   # start an Unluminous here and wait for it
unluminous-cli tab open README.md                         # open a file
unluminous-cli editor view preview                        # look at its Markdown preview
unluminous-cli terminal send cargo test                   # run something in the terminal
unluminous-cli terminal read --wait-for "test result"     # wait for it, and read what it said
unluminous-cli window screenshot shot.png                 # a real picture of the window
```

[The command line](command-line.md) is how it works and `unluminous-cli/docs/commands.md` is the
reference, written to be handed to an AI agent whole.

## Exit codes

| | |
|---|---|
| 0 | it worked |
| 1 | the window refused it, and said why |
| 2 | the command line was wrong: an unknown switch, a mistyped flag, a missing argument |

A mistyped flag is refused rather than quietly treated as text, because a command that did the wrong
thing without saying so is worse than one that did nothing.
