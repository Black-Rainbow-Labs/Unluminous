# task-2322: several projects in one window, as tabs

## What was asked

> We want the ability to open N projects in a window, where each project gets its own custom layout (same
> behavior as if they were open in different windows, but now I want to switch via tabs at the top).
>
> Let's move the git branch selector to the bottom right, where the branch is shown again, but has no ability
> to click on and switch branches. That way we have more room for tabs at the top.
>
> Next to the project name at the top left, there should be a + button, which allows me to open a new tab
> (again, the entire screen should be as if the project were opened in a new window. agent chat, etc should all
> be unique and tied to the project tab's root dir, etc)
>
> The tabs and + icon button should be styled similar to Realm tabs. I should be able to right click a tab and
> see a menu to close it.

## The decision: one `UnluminousApp` per project tab

`UnluminousApp` is one project in one window. It holds about 170 fields, and almost all of them belong to the
project: the open files, the explorer, the terminals, the run configurations, the debugger, git, the realm, the
plugin providers (Agent Chat, Agent Tasks, the Designer, the database plugin), the code index, the language
servers, the command channel. A handful belong to the window: the native menu bar on macOS, the native resize
subclass on Windows, the geometry, the decision to close.

Two ways were weighed.

| | Split `UnluminousApp` into a window and a project | Keep `UnluminousApp` as it is, and hold one per tab |
|---|---|---|
| Work | Every one of 56,000 lines that says `self.x` has to learn whether `x` is the window's or the project's | One new type that owns a list of `UnluminousApp`s and draws one of them |
| What a tab is | Whatever the split decided | Exactly what a second window is, which is what the ticket asks for |
| Risk to what works today | Every feature | The handful of window things, each named below |

The second is chosen. A project tab **is** a window without its own operating system window: it has its own
`UnluminousApp`, so its Agent Chat, its Agent Tasks board, its terminals, its realm, its run configurations, its
debug session, its code index and its plugin panes are its own and are rooted at its own folder, with no code in
any of them changed. "The same behavior as if they were open in different windows" is true by construction.

`app::project_tabs::ProjectWindow` is the new type. It implements `eframe::App`, and `main.rs` hands it to eframe
in place of the `UnluminousApp` it used to hand over. `UnluminousApp` keeps its own `eframe::App` implementation,
because every screenshot test builds one directly and a window with one tab is still an ordinary window.

## What the window owns and what a tab owns

### A tab owns everything about its project

Everything in `UnluminousApp` stays where it is.

### The window decides five things

1. **Which tab is showing.** Only that tab is drawn.
2. **What the tabs hear while they are not showing.** Every tab that is not showing is given
   `UnluminousApp::keep_working_while_hidden` every frame, which is what eframe already gives a minimised window:
   the command channel is answered, the debugger's replies are read, the kernels are heard, and every plugin
   gets its turn on the clock. So an agent driving a hidden tab through `unluminous-cli` is answered, Agent Chat in
   a hidden tab goes on streaming, and the Agent Tasks board in a hidden tab goes on running its watchdog.
3. **The settings.** The settings are one file per person. When a tab is shown, it is given the settings the
   tab that was showing had (`UnluminousApp::set_settings`), so a theme or a font chosen in one tab is the theme
   or font of every tab. **The pane sizes and where each panel is docked are not copied**: they are each tab's
   own, which is what "each project gets its own custom layout" means. Which panels are showing was already per
   project, in `.unluminous/workspace.conf`.
4. **The native things there is one of.** The macOS menu bar is moved to the tab that is showing
   (`native_menu`), the native browser views of the tab that stops showing are hidden, and the microphone is
   let go by a tab that stops showing.
5. **Closing.** The close button, `Close Window` and `Quit` close the window and every tab in it: eframe calls
   `ProjectWindow::on_exit`, and that calls `UnluminousApp::on_exit` for each tab, so each project writes down
   what was open in it and stops its programs.

## The title bar

Today the bar reads: the mark (macOS) or the menus (Windows), then the project's chip, then the branch widget,
then the drag area, then the text tools, the run widget and the window buttons.

It becomes: the mark or the menus, then a **carved track of project tabs** with the one showing raised, then a
round **`+` chip**, then the drag area, then what was there before.

- The track, the pills and the `+` chip are the Realm bar's: `rux`'s carved well 36 points tall, pills 28 points
  tall with the one showing raised in the chip gradient, the inactive ones in the quiet ink and the hovered one
  in the sky colour, and `rux::components::ChipButton` with `rux::Icon::Plus` for the plus. The first pill keeps
  the folder mark the project chip had, so a window with one project looks almost as it did.
- Each pill is named `Project tab: <folder name>`, which is what a test and assistive technology find it by. The
  plus is `New Project Tab`.
- Pills are cut short with an ellipsis and narrowed when the bar is crowded, and the ones that do not fit are
  listed after the last pill as `+N more`, which opens the same menu the plus opens.
- **A right click on a pill opens its menu**: `Close Project Tab`, `Close Other Project Tabs` and `Open in New
  Window`. `Close` on the only tab is dimmed: the window's own close button is how a window goes.
- **The plus opens a menu**: `Open Folder...` (the platform's folder picker), then the recent projects that are
  not already open in this window. Choosing one opens it in a new tab and shows it.
- A project that is already open in this window is shown rather than opened a second time, for the reason
  `OpenFiles::open` shows a file that is already open: two `UnluminousApp`s on one folder would be two writers
  of one `.unluminous` folder.

### The branch moves to the status bar

The status bar already draws the branch at its right hand end, beside the font. The title bar's branch widget is
taken out, so the branch is shown once and cannot be switched from there. Switching a branch is still
`Git -> Branches...` and `unluminous-cli git switch`.

## How a tab is reached from outside the window

`unluminous-cli` reaches a window through the instance file the window's command channel writes into
`<settings>/instances`, named after the process id. A tab has a command channel of its own, so it writes an
instance file of its own:

- `Instance` gains `tab`, which is 0 for the first tab a window opened and counts up from there, and `active`,
  which is true for the tab that is showing. The first tab's file is `<pid>.conf`, as it always was; another
  tab's is `<pid>-<tab>.conf`.
- `--instance` still takes a process id, a port or part of a project's path. A process id names every tab of
  that window, and the one showing is the answer. A port or a path names one tab.
- **A child of a tab is told that tab's port.** `UNLUMINOUS_INSTANCE` is what a terminal node, a chat's agent
  and a ticket's agent are started with so that `unluminous-cli` with nothing named reaches the window that
  started them. It was the process id. `ProjectWindow` sets the address of the tab it is running on the thread
  before it runs any of that tab's code (`services::control::running_for`), and `how_to_reach_this_window`
  reads it, so an agent started from a tab drives that tab and not whichever one happens to be showing. The
  client accepts a port in `UNLUMINOUS_INSTANCE` as well as a process id.
- Only the first tab hosts the MCP HTTP endpoint, because it listens on one fixed port.

## The agent's half

A menu entry needs nothing more, so these are menu entries on `File`, and `action run` reaches them:

| Action | Name | What it does |
|---|---|---|
| `NewProjectTab` | `new-project-tab` | Asks for a folder with the platform's folder picker and opens it in a new tab |
| `CloseProjectTab` | `close-project-tab` | Closes the tab that is showing, unless it is the only one |
| `NextProjectTab` | `next-project-tab` | Shows the tab after the one showing |
| `PreviousProjectTab` | `previous-project-tab` | Shows the tab before it |

The keys are `Ctrl+Alt+PageDown` and `Ctrl+Alt+PageUp` for the next and previous tab (`Cmd+Alt` on macOS),
which are free in `actions.rs` and in `editor_view::handle_input`.

And four commands in the catalogue, under `project`:

| Command | What it does |
|---|---|
| `project tabs` | Lists the tabs: number, folder, port, and which is showing |
| `project tab-open <folder>` | Opens a folder in a new tab and shows it, or shows the tab already on it |
| `project tab-show <tab>` | Shows a tab, named by its number or by part of its folder |
| `project tab-close [tab]` | Closes a tab, the showing one when none is named |

A command arrives at one tab's `UnluminousApp::run_cli`, and the tabs are the window's, so a tab records what
was asked (`UnluminousApp::project_tab_asked`) and answers at once; `ProjectWindow` acts on it at the end of the
frame. Every tab is given a read only copy of the tab list each frame (`UnluminousApp::project_tabs`), which is
what `project tabs` answers from and what the title bar draws.

## What a project remembers, and the session

- What was open in each project is in that project's `.unluminous` folder, as before. Each tab writes its own.
- **The session file holds a line per tab**, with the window's process id: `<pid> <folder>`. Several lines
  with one process id are one window's tabs, in order. `Store::open_window_groups` reads them as windows.
- At a start from the desktop, `main.rs` restores the last window's tabs in this process, and starts every other
  window with its first folder and `--tab <folder>` for each of the rest.
- Closing a tab by hand takes its line out of the session (`Store::forget_open_tab`). Closing a window by hand
  takes all of that window's lines out, unless it was the only window, in which case all of its lines are kept.
- Which tab was showing cannot be the order of the lines, because the order is the order of the tabs. The
  session line for the tab that is showing carries a `*` after the process id (`<pid>* <folder>`), and a
  restored window shows that tab. A file written by an older Unluminous has no `*`, and its window shows its
  first tab.

## egui's memory

Every tab draws into the same `egui::Context`, and egui keeps a widget's state (a scroll offset, a text box's
caret) by the widget's id. Ids are made from the `Ui`'s id, so every tab but the first is drawn inside
`ui.push_id(("project-tab", key))`, which gives its explorer, its lists and its scroll areas state of their own.
The first tab is not wrapped, so a window with one project is drawn with exactly the ids it was drawn with before.

## Faults this design has to avoid, and how

| What could go wrong | What stops it |
|---|---|
| A hidden tab's web page stays on the screen over the tab that is showing | `BrowserHost::hide_every_view` when a tab stops showing |
| Two tabs' browser tabs share an id, so one tab's page is drawn in the other | Browser tab ids come from one counter for the process |
| A hidden tab listens for "Sable" and answers it twice | `UnluminousApp::let_go_of_the_microphone` when a tab stops showing |
| A theme chosen in one tab is undone by another tab's start | `prepare` resets the theme, so the window applies the showing tab's settings again after a tab is opened |
| Two tabs host the MCP endpoint on one port | Only the first tab opens it |
| Usage statistics counted once per tab | Only the first tab is allowed to send them |
| A hidden tab's restored terminals start before anybody looks at them | They start on that tab's first drawn frame, as they always did; a tab nobody looks at costs no shells |

## Tests

In `crates/unluminous-app/tests/project_tabs.rs`, against a `ProjectWindow` built the way a test builds an
`UnluminousApp` (no store, no command channel):

1. A window opens a second project in a tab: two pills, the second showing, its explorer rooted at its folder.
2. Each tab keeps its own open files, its own explorer visibility, its own terminal visibility and its own panel
   sizes across switching back and forth.
3. Settings follow the tab: a theme set in one tab is the theme of the other once it is shown, and the pane
   sizes are not copied.
4. Opening a folder that is already open in a tab shows that tab rather than opening a second one.
5. Closing a tab shows its neighbour; the last tab cannot be closed from the tab menu.
6. The four commands, driven through `run_cli`: `project tabs`, `project tab-open`, `project tab-show`,
   `project tab-close`; and a command asked of a hidden tab is answered on the next frame.
7. A right click on a pill opens the menu, and `Close Project Tab` in it closes that tab.
8. The `+` opens its menu and lists recent projects not already open.
9. The branch widget is no longer in the title bar, and the status bar still names the branch.
10. A screenshot of the title bar with three tabs, accepted after looking at it.

Unit tests beside the code: `Store::open_window_groups` and the `*` mark, `Instance` with `tab` and `active`
round trips, `choose` preferring the showing tab, `arguments::read` with `--tab`, and the tab naming rule.

Then the release build, installed, driven through `unluminous-cli` with three projects, and photographed.
