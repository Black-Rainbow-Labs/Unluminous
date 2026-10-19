# The window

[What it looks like](overview.md) is this page in pictures. This one names the parts.

## What is in it

A **title bar** Unluminous draws itself, 46 points tall. At the left is the Unluminous mark, the
blue `U`, which opens the first menu. The other menus follow as pills, with the project as a chip
carrying its name and its branch. At the right are the text options, the three Markdown view modes,
the run widget and the window buttons. On Windows the three window buttons are flat strips and the
close button turns red under the pointer. On macOS the menus are in the bar along the top of the
screen instead, the window buttons are the traffic lights at the left, where macOS puts them, and the
mark sits just after them and does nothing when pressed. The bar's drag area starts after the mark.

Down the far left a **rail** 44 points wide, one round button per pane: the explorer and git at the
top, and at the bottom the things with a character grid in them. A button whose pane is showing is
carved into the rail. The marks are line drawings of a project, a document, git, the Realm, a chat
bubble, a board, a database, a bug, a run and a terminal, and a plugin that names a mark Unluminous
has no line drawing of keeps its own. The rail is the only way a pane is put away and brought back,
because it is in the same place whether a pane is showing or not. Right clicking a button offers the
four `Move to` rows even for a pane that is put away.

Then the **file explorer** with its filter box, a **tab** for each open file, the **gutter** of line
numbers, the **editing area**, whichever **tile** is showing along the bottom, and a **status bar**
naming the file, its kind, its line endings, the caret's position, the branch, how many files have
changed, and the font. At the far right of the status bar is a flag, which opens
[Report an Issue](#report-an-issue).

The window has no operating system frame, because rounded corners and a translucent background need
the decorations turned off. It is dragged by its title bar and resized from eight invisible grips.
On Windows the window answers its own hit test, so Windows shows the resize arrow, starts the resize
on the press itself and offers snapping. On macOS the window cannot hand a resize to the system, so
it moves its own edge, measured across the screen from where the drag began, and dragging the left
edge does not shake.

**Everything is painted at an absolute position** rather than through the interface library's layout,
because the measurements come from `design/intial-design-screenshot.png`. Run
`cargo run --example sample_design` to print the colour of each region of that image.

## The panels, and moving them

Five panels: the explorer, the terminal, the run tile, the debug tile and the Realm, plus one for
each pane a plugin contributes (Agent-Chat, Agent-Tasks, Database and Designer). Any of them is
dragged by its header to the top, bottom, left or right of the window, with four blue bands showing
where it can go. The strong band is the
layout run over the value the drop would produce, so the preview and the drop are one function and
cannot come apart.

Three rules settle every awkward case:

- **Order is screen order, always along x.** On the left, order 0 is the outermost column; on the
  right it is the one nearest the document, because on the right "left to right" starts in the middle.
- **The strips are taken first, across the whole width**, and the columns come out of what is left.
- **A panel carries two measurements and the side decides which is read** — a width for a column, a
  height for a strip — because one number cannot be both: the terminal is 260 points tall along the
  bottom and a 260 point wide column is half a terminal.

**Two character grids never share one strip.** The terminal, the run tile and the debug tile are one
thing at the bottom and never two, because two grids stacked take the editing area below the fold.
Move the terminal to the right and it stops competing, and both are showing.

### A side can run the whole length of its edge

Each corner of the window belongs to one of the two sides that meet there. By default the top and the
bottom own all four corners. A side **fills** when it owns both of its corners: a column that fills
runs from the top of the window to the bottom, and the strips stop at its edge. There are three ways
to do it:

- Drag a panel to the half of a band nearest the window's edge. That half says `Fill whole side`, the
  landing plate says for example `Terminal tile · whole bottom`, and the panel goes against the
  window's edge, outside every other panel on that side. The half nearest the document only moves the
  panel.
- Use `Fill Whole <Side>` on the panel's own right click menu, which toggles the side the panel is on.
- Run `unluminous-cli panel fill <side> [on|off]`, or `panel dock --fill`. `panel list` answers
  `fills` and `corners`.

Where each corner goes is written to your settings as `panes.corner.top-left = top` and so on.

The strong blue rectangle shows where the panel will land, and the drop does exactly that.

### Panes in the editing area

The empty part of a pane's tab strip carries the whole pane. Let go over the left half of another pane
and it goes before that pane, over the right half and it goes after. `unluminous-cli pane place` does
the same from the command line.

### Maximising

Two presses at the top of a panel maximise it, which is putting every other panel away and not a
fifth kind of layout. The menu entry is `View -> Maximize Pane`, on `Ctrl+Shift+M` (`Cmd+Shift+M` on
macOS). `Escape` or two more presses put the panels back. Showing or hiding a panel with its button
while one is maximised ends the maximise and opens only the panel that was asked for, without bringing
back the panels that were put away. `View -> Reset Panel Layout` is always a way back.

Every panel's edge is a divider that is dragged to resize it, and a double click puts it back to its
usual size. Where a panel is and how large it is lives in **your** settings rather than the project's,
because where the panels are is a habit and the same in every project you open.

## Zooming

The size of the whole interface is its own setting, `appearance.ui.font.size`, from 8 to 40 points,
14 by default, with 12.5 as the size the design was drawn at. It is a zoom of the window's interface,
so every menu, row, icon and dialog grows together. The editor, the terminal, the Markdown preview and
the chat keep the point sizes their own settings name.

Zooming a pane is `Ctrl/Cmd` with `+` and `-`, or the wheel or a pinch with that modifier held. The gesture belongs to
the window rather than to a pane, and the pointer decides whose it is: a pane under the pointer takes
it, and the pane with the keyboard takes it if no pane did.

What one step means differs by pane, deliberately. Where a pane already has a point size somebody
chooses, the zoom walks that setting, so there is one number saying how big the text is rather than a
setting and a multiplier that can disagree — the editing area walks `appearance.font.size` and the
three tiles walk `terminal.font.size`. The explorer and a pane a plugin contributed have no such
number, so theirs is a multiplier over everything they draw, kept in `panes.<name>.zoom`.

**The point under the pointer stays where it is.** In the editing area what is remembered is the text
rather than the offset: where the line the point fell on starts and how far down that line it sat,
worked out again at the new size rather than remembered. A scroll position means something different
the moment the text is laid out at a different size — `app/mod.rs` is 5,900 points tall at nine points
and 10,500 at sixteen — so a number that did not change is a reader half a file away from what they
were looking at.

## The menus

`Unluminous`, `File`, `Edit`, `Code`, `Find`, `View`, `Notebook`, `Run`, `Git` and `Plugins`. `Code` is
drawn for a file whose language can answer its questions, `Notebook` while a notebook tab is showing,
and `Plugins` when an installed plugin contributed entries. Both menu bars are built from one list, so
they hold the same entries with the same shortcuts. Run `unluminous --print-menus` to see it, or
`unluminous-cli action list`, which is built by walking the same list.

Shortcuts come from the keymap, which is applied last: the menus, the macOS menu bar, the command
palette and `action list` all read one answer. See [The keymap](#the-keymap).

| Menu | What is in it |
|---|---|
| `Unluminous` | About Unluminous, Check for Updates, Set Up Unluminous..., Report an Issue..., Settings, Quit. |
| `File` | New Window, Open File, New Jupyter Notebook, New Rust Notebook, Open Web Address, Go to File, Create Project, Open Folder, Recent Projects (with Forget Recent Projects), Reopen Closed Tab, Save, Save As, Close Window. Opening a folder opens it in a window of its own. |
| `Edit` | Undo, Redo, Cut, Copy, Paste, Select All, a `Highlight` submenu holding the four colours and the two ways of clearing one, Navigate Back and Navigate Forward on `Ctrl+[` and `Ctrl+]`, Settings. |
| `Code` | Go to Definition, Find Usages, Rename, Complete Word, Reformat, comment and indentation, Go to Line, Go to Matching Bracket, and the line commands: duplicate, move, join and sort, and Trim Trailing Whitespace. Drawn for a file whose language can answer those questions. |
| `Find` | Find Action, Find, Replace, Find Next, Find Previous, Find in Files. |
| `View` | The three view modes, Dictate into Agent-Chat (`Alt+V`), show or hide the explorer, the editing area and the line numbers, Maximize Pane, the font size, the tabs, Select Opened File, a `Split` submenu, a `Folding` submenu, the three tiles, the Realm and its own submenu, Reset Panel Layout, and the terminal tabs. |
| `Notebook` | Notebook Outline, and a `Cell` submenu holding the cell commands: add, delete, copy, move, merge, split, convert, clear outputs, collapse and run. See [Notebooks](writing-code.md#notebooks). |
| `Run` | Run, Stop, Rerun, Edit Configurations, and the debugger's entries. |
| `Git` | Commit, Add, Exclude, Show Diff, Compare with Revision, Show History, Show Current Revision, Annotate with Git Blame, Rollback, Push, Pull, Fetch, Merge, Rebase, Branches, New Branch, New Tag, Reset HEAD, Stash, Unstash, Manage Remotes, Clone. Dimmed outside a repository, and it grows `Continue` and `Abort` while a merge or a rebase has stopped on a conflict. |
| `Plugins` | What each installed plugin contributed. |

**Absent and dimmed mean different things.** Dimmed is a control that could be used in a moment — undo
with nothing yet to undo, the Git menu outside a repository. Absent is a control that can never apply
to this file: the `F` button is not drawn for a `.rs` file, and the three view modes are not drawn for
a `.txt` one.

## Modals

Every dialog is dragged by its header and resized from any of its four edges or four corners, and a
double click on its header puts it back in the middle at the size it started. All of it is in
`components::modal` rather than in any one dialog, so a dialog written later has it without asking.

**A working dialog opens nearly as large as the window.** A dialog that asks for 600 points of width
or more opens at the window's size less 3% on every side, and that margin is also the most it can be
resized to and the room it can be dragged in. A short question with a field or two in it, `Go to File`
and the command palette keep the size they ask for.

**Where every dialog was left survives a restart.** The placement and the zoom of each are written to
`modals.conf` in the settings folder when a drag ends.

**A dialog zooms.** While one is open, `Ctrl/Cmd` with the wheel, a pinch, and `Ctrl/Cmd` with plus,
minus and `Reset Font Size` all go to the dialog. The dialog stays in the same place on the screen
with everything in it larger, and its text is laid out again at the size it is seen at, so it stays
sharp. `unluminous-cli modal zoom [factor|reset]` does the same from the command line.

**The backdrop is a blurred copy of the window.** The previous frame is shrunk and blurred on the
graphics card and drawn under the dark backdrop on the dialog's first frame. Under the Unluminous Glass
theme the dialog is drawn over the frosted background picture instead.

**Enter presses the last button in the footer**, which is the one that does the thing and is filled in
the accent colour. A footer whose last button is dimmed is a modal there is nothing to confirm, so the
key press is left alone rather than doing nothing loudly. The commit panel is the one exception and
uses the command key with Enter, because its message box is a text area where Enter is a new line.

**A modal takes the keyboard from the panes behind it.** Without that, Enter in the delete
confirmation would delete the file *and* put a new line in the file behind it *and* open the row the
explorer's cursor was on.

### Notices

A notice at the edge of the window can carry buttons. The update offer has `Install & Restart` and
`Don't Ask Again`, and the notice that an agent was installed after setup has buttons to run setup or
decline it. A notice with buttons does not fade, and a later confirmation never pushes it off the
screen, because a question that was pushed away would never be answered.

### Report an Issue

The flag at the far right of the status bar, and `Unluminous -> Report an Issue...`, open a dialog
that writes up a problem on GitHub. Unluminous never posts the issue itself, because a token inside
the program could be taken out of it. It opens
`github.com/Black-Rainbow-Labs/Unluminous/issues/new` with the title and the body already filled in,
and you press GitHub's own Submit while signed in.

- The window is photographed when the dialog opens, with the dialog left out of that picture.
- The address stays under 8,000 bytes. The log is cut first from its oldest end, then the description,
  and whatever is cut is attached whole as `unluminous-log.txt` or `description.txt`, with a line in the
  body saying so.
- The files go on the system clipboard as files, so one `Ctrl/Cmd+V` on the issue page attaches them
  all. A second file with a name already attached is attached as `name-2.ext`.
- `Cancel` and `Escape` keep the draft. `Discard` and `Done` throw it away.

`unluminous-cli report open | status | set | attach | detach | screenshot | send | close` is the same
from the command line, and `report send --no-browser` answers the address and the files without opening
anything.

### Set Up Unluminous

The dialog that connects the command line tool, the coding agents, the speech model and the microphone
is described in [Getting started](getting-started.md#the-first-window-with-no-project). It is on
`Unluminous -> Set Up Unluminous...`.

## The explorer

The explorer is a list of the project's files with a filter box, and it follows the tab: the file
showing in the pane with the keyboard is selected, the folders above it are opened out, and the list
scrolls the least amount that brings the row into view. `View -> Select Opened File` asks for it by
hand.

**Several rows can be chosen.** A click with `Ctrl` or `Cmd` adds a row to the choice, and a click with
`Shift` chooses the run of rows from the last one clicked. A click with a modifier opens nothing and
folds nothing. Cut, Copy, Paste and dragging act on every chosen row when the row they are about is one
of them, which is what a right click on one of several chosen rows means in a file manager.

**Files travel through the system clipboard.** While the explorer has the keyboard it takes `Ctrl/Cmd+X`,
`C` and `V`. A cut or a copy also puts the files on the system clipboard, so they paste in a file
manager, and a paste takes files another program put there. A paste goes into the cursor's folder, or
the folder the cursor's file is in. A file dragged in from the Finder or Explorer lands in the folder
under the pointer, which is marked while the file is carried over the panel.

**The explorer holds the keyboard when it is clicked.** `Delete` asks whether to throw the chosen file
away (to the Recycle Bin on Windows), the arrow keys walk the list, `Enter` opens the file, and a
letter typed hands the keyboard to the editor. The empty space below the rows is the project folder's
own row, so its menu offers New but dims the entries about a particular file.

`unluminous-cli explorer choose <path> [--add] [--range]` is the click, `explorer copy-in <path>
<folder>` is the drop, and `explorer new-file`, `new-folder` and `reload` are the rest.

## The Settings window

`Edit -> Settings`, `Unluminous -> Settings`, or command and comma. It is a single surface 1000 by 700
points. Down the left is a card with the window's name, a search box and the pages as pills, the
chosen one carved in. On the right are the group and the page's name, a round `Close settings`
button, and the page's settings as raised cards. There is no footer and no `Done`: the round cross and
`Enter` close it. The window is **one size for every page** and the page scrolls inside it, because a
dialog that changed height as its list was walked would jump under the pointer.

| Page | What is on it |
|---|---|
| `Appearance & Behavior -> Appearance` | the font the window's own text is set in and the interface size, and whether a plugin's pane is drawn with depth |
| `Appearance & Behavior -> Theme` | the Background card (whether the window is drawn on a picture, which picture, and how opaque), the theme, the accent and the icon set |
| `Keymap` | the preset and every action's shortcut |
| `Editor -> Editor` | the editor's font and size, the line numbers, what one indent is, auto indent, trimming, the completion popup and language servers, the debugger's value tooltip, line endings, what search leaves out, the version check, and the usage statistics card |
| `Plugins` | the marketplace and what is installed |
| `Tools -> Terminal` | which program a terminal tab runs, the size it draws its grid at, and shell integration |
| `Tools -> MCP` | installing Unluminous into an agent, and the HTTP server |
| `Tools -> AI Configuration` | the endpoints Agent-Chat and Agent-Tasks pick from: Claude Code, Codex, OpenCode, Pi and addresses, with their keys |
| `Tools -> Voice` | speech to text, the endpoint, the microphone, and listening for "Sable" |
| `Plugins -> <plugin>` | a page for each plugin that contributed one: Agent-Chat, Agent-Tasks, Database and Designer |

Changes take effect as they are made. `unluminous-cli settings list` prints every setting with a
sentence each, `settings get` reads one and `settings set` writes one. A setting that is at its
default is left out of the settings file.

### The keymap

`Settings -> Keymap` chooses a preset and records your own shortcuts on top of it. `keymap.preset` is
`unluminous`, `vscode` or `intellij`, and a preset binds every action whose name it knows to the
chord that editor uses. A shortcut you set on an action's row is written as `keymap.key.<action>`.

- **Your binding beats the preset, and the preset beats Unluminous's own.**
- **A chord that another action holds is taken from that action**, and the page says so. `none` takes an
  action's shortcut away.
- **Shortcuts that typing and the clipboard read are refused**, such as `Ctrl+C` or a bare letter.
- **Every menu entry can be given a shortcut** the day it is added, because the keymap is applied to the
  list the menus are built from.

`unluminous-cli settings keymap [filter] [--changed]` lists every action with its shortcut and where
that came from (`unluminous`, `preset`, `yours` or `fixed`). `settings bind <action> <chord>` sets one,
with `--reset` and `--reset-all` to take your own shortcuts away.

### The font is one setting, and it reaches every tab

`appearance.font.family` and `.size` are one setting for the whole window, the way the reference editor has one
editor font. They are on the Editor page, and every font installed on the machine is offered, the
usual ten first. Changing it changes every file that is open, not only the one showing. The size is also
on the keyboard, on `View -> Reset Font Size`, and on a pinch or the wheel over the editing area, and
whichever of them is used it is the same setting, so it is still there next time.

**Setting it is not an edit.** Nothing goes onto any document's undo history and no file is marked as
having unsaved changes, because what Unluminous saves is plain text and carries no formatting.

`appearance.ui.font.family` is the family of the window's own text, the menus, the rail, the explorer
and the status bar, and `appearance.ui.font.size` is the interface size described under
[Zooming](#zooming). They are separate from the editor's font because a large document in a compact
window is a reasonable thing to want. An empty family means the editor's.

### Themes

A theme says what every colour in Unluminous's own palette means, and one that names the nine token
colours also colours code, in every language at once. The list of names is closed: **a theme says what
a name means; it cannot add a name**, and a role Unluminous has not got is refused with the list.

Eight themes ship. Unluminous carries three of its own: **Unluminous Dark**, which is the default,
**Unluminous Light** and **Unluminous Glass**. `Themes Bundle 1` adds five, every number in them read
out of the plugin jars of the reference editor they come from: Islands Dracula Colorful, Material
Palenight, Material Deep Ocean, Monokai Pro and One Dark.

**Unluminous Light** colours code itself, in a One Light scheme, because every language plugin's own
colours were chosen for a dark ground. Its terminal is light too, and the shadows the board and the
chat are drawn with are a cool grey under a white edge. A theme plugin can be light as well, with
`dark = false`. A terminal moves onto a new palette the next time it is drawn.

**Unluminous Glass** is a dark theme whose raised surfaces are glass: the background picture behind each
one is bent the way a lens bends it, with a faint colour split at the edge, and a tint is painted
over it. The whole window is one sheet of glass and every pane and the editing area is a glass panel
on it. Menus, dropdowns and tooltips are glazed too, and a canvas node's card is glass at full
strength. Glass bends a picture it is handed, so choosing the theme while no picture is chosen
chooses `space-milky-way-core.jpg`. Glass over the bare desktop has nothing to bend, so popups are made
nearly opaque. A still frame builds no glass, and it costs about 0.4 ms of a frame while the chat is
scrolled.

The **accent** is one colour over whatever the theme chose. The **icon set** is which drawn marks the
rail and the explorer's folder arrow use: `material` by default, or `classic` for the ones Unluminous
shipped with. `unluminous-cli theme list` says what there is and `theme set` paints the window in one.

### The background

The Background card is the first on the Theme page.

`appearance.background.enabled` is the switch, and it is **off** by default. Off, the window is
opaque and draws no picture, whatever `appearance.background.image` and `appearance.background.opacity`
say. Choosing a picture, passing `--opacity`, and choosing Unluminous Glass all turn it on.

`appearance.background.opacity` goes from 0.05 to 1.0, and 0.86 is the value a new settings file starts
with. The window is created transparent, the background is painted with that alpha and every glyph is
painted at alpha 255, so what is behind shows through the background while the writing stays sharp.
With no picture chosen, what shows through is the desktop. On macOS that is the whole story.

On Windows the same code drew a solid window, and three separate faults each had to be fixed: the
graphics backend, the swapchain and an uncleared surface.
[Architecture](architecture.md#the-window-lets-the-desktop-through) says what each one is.

**Nineteen 4K pictures ship in the binary**, named by subject (`abstract-`, `landscape-`, `nature-`,
`space-` and `wood-`) so the grid, which sorts by name, groups them. Each is written once into the
`backgrounds` folder in the settings folder, as a JPEG at quality 82, and after that it is an ordinary
file: the grid lists what is there, and removing one removes it, and it is not written again. A
picture you add is copied into the same folder. The grid shows 480 pixel thumbnails kept in
`.thumbnails`. The window shrinks the chosen picture to cover the screen it is on, so a 1080p screen
holds a 1080p texture. `unluminous-cli background` lists, adds, removes and chooses pictures.

### Usage statistics

Unluminous sends anonymous usage statistics to Google Analytics, and `telemetry.enabled` is `true` by
default. The `Usage statistics` card on the Editor page turns it off, and `false` stops counting and
sending at once.

- **Only names from closed lists are sent**: a menu entry's name, a command's name, a kind of file, a
  kind of endpoint, a panel's name. A plugin that did not ship with Unluminous is sent as `other`. Never
  a path, a file name, a project name, an address or anything typed.
- An `app_open` is sent ten seconds after a window opens, then one request about every twenty minutes
  while somebody is using the window, nothing while it is idle, and an `app_close` on the way out.
  Nothing in the window waits on the network.
- Only the installed program sends. A window started by a test never does, and a window started with
  `--background` does not unless `UNLUMINOUS_TELEMETRY=on`. `UNLUMINOUS_TELEMETRY=off` stops it for
  any window.

`unluminous-cli settings telemetry [--flush]` says whether it runs, why not, and what it has sent.

## Who holds the keyboard

`unluminous-cli status --section keyboard` answers it in four fields, which are four different
questions: `holder` is Unluminous's own focus, `textBox` is whether any text box has it, `node` is
which canvas node the keys are in, and `pane` is which editing pane.

The second is the one that explains most confusion: while **any** text box holds the focus, the
editing area, every terminal grid and every provider stand aside, deliberately, so that typing a
filter does not also type into the file behind it.

Two rules hold underneath. **A canvas that comes back has somewhere for the first key press to go**,
and **the keyboard goes to a surface that is on the screen** — if the editing area is not showing and
the canvas is, the canvas holds the keyboard. Where both are showing the editing area keeps it.

There is also a widget of Unluminous's own that holds the interface library's keyboard focus and
claims `Tab` and both pairs of arrows. Without it the focus walked
out of the document on the first arrow key and landed on the first button in the window, which is
`Close` — and a button that holds the focus is pressed by `Space`. That was reported as Unluminous
crashing while somebody typed, and there was nothing to find, because the window had been asked to
close in the ordinary way.
