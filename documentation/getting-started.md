# Getting started

## Install it

**[unluminous.com](https://unluminous.com)** has the Windows installer, with its size and its SHA-256
beside it, which is the way in for somebody who does not want to build anything.

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

### The switches

All of them exist so a starting state can be chosen without clicking, which is what makes it possible
to capture the window in a particular state.

| Switch | What it does |
|---|---|
| `--opacity N` | The starting background opacity, from 0.05 to 1.0. The same setting as `Settings -> Appearance -> Background`. |
| `--view raw\|side\|preview` | Which of the three ways of looking at a Markdown file it starts on. |
| `--terminal` | Open the terminal at the bottom straight away. |
| `--background` | Open the window **without making it the foreground window**, so neither the keyboard nor the virtual desktop moves away from whatever is in front. |
| `--menu-bar native\|in-window` | Where the menus are drawn. macOS uses the bar along the top of the screen and everything else uses Unluminous's own title bar; naming it is how the bar inside the window can be looked at on a Mac. |
| `--control on\|off` | Whether this window listens for the command line. On unless it is turned off, which closes the channel `unluminous-cli` drives it down. |
| `--replay-screen <file>` | Print the bytes in that file and then become the program named after `--`. This is how a terminal node comes back showing what was on it; it is not meant to be typed. |
| `--print-menus` | Print the menus and their shortcuts, and stop. |
| `--version` | Print the version and the build date, and stop. The same two facts `Unluminous -> About Unluminous` shows. |

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
| `highlights.txt` | every marked passage in the project, as `<start> <end> <#rrggbbaa> <path>` |
| `breakpoints.conf` | every breakpoint, as the byte offset of its line's start |
| `run-configurations.conf` | the run configurations that were kept |
| `space.conf` | the canvas: its views, its nodes, their connections and where the camera was |

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
| `plugins/` | the plugins installed by hand, each a folder that shadows a bundled one of the same id |
| `instances/<pid>.conf` | the port a running window is listening on and the token a request has to carry |

A setting Unluminous does not recognise is kept and ignored rather than making the file refuse to load,
and a file with a stray line in it starts with defaults rather than not starting.

## The first five commands

Everything the menus, the keyboard and the mouse can ask for is a command, and `--json` makes every
answer machine-readable.

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
