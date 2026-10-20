# task-2270: Unluminous issues v20

Twelve reports: eleven in the ticket and one Jason added while the work was under way. Each section says what
was reported, what the code does today, why that produces the report, and what changes. Where something was
measured, the numbers are given.

## 1. The ticket's agent receives only the end of its handoff

**Report.** *"Part of the message doesn't make it to the agent on agent tasks start work."* The picture shows the
first message in a ticket's chat starting mid sentence: *"at the end; and remove one that no longer applies with
`todo-remove task-40 <n>`..."*

**What happens.** `agent::handoff` is 1,283 bytes for `task-40`. `TicketTerminal::type_line` writes it into the
pseudoterminal in one write. Measured against the picture, the text shown starts at byte 1,022 of 1,283.

macOS keeps at most 1,024 bytes of input a program has not yet read in a pseudoterminal. When more arrives, the
kernel does not refuse it: it throws away everything already waiting and keeps what comes after. So a Mac agent
was handed the last 261 bytes. Windows' console accepts the whole line, which is why the transcripts on this
machine all hold the full handoff (four of them were checked, each 1,327 characters inside Claude Code's
`<pasted_content>` wrapper).

**Change.** `type_line` writes the line in pieces of at most 256 bytes (`terminal::PIECE`), 20 ms apart, from
`TicketTerminal::pump`, never splitting a character. When the program has asked for bracketed paste, the pieces
are wrapped in `ESC [200~` and `ESC [201~`, which is what every terminal does with text a person pastes, so the
program reads one paste however its reader splits the bytes. The return is still a separate write,
`SUBMIT_AFTER` after the last piece, for the reason `type_line` already records. A line asked for while pieces
are still going waits in the queue.

**Tests.** `pieces_of` is a pure function: the pieces add up to the line, none is over 256 bytes, none splits a
character, and the bracketed form opens and closes the paste.

## 2. The caret in a zoomed ticket description stands beside the wrong letter

**Report.** *"Task modal description. at some zoom levels the cursor isn't aligned with the text line."*

**What happens.** A zoomed modal draws through a transform, and `crisp::sharpen_the_text_in` lays every text
shape out again at the zoomed size so the letters are sharp. The text box measured its caret from the original
galley. A font's advances and line heights are rounded to whole pixels at each size, so a line laid out at
13 × 1.1 points is not 1.1 times the line laid out at 13. Measured in a test, the glyph the caret is placed after
was a whole point away at 1.1, and the error grows along a line and down a paragraph.

**Change.** `crisp::keep_the_original_places` moves every row and every glyph of the sharp layout to where the
original layout had it, times the zoom, and snaps each glyph's ink to a whole pixel. The bitmaps stay sharp; only
their positions follow the original. A galley whose rows or glyph counts do not pair up is left alone.

**Test.** `the_sharper_words_stay_where_the_caret_was_measured_from` lays a wrapped paragraph out at four zooms:
every glyph position matches the original exactly, and every glyph's ink is within one original point plus one
screen pixel. With the fix removed it fails at 1.1.

## 3. "task-23 created" under Add Task

**Report.** *"When I add a new task, a label shows up under Add Task button. I don't want that."*

**Change.** The board header drew `AgentTasks::message` in a strip under its row. That strip is removed. A
failure still reaches a person as a toast through `Request::Message`, and `plugins view agent-tasks` still
reports the message for an agent.

## 4. A File Editor node has no preview buttons

**Report.** *"File should have preview buttons for md files, etc. If i drag a file from folder explorer to realm,
I can't open it in file preview pane."*

**What happens.** A note node draws its Markdown file through `UnluminousApp::show_a_document_in`, which honours
the tab's view mode, and has the three view buttons in its header. A File Editor node calls `show_editor`
directly, which is the source and nothing else, and its header has no view buttons. A Markdown or Mermaid file
dropped onto a File Editor node, or a tab dragged out of a pane onto the canvas, could therefore never be
previewed.

**Change.**

