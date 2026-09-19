# The window

[What it looks like](overview.md) is this page in pictures. This one names the parts.

## What is in it

A **title bar** Unluminous draws itself, holding the menus at the left, the project's name and its
branch after them, and at the right the text options, the three Markdown view modes, the run widget
and the window buttons. On macOS the menus are in the bar along the top of the screen instead and the
window buttons are at the left, where macOS puts them.

Down the far left a **rail**, one button per pane: the explorer and git at the top, and at the bottom
the things with a character grid in them. A button whose pane is showing is drawn as the same filled
pill every list in Unluminous uses for its chosen row. The rail is the only way a pane is put away and
brought back, because it is in the same place whether a pane is showing or not.

Then the **file explorer** with its filter box, a **tab** for each open file, the **gutter** of line
numbers, the **editing area**, whichever **tile** is showing along the bottom, and a **status bar**
naming the file, its kind, its line endings, the caret's position, the branch, how many files have
changed, and the font.

The window has no operating system frame — rounded corners and a translucent background need the
decorations turned off — so it draws its own eight resize grips, invisibly, and is dragged by its
title bar.

**Everything is painted at an absolute position** rather than through the interface library's layout,
because the measurements come from `design/intial-design-screenshot.png`. Run
`cargo run --example sample_design` to print the colour of each region of that image.

## The panels, and moving them

Five panels: the explorer, the terminal, the run tile, the debug tile and the Base of Infinite Space,
plus one for each pane a plugin contributes. Any of them is dragged by its header to the top, bottom,
left or right of the window, with four blue bands showing where it can go. The strong band is the
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

Two presses at the top of a panel maximise it, which is putting every other panel away rather than a
fifth kind of layout; `Escape` or two more presses put them back. `View -> Reset Panel Layout` is
always a way back.

Every panel's edge is a divider that is dragged to resize it, and a double click puts it back to its
usual size. Where a panel is and how large it is lives in **your** settings rather than the project's,
because where the panels are is a habit and the same in every project you open.

## Zooming

`Ctrl/Cmd` with `+` and `-`, or the wheel or a pinch with that modifier held. The gesture belongs to
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

## The nine menus

`Unluminous`, `File`, `Edit`, `Code`, `Find`, `View`, `Run`, `Git` and `Plugins`. Both menu bars are
built from one list, so they hold the same entries with the same shortcuts. Run `unluminous --print-menus`
to see it, or `unluminous-cli action list`, which is built by walking the same list.

| Menu | What is in it |
|---|---|
| `Unluminous` | About Unluminous, Check for Updates, Settings, Quit. |
| `File` | New Window, Open File, Open Web Address, Go to File, Open Folder, Recent Projects, Reopen Closed Tab, Save, Save As, Close Window. Opening a folder opens it in a window of its own. |
| `Edit` | Undo, Redo, Cut, Copy, Paste, Select All, a `Highlight` submenu holding the four colours and the two ways of clearing one, Navigate Back and Forward, Settings. |
| `Code` | Go to Definition, Find Usages, Rename, Complete Word, Reformat, comment and indentation, Go to Line, Go to Matching Bracket, and the line commands — duplicate, move, join and sort. Drawn for a file whose language can answer those questions. |
| `Find` | Find Action, Find, Replace, Find Next, Find Previous, Find in Files. |
| `View` | The three view modes, show or hide the explorer, the editing area and the line numbers, Maximise Pane, the font size, the tabs, Select Opened File, a `Split` submenu, a `Folding` submenu, the three tiles, the Base of Infinite Space and its own submenu, Reset Panel Layout, and the terminal tabs. |
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

**Enter presses the last button in the footer**, which is the one that does the thing and is filled in
the accent colour. A footer whose last button is dimmed is a modal there is nothing to confirm, so the
key press is left alone rather than doing nothing loudly. The commit panel is the one exception and
uses the command key with Enter, because its message box is a text area where Enter is a new line.

**A modal takes the keyboard from the panes behind it.** Without that, Enter in the delete
confirmation would delete the file *and* put a new line in the file behind it *and* open the row the
explorer's cursor was on.

## The Settings window

`Edit -> Settings`, `Unluminous -> Settings`, or command and comma. The pages are down the left under
their headings and the chosen page is on the right. The window is **one size for every page** and the
page scrolls inside it — a dialog that changed height as its list was walked would jump under the
pointer.

| Page | What is on it |
|---|---|
| `Appearance & Behavior -> Appearance` | the font the editor sets a document in, the font the window's own text is set in, the background opacity, and whether a plugin's pane is drawn with depth |
| `Appearance & Behavior -> Theme` | the theme, the accent and the icon set |
| `Editor -> Editor` | the line numbers, what one indent is, auto-indent, trimming, the completion popup, line endings, what search leaves out, and the version check |
| `Plugins` | the marketplace and what is installed |
| `Tools -> Terminal` | which program a terminal tab runs, the size it draws its grid at, and shell integration |
| `Tools -> MCP` | installing Unluminous into an agent, and the HTTP server |
| `Plugins -> <plugin>` | a page for each plugin that contributed one: Agent-Chat, Agent-Tasks and Database |

Changes take effect as they are made. `unluminous-cli settings list` prints all forty settings with a
sentence each, `settings get` reads one and `settings set` writes one.

### The font is one setting, and it reaches every tab

`appearance.font.family` and `.size` are one setting for the whole window, the way the reference editor has one
editor font. Changing it changes every file that is open, not only the one showing — the size is also
on the keyboard, on `View -> Reset Font Size`, and on a pinch or the wheel over the editing area, and
whichever of them is used it is the same setting, so it is still there next time.

**Setting it is not an edit.** Nothing goes onto any document's undo history and no file is marked as
having unsaved changes, because what Unluminous saves is plain text and carries no formatting.

`appearance.ui.font.*` is the window's own text — the menus, the rail, the explorer and the status
bar — and it is separate because a large document and a compact window is a reasonable thing to want.
Empty means the editor's family, so a settings file that names nothing is drawn exactly as it always
was.

### Themes

A theme says what every colour in Unluminous's own palette means, and one that names the nine token
colours also colours code, in every language at once. The list of names is closed: **a theme says what
a name means; it cannot add a name**, and a role Unluminous has not got is refused with the list.

`Themes Bundle 1` ships five, every number in them read out of the plugin jars of the reference editor they come
from: Islands Dracula Colorful, Material Palenight, Material Deep Ocean, Monokai Pro and One Dark.
Light is refused, with the reason written down rather than implied: the window is drawn on a
transparent ground, the depth recipe lifts a surface and shadows it with black, and 483 accepted
pictures are judged against a dark ground.

The **accent** is one colour over whatever the theme chose. The **icon set** is which drawn marks the
rail and the explorer's folder arrow use — `material` by default, or `classic` for the ones Unluminous
shipped with. `unluminous-cli theme list` says what there is and `theme set` paints the window in one.

### The window lets the desktop through

`appearance.background.opacity` is one slider from 0.05 to 1.0. The window is created transparent, the
background is painted with that alpha and every glyph is painted at alpha 255, so the desktop shows
through the background while the writing stays sharp. On macOS that is the whole story.

On Windows the same code drew a solid window, and three separate faults each had to be fixed — the
graphics backend, the swapchain and an uncleared surface.
[Architecture](architecture.md#the-window-lets-the-desktop-through) says what each one is.

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