- `realm_view::Framing` gains `file_view`: the view of the tab a File Editor node is showing, when that tab is a
  document whose kind has a preview (`file_kind::preview_applies`), and which kind of preview it is.
- The header draws the same three buttons a note has for it, named after the node, and reports the press as
  `NodeOutcome::view`.
- `act_on_a_node` sets the tab's own `view_mode` for a File Editor node, through `set_a_nodes_view`, which also
  serves a note. `realm note <node> view` reaches both kinds.
- `show_an_editor_node` draws a document tab through `show_a_document_in`, so the source, the side by side view
  and the preview are the editing area's own.

## 5. More than one web page at a time, and links open in a new tab

**Report.** *"we need to be able to show more than one window at a time. right now only one web page renders at
a time. Click on page links should open new tabs by default."*

**What happens.** `services/browser.rs` keeps one native view per window and points it at whichever placement is
chosen. Every other rendered tab or browser node draws a sentence, or in a pane the last picture of its page. The
rule rests on `task-1756`'s measurement: creating a second WebView2 controller while another lived hung the
window in a nested message pump. That measurement was taken while views were still created inside the egui pass.
`task-1756` itself then moved creation to `raw_input_hook`, before the pass, because creation inside the pass
re-entered it. The two-view case was not measured again after that move.

**Plan.** Measure it again with today's code before deciding.

1. `native::NativeHost` keeps a map of views by tab id instead of one `Option<NativeView>`. Each view has its own
   id (`unluminous-browser-<tab>`), which wry's macOS state map is keyed by. Every view is created in
   `reconcile`, which runs from `raw_input_hook`, and at most one is created a frame.
2. Every placement drawn this frame gets its own view, placed with its own bounds, zoom and crop. A tab that is
   not placed has its view hidden at WebView2's low memory target. Past `MOST_LIVE_VIEWS` (six), the view least
   recently placed is dropped and its tab draws its last picture until it is placed again.
3. Events are tagged with the tab the view belongs to, instead of the one tab the single view pointed at, so the
   `awaiting` and `pointed_at` bookkeeping for a view that changes page under a tab is no longer needed for
   moving between tabs.
4. "The tab showing" stays as one idea: the page that has the keyboard. The picker, the page's right click menu
   and the commands that act on "the tab showing" use it.
5. `UNLUMINOUS_BROWSER_VIEWS=one` keeps today's behaviour, so the two can be compared on the same build.

The measurement is a window with two editing panes each showing a rendered tab and three browser nodes on the
canvas, driven through `unluminous-cli`: open, switch, navigate, resize, zoom the canvas, close. A hang shows as
`status` timing out with requests queued.

**Measured on 2026-10-20**, release build, Windows 11, WebView2 runtime as installed, a window started with
`--background` and a settings folder of its own (`_agent_output/task-2270-browser-views/measure-views.ps1`):

| | five pages, a view each | `UNLUMINOUS_BROWSER_VIEWS=one` |
|---|---|---|
| any command timed out | no | no |
| slowest `status` | 133 ms | 54 ms |
| views | 5, every page `live` | 1 |
| window working set | 214 MB | 210 MB |
| WebView2 processes | 9 | 6 |

All five pages were drawn at once: both editing panes and the browser nodes, with a node behind another cut where
the front one covers it (`many2-all-pages.png`). The hang `task-1756` measured did not happen with views made
before the egui pass and one a frame.

The first run found a different fault, which is fixed: a browser toolbar narrower than its three buttons gave the
address well a negative width, and `rux` panicked in `Sprites::tidy` with `min > max ... max = -46.5`, which closed
the window. The toolbar now leaves the well out when there is no room for it.

**The link script was checked under jsdom** (`links-test.cjs`): a plain click on a link to another page calls
`window.open` and is cancelled; a click with a modifier, a link to a place on the same page, a `download` link and
a `javascript:` link are left to the page. A click inside a native page cannot be sent without bringing the window
to the front, which the project's own rule forbids, so the step from the engine's new window request onwards is
covered by `a_link_in_a_browser_nodes_page_opens_a_node_beside_it`.

**Links.** A view is built with an initialization script, unless `browser.links` is `same-tab` or the tab is a
design. On a plain left click on a link to another page (not a `#` link on the same page, not `javascript:`, not
`download`, no modifier), the script calls `window.open(href)` and cancels the click. That raises the view's own
new window request, which `with_new_window_req_handler` already turns into `BrowserEvent::OpenRequested`. The
script gives the page nothing to call: it uses the browser's own new window mechanism, so the rule that a page has
no host bridge stands. A request from a tab in a pane opens a new tab after it in the same pane; a request from a
browser node opens a new browser node beside it. `browser.links = new-tab | same-tab` is the setting, on
`Settings -> Editor -> Web pages`, with `new-tab` the default.

## 6. A long name runs past its row in the explorer

**Report.** *"text bleeds outside of the highlighted area"*, a long file name drawn past the right end of its
pill to the edge of the panel.

**Change.** A row's name is laid out with `TextWrapping::truncate_at_width` to the pill's right end less its
padding, and short of the unsaved dot when there is one, so a long name ends in an ellipsis inside the pill. The
whole name is still the row's accessible name.

## 7. The tab strip scrolls sideways with the wheel and a swipe

**Report.** *"I should be able to scroll tabs horizontally, without a scrollbar. e.g. scroll wheel when hovered
over a tab should move left to right, same with swiping on trackpad."*

**What happens.** `file_tabs::shift` worked out the strip's offset from the tab showing on every frame, so a tab
off the end could only be reached by showing it some other way.

**Change.** `file_tabs::scroll_the_strip` keeps the offset in egui's memory under the strip's pane. The wheel or a
swipe over the strip moves it, whichever way the larger part of the movement went, and the movement used is taken
out of the frame so the editor under the strip does not scroll too. When the tab showing changes, or the number of
tabs changes, the strip moves the least distance that shows the whole of the tab showing (`keep_in_view`). There
is no scrollbar.

## 8. New Browser Tab and Send to Realm on the tab menus

**Report.** *"Right click in top space or on existing tab -> open new browser tab. Right click on tab -> send to
realm so the file gets added as a new node in the current open realm."*

**Change.**

- `Action::NewBrowserTab` opens a rendered tab on `about:blank` in the pane that was right clicked, with its
  address field holding the keyboard. It is on the tab menu, on a new menu for the empty part of the strip, and on
  the `File` menu, so `action list` and `action run new-browser-tab` reach it.
- `Action::SendTabToRealm` moves the tab showing onto the realm showing as a node of its own: a rendered tab
  becomes a browser node on the same address, a picture, sound or video becomes the node that shows it, and
  anything else becomes a File Editor node holding the tab (the same as dragging the tab onto the canvas). The
  node is placed in the middle of the canvas's view and the Realm panel is shown if it was put away. It is absent
  while the Realm plugin is off, and dimmed on a tab that already lives on a node. It is on the tab menu and in
  `View -> Realm`.

## 9. Choosing the folder Agent Chat runs in, and "Other" for a ticket's project

**Report.** *"In settings, allow me to select the project folder for the directory the agent chat uses, similar
to agent tasks task modal. Also allow me to select "Other" and see a folder picker where I can select the
folder. Add that for agent tasks task modal as well."*

**What happens.** The chat's agent always starts in the window's open project (`AgentChat::project`, read by
`what_to_ask_the_agent`). There is no setting. The ticket modal's `Project` field is a `rux` select over the
ticket's folder, the window's project and the recent projects, with no way to choose any other folder. A plugin
cannot open the system folder picker itself: the window owns the picker, and `UnluminousApp::folder_picker` is
the seam a test replaces.

**Change.**

- `plugin_ui::Request::PickFolder { id, title, start }`. The window calls `folder_picker` and answers the provider
  through `UiProvider::answered` with `{ "folder": path }`, or `{ "folder": null }` when the person cancelled.
  This is the shape `Request::ClipboardPicture` already has.
- `components::folder_choice` is one control both places use. It is a `rux` select chip with a folder mark. The
  rows are the default (named with the folder it stands for), the window's project, the recent projects, the
  folder already chosen when it is none of those, and last `Other folder...`. Each row shows the folder's name
  and the useful end of its path. Choosing `Other folder...` opens the system picker, starting from the folder
  chosen now, and the folder picked becomes the choice. A cancelled picker leaves the choice as it was.
- Agent Chat: `Configuration::folder` (`folder` in `settings.conf`), empty for the window's project. It is the
  agent's working folder, what the system prompt names as the project, and what the welcome screen shows. It is a
  row on `Settings -> Agent-Chat` (in the card for an agent run as a program) and in the Chat settings dialog.
  `plugins run agent-chat folder [path|project]` reads and sets it, and `settings set plugins.agent-chat.folder`
  reaches it.
- Agent Tasks: the ticket modal's `Project` field uses `folder_choice`; `Other folder...` writes
  `Field::Project`. `plugins run agent-tasks project <key> [folder|default]` is the agent's half, which did not
  exist. The Settings page's `Default project` row uses the same control.

## 10. Opening nearly any file

**Report** (added while this ticket was being worked): *"files like md should be openable by unluminous, but when
I right click and open e.g. an md file I get error that it can't. ensure we can open nearly all file types. If not
renderable, just open a tab that says 'Unable to render this type of file'."* The picture is macOS's dialog:
*The document "instructions.md" could not be opened. Unluminous cannot open files in the "Markdown Document"
format.*

**What happens.** `Info.plist` declares Markdown, plain text, source and folders, so Finder offers Unluminous for
a Markdown file and sends it the open documents Apple event. winit's application delegate answers only
`applicationDidFinishLaunching:` and `applicationWillTerminate:`, so AppKit falls back to its document
controller, which has no document class for the type and shows that dialog. No file Finder hands Unluminous has
ever reached Unluminous's code. Inside the window, a file that is not text, or is too large, is refused with a
status bar line, and its explorer row does nothing on a click.

**Change.**

- `services::opened_by_the_system` (macOS). Before the event loop starts, Unluminous observes
  `NSApplicationWillFinishLaunchingNotification`, which AppKit posts after it has installed its own Apple event
  handlers, and installs a handler for the open documents event (`kAEOpenDocuments`). The handler reads each file
  address out of the event and queues its path. A window takes the queue every frame. Through Objective-C's
  runtime with `objc2`, which is already a dependency, and only on macOS.
- What a window does with a path: a folder opens in a window of its own (`open_a_project_window`). A file opens in
  a tab of this window. A window still waiting for a project (`task-2305`) opens the file the way
  `unluminous <file>` does instead, in a window on the file's folder, and closes itself.
- `Info.plist` gains a last document type for `public.item` and `public.data` as a viewer with rank `Alternate`,
  so Unluminous is offered for nearly any file without claiming any of them.
- The Windows installer: Unluminous is listed under `*\OpenWithProgids`, so it is in the `Open with` list of
  every file, and `Applications\unluminous.exe\SupportedTypes` is removed, because it limited `Choose another
  app` to the ten extensions it named. The right click verb for every file was already there.
- A file that exists but cannot be shown opens a tab anyway. `OpenFile::unrenderable` holds the reason; the
  editing area draws *"Unable to render this type of file"* with the file's name, its size and why, and two
  buttons: `Open in Default App` and `Reveal in File Manager`. Its explorer row opens it on a click like any other
  row. `tab open` answers with `"rendered": false` for it. Saving, editing and the text commands refuse it the
  way they refuse a picture.

## 11. What is deliberately not done

- No change to which file kinds Unluminous claims as the default handler on either platform.
- No page to page drag of tabs between a node and a pane beyond what already exists.
- The `about:blank` tab has no start page.
</content>
</invoke>
