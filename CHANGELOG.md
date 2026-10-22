# Changelog

What changed in each release of Unluminous, for somebody deciding whether to update.

Written by `tools/changelog.mjs` from the git history, and rewritten by `tools/release.ps1` and
`tools/release.sh` on every release, so it cannot fall behind. One line per ticket; the version
bumps the release script makes are left out, because they are the boundaries rather than the work.

## 1.8.2 — 2026-10-22

- Project tabs keep the window's size, the status bar branch switches branch, Realm links stay in their node (`task-2325`)
- A tab in the terminal is drawn as a space (`task-2325`)

## 1.8.1 — 2026-10-21

- A pull request template, the checks that enforce it, and a way to land one (`task-2326`)

## 1.8.0 — 2026-10-21

- Several projects in one window, a project tab each (`task-2322`)
- Pills shrink further and drop their mark, a closing tab answers what it held (`task-2322`)
- A pill keeps its folder mark unless it was shrunk (`task-2322`)
- Accept the pictures that carry both main's round modal corners and the project tabs (`task-2322`)

## 1.7.3 — 2026-10-21

- A message sent while a ticket's agent works is drawn where it was taken, a modal keeps the window's corners round, and Atrius 0.3.1 never reads a large file whole (`task-2323`)
- Accept the pictures where a modal now leaves the window's corners round (`task-2323`)
- The Sable memory test waits ten seconds for the wake models, which loading under the release's test load could exceed two (`task-2323`)

## 1.7.2 — 2026-10-21

- A run of tool calls is done when one failed, adds up its time, and zoomed text keeps the font's spacing (`task-2321`)
- Accept the four pictures the change moved (`task-2321`)

## 1.7.1 — 2026-10-21

- A node sent to the Realm is centred in the view and sized to fit it (`task-2320`)

## 1.7.0 — 2026-10-21

- Three crashes, the find bar's toggles and list, tab hints, and a browser tab's ground (`task-2320`)
- Record the rules in CLAUDE.md and the covered page comment (`task-2320`)
- Accept the two tab drag pictures, which no longer show the tab's name under the pointer (`task-2320`)
- Choose the microphone dictation uses (`task-2320`)
- Settings set and get know voice.microphone, and the Voice page test chooses a device (`task-2320`)
- The settings list test knows voice.microphone's help (`task-2320`)
- Send to Realm keeps the tab open, and a dragged browser tab is a browser node (`task-2320`)
- Clippy (`task-2320`)
- A restored node shows its own tab of a file the editing area also has open (`task-2320`)

## 1.6.0 — 2026-10-20

- Text editing gaps, terminal undo, modal zoom clip, input coordinates and CLI listing fixes (`task-2318`)
- More editing keys, window points everywhere, and CLI wording (`task-2318`)
- Window size in window points, area help, search follows the named window, and accepted pictures (`task-2318`)
- Clippy, test module last and no extra borrow of the painter (`task-2318`)

## 1.5.0 — 2026-10-20

- Menus and nodes over a page, the explorer's last rows, text area drags, a chat per project, and moving between windows (`task-2317`)
- The window walk excludes Next and Previous Window, the reference editor is not named, and a test writes atomically (`task-2317`)
- The skill gets its installed-by mark whatever line endings the checkout gave SKILL.md (`task-2317`)
- Accept the pictures with Next Window and Previous Window, the explorer list ending at its last row, and the description's placeholder a pixel to the right (`task-2317`)

## 1.4.0 — 2026-10-20

- Agents choose and call Unluminous's commands more accurately, measured on a 52 case eval (`task-2314`)
- Two checks use contains, which clippy asks for (`task-2314`)
- Rustfmt (`task-2314`)
- The refusal from showing a named file is boxed, which clippy asks for (`task-2314`)
- Accept the pictures where tab open now opens a tab of its own, and list batch and git diff with the client's commands (`task-2314`)

## 1.3.4 — 2026-10-20

- Unluminous issues v20 (`task-2270`)
- One if for a file handed over to a window waiting for a project (`task-2270`)
- Rustfmt (`task-2270`)
- The fresh install test reads the server's name, not a path that contains it (`task-2270`)

## 1.3.3 — 2026-10-19

- Chat links put the line at the top, the explorer follows them, ACP pins a version npm has, toasts redesigned (`task-2313`)
- The npm pins file is written atomically (`task-2313`)
- Accept the pictures where the explorer now centres the opened file (`task-2313`)

## 1.3.2 — 2026-10-19

- The ACP registry is cards with Add, Update and Uninstall, and a missing npx version runs the newest one npm has (`task-2308`)

## 1.3.1 — 2026-10-19

- The README sells Unluminous the way unluminous.com does, and the docs and pictures are of 1.2.2 (`task-2310`)
- A release refuses in seconds, retries what is transient, and carries on after a failure (`task-2312`)
- The README and the window guide name the keymap's presets on the line that says keymap, so the naming test passes on main again (`task-2312`)

## 1.3.0 — 2026-10-19

- Agents that speak the Agent Client Protocol, and the ACP registry (`task-2308`)
- The acp wire's comment does not name the reference editor (`task-2308`)
- The AI Configuration picture, whose shapes are a dropdown now that there are eight (`task-2308`)

## 1.2.2 — 2026-10-19

- The installers ship the speech model and ONNX Runtime, so dictation downloads nothing (`task-2306`)
- The speech model package step puts a bare CC aside, as the binary build already does (`task-2306`)

## 1.2.1 — 2026-10-19

- A window with no project waits for one in the Set Up dialog rather than opening the folder it stands in (`task-2305`)
- The waiting window test compares with a slice of the folder, for clippy (`task-2305`)

## 1.2.0 — 2026-10-19

- Pi as a fifth agent for Agent Chat, Agent Tasks, MCP and setup (`task-2302`)
- The Pi decoder's first message_start is one match arm, for clippy (`task-2302`)
- Fixes the live Pi runs found (`task-2302`)
- The window suite expects Pi among the endpoints, and the pictures with its pill and its Install button (`task-2302`)
- Set Up Unluminous has a Pi row, and unticking Connect agents in the installer declines Pi too (`task-2302`)
- Cargo fmt (`task-2302`)
- Setup's plan has a Pi step after the rebase onto task-2301 (`task-2302`)
- The Voice page's Download button has room, so opening it with no model no longer closes the window, and a failed model download is said in its dialog (`task-2302`)
- A new ticket shows its todos and comments under its description (`task-2302`)
- The Voice page test measures its one Download button without a loop, for clippy (`task-2302`)
- CLAUDE.md says what the Voice page, the speech model dialog and a new ticket changed (`task-2302`)

## 1.1.0 — 2026-10-19

- The TDD for a Set Up Unluminous dialog that finishes the install from the window (`task-2301`)
- The Set Up Unluminous dialog finishes setup from inside the window (`task-2301`)
- A background window never opens Set Up Unluminous by itself, and a test of it opening (`task-2301`)
- The open by itself test uses a second window of its own (`task-2301`)
- Clippy (`task-2301`)
- The Keymap page lists Unluminous -> Set Up Unluminous... (`task-2301`)

## 1.0.19 — 2026-10-18

- A panel let go where the blue rectangle shows lands there (`task-2298`)

## 0.99.2 — 2026-10-18

- A release that does not have to be run twice (`task-2295`)
- CLAUDE.md says what a ticket needs to know about the release now (`task-2295`)

## 0.99.1 — 2026-10-18

- Unluminous Issues v18 (`task-2294`)
- A shorter realm folder summary, under the MCP schema ceiling (`task-2294`)
- The changelog (`task-2294`)

## 0.99.0 — 2026-10-18

- Unluminous Issues v18 (`task-2293`)
- Rustfmt on the ticket's own code, which the release's format check asks for (`task-2293`)
- The changelog (`task-2293`)
- The release starts the macOS build without CC, as it runs every other build (`task-2293`)

## 0.98.3 — 2026-10-17

- Realm ports zoom with the canvas, glass fixes, a sized switch knob, and nineteen 4K wallpapers (`task-2291`)
- The glass theme's picture is the Milky Way core, since glass over the paper stripes could not be read (`task-2291`)
- The accepted pictures: the new default and glass wallpapers, hidden shape points and the sized switch knob (`task-2291`)
- The changelog (`task-2291`)

## 0.98.2 — 2026-10-17

- Switching always listen off releases the speech model at once, and voice status says what is loaded (`task-2290`)
- The code for releasing everything Sable loaded when always listen is switched off (`task-2290`)
- The changelog (`task-2290`)

## 0.98.1 — 2026-10-17

- Sable requests keep their words and send, a second press sends, Alt+V, one listening window (`task-2290`)
- The View menu picture with Dictate on Alt+V (`task-2290`)
- The changelog (`task-2290`)

## 0.98.0 — 2026-10-17

- Repo notes on releasing from a worktree and on timing the window (`task-2288`)
- Speech to text in the Agent-Chat prompt (`task-2290`)
- Clippy, the model state read when asked, ticket chats keep no microphone (`task-2290`)
- The changelog (`task-2290`)

## 0.97.0 — 2026-10-17

- A smoothness bench under load, and a frame trace that says when each frame ended, how long its thread ran and on which kind of core (`task-2288`)
- The window's thread runs first on a busy machine, and rux draws at the processor's own SIMD level (`task-2288`)
- A modal's backdrop is the previous frame blurred on the GPU, on the modal's first frame (`task-2288`)
- Rux layers, plugin panes and Realm nodes drawn as meshes, with every shadow a cached sprite (`task-2288`)
- The smoothness rules and the TDD with every round's numbers (`task-2288`)
- The backdrop's blur is asked for on every frame a modal is open and made once, so a frame that is never painted cannot lose it (`task-2288`)
- The changelog (`task-2288`)

## 0.96.1 — 2026-10-17

- Words centred in chips, glass explorer and badges, minus before plus, American spelling (`task-2287`)
- Close the keymap test the rebase joined to the ink test (`task-2287`)
- The Settings pictures again, with the Keymap row and American spelling (`task-2287`)
- The changelog (`task-2287`)

## 0.96.0 — 2026-10-17

- Settings -> Keymap, with VS Code and IntelliJ presets (`task-2286`)
- IntelliJ's Expand All is left out, because Ctrl+Shift+Plus is Ctrl+Plus (`task-2286`)
- Rustfmt the keymap (`task-2286`)
- The changelog for the keymap (`task-2286`)
- The update check test says how long status took when it fails (`task-2286`)

## 0.95.1 — 2026-10-17

- An agent picks its conversation back up after a restart (`task-2285`)
- The scripted agent checks its arguments with a loop rather than findstr (`task-2285`)
- The Codex resume window test covers a ticket with no session and one whose session cannot be found (`task-2285`)
- A window suite receipt per binary and commit, so tickets running the suite at once keep their own (`task-2285`)
- Rustfmt the Codex resume window test (`task-2285`)
- The changelog for restoring agent conversations (`task-2285`)

## 0.95.0 — 2026-10-16

- Whiteboard shape nodes on the Realm (`task-2284`)
- Raise shapes with the same three shadows and lit edge as every node (`task-2284`)
- Shapes are glass under Unluminous Glass (`task-2284`)
- The glass cover's black through from_black_alpha (`task-2284`)
- The changelog for the whiteboard shapes (`task-2284`)

## 0.94.1 — 2026-10-16

- A Realm node's ports show while the pointer is within 150 points of them (`task-2282`)
- The changelog written from the history (`task-2282`)
- The code index rows have names short enough to sit beside their lists (`task-2282`)
- Settings help from the card's margin, plugin file icons clear of the name, Run and the branch centred, and the rail's buttons centred (`task-2282`)
- Words sit in the middle of a text box's caret and of a menu row, and Directory shows the full path (`task-2282`)
- The window pictures with the rail centred, words centred in their rows, and settings help from the card's margin (`task-2282`)
- The changelog written from the history (`task-2282`)

## 0.94.0 — 2026-10-16

- No square edge outside the window's corners, the macOS mark opens its menu, and button words stay inside their buttons (`task-2282`)
- The Designer has Browse and Comment, and no Select (`task-2282`)
- Settings for how the code index loads, with the current defaults and what each costs (`task-2282`)
- A Realm node's ports show only while the pointer is within 100 points of them (`task-2282`)
- Button words are cut only near the button's edge, and settings list knows the new code index help (`task-2282`)
- The debug tile's Copy command is centred in its pill (`task-2282`)
- The symbol_cost example opens the index with the new file contents argument (`task-2282`)
- The changelog written from the history (`task-2282`)

## 0.93.0 — 2026-10-16

- The bars sit on the window's glass with no panel, panes use rux's capped Chrome::panel, and rux draws glass edges at the display's resolution and cuts glass to its clips (`task-2281`)
- The glass pictures with the bars on the window's glass and the edges at full resolution (`task-2281`)
- Rux with glass corners that are a true curve and the glass fading just inside its edge (`task-2281`)
- No dark ground in the editing area under glass, a popup fading in is glazed as it fades, and the board's sideways bar can be dragged (`task-2281`)
- Under glass, pane headers, the tiles and the file tabs sit inside their panel, and the open file's tab is glass (`task-2281`)
- The glass pictures with no editor ground, the glass file tab and the lower pane headers (`task-2281`)
- The changelog written from the history (`task-2281`)

## 0.92.0 — 2026-10-16

- The window is one sheet of glass, every docked panel is a frosted glass pane on it, and the glass edges are faint (`task-2281`)
- Rustfmt over the files the second glass pass changed (`task-2281`)
- Every pane, the editing area and both bars are glass panels drawn the way a card is, with no light line along any edge (`task-2281`)
- A drag draws no frames and a modal opens complete in one frame under glass; the rim detector and the speed bench are in tools (`task-2281`)
- The glass pictures with the panes and bars as glass panels and Settings over the frosted picture (`task-2281`)
- The changelog written from the history (`task-2281`)

## 0.91.0 — 2026-10-16

- Modals fill the window and remember where they were, the interface size works, selection draws when zoomed (`task-2280`)
- Every text box selects the run under a double click; tests follow the editor font to the Editor page (`task-2280`)
- The double click interval is given to egui on the first frame; snapshots for the filled modals, the moved font settings and the labelled drop targets (`task-2280`)
- The glass pictures at the design's interface size, with the Settings modal filling the window (`task-2280`)
- Svg_icon compares a path command with eq_ignore_ascii_case, which clippy asks for (`task-2280`)
- Run set cut to one sentence and one example so the MCP schema stays under its ceiling (`task-2280`)
- Rustfmt over the files this ticket changed, which the release's format check asks for (`task-2280`)
- The project state test waits for the shared writer; repo-plan says to run cargo fmt before committing (`task-2280`)
- The changelog written from the history (`task-2280`)

## 0.90.0 — 2026-10-16

- Unluminous Glass, a theme whose surfaces bend the picture behind the window (`task-2281`)
- The glass pictures with the editing area's second ground (`task-2281`)
- The changelog written from the history (`task-2281`)

## 0.89.0 — 2026-10-15

- Language servers and the embedding model are off by default and stop the moment they are switched off (`task-2279`)
- A last-chance save and a read of the project state wait for the writer thread (`task-2279`)
- The writer's job boxes the state it carries, and rustfmt over the three files this ticket wrote (`task-2279`)
- The window holds 149 MB with this repository open, down from 259 (`task-2279`)
- The modal backdrop is taken again when the background picture arrives or changes (`task-2279`)
- The release build's memory, measured (`task-2279`)
- The changelog written from the history (`task-2279`)

## 0.88.0 — 2026-10-15

- An install finishes setting Unluminous up with unluminous-cli setup (`task-2278`)
- The installer README says what setup does and how a fresh install is tested (`task-2278`)
- Setup is answered by the client, like mcp install, so the window walk lists why it never succeeds there (`task-2278`)
- The changelog written from the history (`task-2278`)

## 0.87.2 — 2026-10-15

- A long chat no longer stops the ticket modal, and eight smaller fixes (`task-2277`)
- The tab name test clears the texture changes its frame made (`task-2277`)
- A status chosen in a zoomed ticket modal is the status, and a row left as space keeps its spacing (`task-2277`)
- Rustfmt's layout for the chat, the ticket modal, the tabs and the sequence diagram (`task-2277`)
- The changelog written from the history (`task-2277`)

## 0.87.1 — 2026-10-15

- A report draft that goes away unsent takes its folder with it (`task-2276`)
- The changelog written from the history (`task-2276`)

## 0.87.0 — 2026-10-15

- Report an issue from the window, and every modal in the redesign (`task-2276`)
- The report's temporary log and description are allowed plain writes (`task-2276`)
- The git walk reads the repository's history rather than racing the open file's (`task-2276`)
- Rustfmt's layout for the report and the restyled modals (`task-2276`)
- The changelog written from the history (`task-2276`)

## 0.86.1 — 2026-10-15

- The telemetry tests remove their scratch folders when they end (`task-2271`)
- The window reads git again when the repository changes outside it (`task-2274`)
- Git is read every 10 s while the window has the focus, not every 5 s (`task-2274`)
- A test asking git status through run_command_line waits for the read (`task-2274`)
- The changelog written from the history (`task-2274`)

## 0.86.0 — 2026-10-14

- Anonymous usage statistics sent to Google Analytics, with a switch in Settings (`task-2271`)
- Telemetry writes its files through store::write_atomically, and a clippy fix (`task-2271`)
- The title bar and the application icons are the blue U again, still (`task-2271`)
- The changelog written from the history (`task-2271`)

## 0.85.0 — 2026-10-14

- Chat selection, a ticket's long chat, comment order, To Side Pane, scroll after a tab closes (`task-2269`)
- The ticket modal's Description and Comments fold, as Agent and Todos do (`task-2269`)
- Rustfmt (`task-2269`)
- The changelog written from the history (`task-2269`)

## 0.84.7 — 2026-10-14

- Comment & Send to Agent starts the agent on a ticket the watchdog moved back to New (`task-2268`)
- The debugger's stopped line is centred on its letters, and its inline values sit on the code's baseline (`task-2268`)
- A message echoed in a ticket's terminal is no longer read as a menu, and a menu's buttons wrap instead of overlapping (`task-2268`)
- Inillucent 2.0.5, which keeps the ONNX Runtime environment alive to the end of the process (`task-2268`)
- The changelog written from the history (`task-2268`)

## 0.84.6 — 2026-10-14

- A macOS build made on Windows is signed with its entitlements, and a release from a worktree publishes the site and the public source (`task-2268`)
- The changelog written from the history (`task-2268`)

## 0.84.5 — 2026-10-14

- MacOS clippy is clean, and the odd length test runs on Windows (`task-2268`)
- On macOS a project with a browser tab and saved cookies no longer closes on start, and the code index loads ONNX Runtime (pull request #3) (`task-2268`)

## 0.84.4 — 2026-10-14

- The Arches U logo, still, in a light and a dark version (`task-2258`)
- The changelog written from the history (`task-2258`)

## 0.84.3 — 2026-10-13

- The sentence in a browser pane that is not live is wrapped and clipped to the pane (`task-2263`)
- The changelog written from the history (`task-2263`)

## 0.84.2 — 2026-10-13

- A page's picture keeps its shape, and the notice for a ticket that was not created has a test and a picture (`task-2263`)
- The changelog written from the history (`task-2263`)

## 0.84.1 — 2026-10-13

- A click on the Designer's page keeps the view there, and a page's picture is taken only once it has loaded (`task-2263`)
- The changelog written from the history (`task-2263`)

## 0.84.0 — 2026-10-13

- Trusted ticket folders, menu buttons in the ticket chat, a title that is required, todos under the agent, link menus, and the browser view that stays put (`task-2263`)
- A press on a page gives the view to that page, the tests and pictures for the new ticket layout, and the rules in CLAUDE.md (`task-2263`)
- The changelog written from the history (`task-2263`)

## 0.83.0 — 2026-10-13

- The ticket's agent as a chat, a folding fields card, icon buttons on comments, todos ticked as agents go (`task-2261`)
- The changelog written from the history (`task-2261`)

## 0.82.0 — 2026-10-13

- The ticket modal's comment box holds three lines and its buttons can be pressed (`task-2260`)
- The Designer pane has its own tabs, starting blank and remembered (`task-2260`)
- A refused tab is checked by its code (`task-2260`)
- Tab open names the Designer tab, and the empty terminal strip is whole or absent (`task-2260`)
- The code formatted (`task-2260`)
- The changelog written from the history (`task-2260`)

## 0.81.1 — 2026-10-13

- The mark at the top left of the macOS bar too (`task-2258`)
- Rustfmt the macOS mark test (`task-2258`)
- The changelog written from the history (`task-2258`)
- Two database pictures allow for the query's run time in their result line (`task-2258`)
- Max_failed_pixels, the name egui_kittest gives it now (`task-2258`)
- The changelog written from the history (`task-2258`)

## 0.81.0 — 2026-10-13

- The Unluminous mark, animated in the title bar and still in the icons (`task-2258`)
- Rustfmt the logo, the menu bar and the setting's command line arm (`task-2258`)
- The changelog written from the history (`task-2258`)

## 0.80.0 — 2026-10-12

- Idle CPU, window drag and resize behind a modal, resizable ticket sections, panes kept across a restart (`task-2255`)
- Browser tabs, links, selection, menus, designs and tool calls (`task-2255`)
- Tests and notes for the browser, link, design and ticket changes (`task-2255`)
- Catalogue text cut to a sentence and the schema ceiling recorded (`task-2255`)
- Window tests follow the new toolbar, folded tool calls and link clicks (`task-2255`)
- The changelog written from the history (`task-2255`)

## 0.79.0 — 2026-10-12

- The Designer plugin, for designing in HTML with Agent-Chat (`task-2254`)
- The changelog written from the history (`task-2254`)

## 0.78.0 — 2026-10-12

- A console SELECT in the table grid, a smooth browser zoom, sessions, and Agent-Tasks fixes (`task-2253`)
- A window that is not on the screen keeps answering, and the format and lint fixes (`task-2253`)
- The changelog written from the history (`task-2253`)

## 0.77.0 — 2026-10-11

- TDD for an optional code index and half the memory when it is on (`task-2249`)
- The Database pane and the Realm in the Settings design's language (`task-2250`)
- The changelog written from the history (`task-2250`)

## 0.76.1 — 2026-10-11

- The embedding model is let go a minute after its last use (`task-2247`)
- The changelog written from the history (`task-2247`)

## 0.76.0 — 2026-10-11

- The code index stops taking 12 GB, AI Configuration, OpenCode, modals, scrollbars and the realm divider (`task-2247`)
- Ai.provider settings written without assigning inside Ok, which clippy refuses (`task-2247`)
- Settings list's seam test knows the AI endpoint rows' help (`task-2247`)
- The changelog written from the history (`task-2247`)

## 0.75.0 — 2026-10-11

- An inventory of every setting the Settings modal shows, for the designer (`task-2244`)
- The settings inventory as one file per screen, each short enough for one canvas comment (`task-2244`)
- The Settings window is the Settings design, every page drawn with a shared settings kit (`task-2246`)
- The settings pages formatted the way cargo fmt writes them (`task-2246`)
- The changelog written from the history (`task-2246`)

## 0.74.0 — 2026-10-11

- The release speed design records the 0.73.1 release and the after numbers (`task-2240`)
- Agent-Tasks drawn to the approved design, the board and the ticket (`task-2241`)
- The changelog lists the Agent-Tasks redesign as unreleased (`task-2241`)
- The title bar, the menus and the rail drawn to the Editing code design (`task-2241`)
- The changelog lists the window chrome redesign as unreleased (`task-2241`)
- The pictures a test draws in a second window on the same thread, taken with the chrome state tied to its context (`task-2241`)
- The renamed terminal tab picture without the index's fence file in the sample folder (`task-2241`)

## 0.73.1 — 2026-10-10

- Release on 80% of the processors, test only the crates that changed, build both platforms at once (`task-2240`)
- Release.sh uses the same gate, tests for the selection, and the design (`task-2240`)
- Line tables only in test builds, and the window suite measurements (`task-2240`)
- The changelog lists the release speed work as unreleased (`task-2240`)

## 0.73.0 — 2026-10-10

- A learned completion ranking, trained on the tune half of the evaluation (`task-2237`)
- The model's trees are statics, not consts clippy says may be copied at each use (`task-2237`)
- The trainer's __pycache__ is not part of the repository (`task-2237`)
- The held gate and the controls in SCORECARD.md, and what was built in the TDD (`task-2237`)
- The first letter's case is a fact the ranking reads (`task-2237`)
- The shipped build on the held half beside the gated one in SCORECARD.md (`task-2237`)
- The changelog lists the learned completion ranking as unreleased (`task-2237`)
- A comment names the reference editor as the naming rule asks (`task-2237`)

## 0.72.0 — 2026-10-10

- Every Agent-Chat component and message part drawn to the design (`task-2235`)
- The changelog lists the Agent-Chat redesign of every component as unreleased (`task-2235`)
- The scrolled chat settings picture without a code index folder a run left in the sample (`task-2235`)

## 0.71.0 — 2026-10-10

- Completion with structure and language servers, measured against the reference editor (`task-2232`)
- The held out gate and its scorecard, with the latency reading for G4 (`task-2232`)
- Unluminous-lsp takes the workspace version after the rebase onto 0.70.0 (`task-2232`)
- The changelog lists the completion work as unreleased (`task-2232`)
- Rustfmt on the files the tuning changed (`task-2232`)
- Clippy's sort_by_key in the rust-analyzer toolchain search (`task-2232`)

## 0.70.0 — 2026-10-10

- TDD for completion equal to IntelliJ, Rust and TypeScript first (`task-2231`)
- Agent-Chat drawn to the approved design, in dark and light (`task-2235`)
- The changelog lists the Agent-Chat redesign as unreleased (`task-2235`)

## 0.69.1 — 2026-10-09

- Claude Design starting point for Unluminous: the rux design system and a canvas of the window (`task-2230`)
- A Rust kernel is stopped with its whole process tree when the bridge shuts down (`task-2229`)
- The changelog lists the Rust kernel shutdown fix as unreleased (`task-2229`)
- Rustfmt export_icons_svg.rs from task-2230, which the release's format check refuses unformatted (`task-2229`)
- The changelog lists the formatting commit as unreleased (`task-2229`)
- Name the icon drawing type in export_icons_svg.rs, which clippy refused as too complex (`task-2229`)
- The changelog lists the clippy fix as unreleased (`task-2229`)

## 0.69.0 — 2026-10-09

- Rust notebooks, completion from the kernel, and the kernel picker fix (`task-2229`)
- Rustfmt the notebook completion and Rust kernel changes (`task-2229`)
- The changelog lists Rust notebooks and notebook completion as unreleased (`task-2229`)

## 0.68.1 — 2026-10-09

- Action run answers with a message only when the action set one (`task-2220`)
- The changelog lists the action run answer as unreleased (`task-2220`)

## 0.68.0 — 2026-10-09

- Review fixes: # %% lines stay in their cell, the debugger lets the kernel go, Restart and Run All runs, undo keeps outputs, nothing waits on Python in a frame (`task-2220`)
- The notebook rows the review found missing: sections, tags, dragging a cell, the outline, ?name, and the output scroll setting (`task-2220`)
- Commands for what only a person could do to a notebook (`task-2220`)
- Window and kernel tests for each review finding, and a release that must start a kernel (`task-2220`)
- CLAUDE.md and the PRD say how markers, debugging and the new commands work (`task-2220`)
- Accept the pictures with the notebook icon, the drag handle and the Notebooks settings section (`task-2220`)
- Split the long notebook functions, keep the tab in its file while painting, and share the cell lookup (`task-2220`)
- A restart waits for both kernel channels, and notebook status reports tags, hidden cells and the kernel's process (`task-2220`)
- The changelog lists the review fixes as unreleased (`task-2220`)
- The debug tests wait for the stack to be read before asking where the debugger stopped (`task-2220`)
- The changelog lists the debug test fix as unreleased (`task-2220`)

## 0.67.0 — 2026-10-09

- Jupyter notebooks: cells, outputs, a kernel through Python, debugging a cell, and the notebook commands (`task-2220`)
- Accept the menus with New Jupyter Notebook, and mark a picture's untinted white (`task-2220`)
- The changelog lists notebooks as unreleased (`task-2220`)
- A kernel that is alive and slow to start is waited for up to three minutes (`task-2220`)
- The changelog lists the longer kernel start as unreleased (`task-2220`)

## 0.66.4 — 2026-10-08

- Stop writing to disk on every frame, keep tab layouts, and shrink the graphics device (`task-2218`)
- Lay a line out with one copy, and hand layout a run's ASCII advances at once (`task-2218`)
- The TDD with the measurements, two window tests, and the rules in CLAUDE.md (`task-2218`)
- The changelog lists the performance work as unreleased (`task-2218`)

## 0.66.3 — 2026-10-08

- Crisper depth, calmer chosen states, and the chat that follows the theme (`task-2219`)
- Accept the pictures of the new depth, rail, title bar and chat (`task-2219`)
- The changelog lists the design pass as unreleased (`task-2219`)

## 0.66.2 — 2026-10-08

- Text in a zoomed node or modal is drawn one texel to one pixel (`task-2216`)
- Format the window pixel test (`task-2216`)
- The changelog lists sharper zoomed text as unreleased (`task-2216`)

## 0.66.1 — 2026-10-08

- The light theme's comments are a step darker, so they read over a picture (`task-2215`)
- The changelog lists the darker light comments as unreleased (`task-2215`)

## 0.66.0 — 2026-10-08

- Unluminous Light, and every surface follows a light or a dark theme (`task-2215`)
- Rustfmt, one unused import, and a colour marker may sit on the line above (`task-2215`)
- The changelog lists the light theme as unreleased (`task-2215`)

## 0.65.1 — 2026-10-08

- Agent Tasks holds together when zoomed, and the ticket modal is easier to work in (`task-2214`)
- The ticket modal's sections share its height by what they hold (`task-2214`)
- The changelog lists the Agent Tasks changes as unreleased (`task-2214`)

## 0.65.0 — 2026-10-07

- Agent chat answers can hold components, drawn as instruments (`task-2211`)
- A press on the welcome or a component chooses its chat node, and their ids are the conversation's (`task-2211`)
- The gallery tests are laid out the way rustfmt lays them out (`task-2211`)
- The component code passes clippy with warnings denied (`task-2211`)
- The changelog lists the components as unreleased (`task-2211`)

## 0.64.5 — 2026-10-07

- A release from a ticket worktree copies notarize.env and fetches the cross toolchain, or it has no macOS half (`task-2207`)
- The changelog lists 0.64.4 as released (`task-2207`)

## 0.64.4 — 2026-10-07

- Realm pages respect the stacking order and stay in the canvas, a playing video keeps playing, Apple Lossless plays, and Atrius 0.1.2 (`task-2207`)
- A sound file's decoder type has a name, so the signature fits the line rustfmt allows (`task-2207`)
- The m4a test imports nothing it does not use, and the changelog lists this ticket (`task-2207`)

## 0.64.3 — 2026-10-07

- The chat prompt's text and caret stay inside its field (`task-2200`)
- Atrius 0.1.1, whose store thread survives a store it cannot open and a job that panics, and which answers a question from its words when the passages cannot (`task-2200`)
- A media file copied into a realm is written through a temporary, which the persisted write check asks for (`task-2200`)

## 0.64.2 — 2026-10-07

- A realm tab that is showing is the canvas, so asking for the canvas does not open the panel beside it (`task-2202`)
- The changelog written out from the history (`task-2202`)
- The note test checks the note without a picture, which differed with frame timing under load (`task-2202`)

## 0.64.1 — 2026-10-07

- A picture, sound or video from outside the project is copied into the realm, and a .realm file opens in a tab (`task-2202`)
- Keep a node's keyboard only when the realm is in a tab, and let the sound test wait for its frame (`task-2202`)
- Browser sessions survive a restart, and a browser node can show one element (`task-2203`)
- The sound test plays thirty seconds, so a slow frame cannot reach the end of it (`task-2202`)
- The changelog written out from the history (`task-2203`)
- The changelog written out from the history (`task-2202`)
- This repository's canvas, imported from .unluminous/space.conf into realm files (`task-2202`)
- A file copied into a realm is written atomically (`task-2202`)
- The note picture is taken after the explorer is read again, so its file count is the same every run (`task-2202`)
- The realm's player summaries are one sentence each, so the grouped MCP schema is under its ceiling again (`task-2202`)

## 0.64.0 — 2026-10-07

- Realm stacking order, agent chat sizing, tooltips and the gutter's marks (`task-2200`)
- The sound node test plays a thirty second clip, so a busy test binary cannot let it end before the test looks for Pause (`task-2200`)
- The ticket modal's comments are whole and scroll, and a pane no longer follows the editor's font (`task-2200`)

## 0.63.1 — 2026-10-07

- Browser sessions survive a restart, and a browser node can show one element (`task-2203`)
- The changelog written out from the history (`task-2203`)

## 0.63.0 — 2026-10-07

- Realm TDD, the Base of Infinite Space as .realm files in the project (`task-2199`)
- The Realm, a canvas kept in .realm files in the project (`task-2202`)
- The pictures the realm changes, accepted after looking at them (`task-2202`)
- Clippy, and the bundled plugin test allows a ui plugin's own file type (`task-2202`)
- Cargo fmt (`task-2202`)
- The realm tests' project folders have fixed names, so their pictures are the same on every run (`task-2202`)
- The changelog written out from the history (`task-2202`)

## 0.62.0 — 2026-10-06

- Unluminous issues v5 (`task-2198`)
- Rustfmt the new code (`task-2198`)
- Clippy (`task-2198`)
- The changelog written out from the history (`task-2198`)
- Wait up to an hour for Apple's notary (`task-2198`)

## 0.61.0 — 2026-10-05

- Mermaid edges, double click, folder pane, file icons, macOS edge resize (`task-2194`)
- The changelog written out from the history (`task-2194`)

## 0.60.0 — 2026-10-05

- Agent-Tasks on Inillucent, a rux ticket modal, and the chat and terminal fixes (`task-2193`)
- Cargo fmt (`task-2193`)
- The changelog written out from the history (`task-2193`)

## 0.59.0 — 2026-10-03

- Compare the index with git grep, ugrep, ugrep's index and Zoekt, and publish the comparison (`task-2139`)
- The code index is Atrius, from the atrius-index crate (`task-2172`)
- The changelog written out from the history (`task-2172`)

## 0.58.0 — 2026-10-01

- Publish unluminous.com and blackrainbowlabs.com from the black-rainbow-labs-sites repository (`task-2128`)
- The task-2138 code index design, copied in as the plan this ticket implements (`task-2139`)
- Revise the code index design for Inillucent 2.0.2, Claude Code's real Grep flags and the Labs page (`task-2139`)
- Move to inillucent-driver 2.0.2 from crates.io, with the embed feature (`task-2139`)
- The unluminous-index crate's exact engine and host, the search area, and the evaluation harness (`task-2139`)
- Read a plugin manifest's language half in unluminous-core (`task-2139`)
- Freeze the code index query sets before the first measured run (`task-2139`)
- Keep git from converting the frozen query sets' line endings, which their hashes cover (`task-2139`)
- Index files in blocks, fix a planner fault that dropped files, keep the host off the caller's pipes (`task-2139`)
- Mcp serve takes --areas in its catalogue row, which the server already read (`task-2139`)
- Symbols, chunks, passage search and the def, refs, outline and fragment verbs (`task-2139`)
- Keep the agent rubric's bytes stable too, since agent-manifest.json hashes them (`task-2139`)
- Compact the index off the lock, and never hold a lock while saving (`task-2139`)
- Refs keeps the lines that define a name, first in their file (`task-2139`)
- Freeze the agent level rubric before the first agent run (sha256 da370103) (`task-2139`)
- A grouped tool takes a command's keys from beside command, and a search-only agent gets one tool (`task-2139`)
- An agent is shown a search's text, not its JSON (`task-2139`)
- Outline takes --structured too; files had it twice (`task-2139`)
- Vectors in the passage table, embedded in the background, cached by chunk hash (`task-2139`)
- A renamed copy of the CLI starts its index host from itself (`task-2139`)
- Build the passage table from a copy, so a rebuild never holds the index lock (`task-2139`)
- Embed a batch with no transaction open and write it in one, rather than two commits a chunk (`task-2139`)
- A meaning run waits for vectors and records the top hit's confidence, so the cutoff can be chosen on dev (`task-2139`)
- Every search verb is driven from the window, and a folder that is not there is refused (`task-2139`)
- Passage search turns an answer away by word coverage at 0.30, with or without vectors (`task-2139`)
- A refusal names the keys taken out of the call, and files inside a folder matches its whole path when the folder's own name is asked (`task-2139`)
- The passage table's fusion is an evaluation setting and part of its version (`task-2139`)
- The index arm asks the cache folder the run names (`task-2139`)
- Passage search puts code and tests before documents, and embeds the question once (`task-2139`)
- Small answers carry the code around their hit, passage search ranks code before documents, cutoff 0.31 (`task-2139`)
- The vector waiter does not stop on a host that has only just started (`task-2139`)
- A short files answer carries the best file's outline, and the search note says to read a range (`task-2139`)
- Def takes a qualified name such as Shell::open, the type's own definition first (`task-2139`)
- A plain English answer shows five chunks and the best one's code, rather than ten (`task-2139`)
- A host does not embed a repository too large for the processor, and UNLUMINOUS_EMBED=off stops it embedding at all (`task-2139`)
- A path named by its last folders is read as the one folder it names, and code comes before documents in an exact answer (`task-2139`)
- An agent study scenario for code search (`task-2139`)
- CLAUDE.md records the code index's rules and what the measurement says (`task-2139`)
- The held out scorecard: G1 passes, G2 is not reachable by construction, G3 is not met (`task-2139`)
- Cargo fmt and clippy on the index and search code; the host lock no longer needs Rust 1.89 (`task-2139`)
- The changelog written out from the history (`task-2139`)
- The trigram index's comment names the reference editor, as shipped text must (`task-2139`)

## 0.57.1 — 2026-09-24

- A terminal test that finds no tab says whether the shell failed to start or stopped (`task-2110`)
- Write the changelog out from the history, which the release checks first (`task-2110`)
- The control round trip test waits thirty seconds for its request, not two (`task-2110`)
- Write the changelog out from the history (`task-2110`)

## 0.57.0 — 2026-09-23

- Publishing the open source leaves a tag the public repository already has alone (`task-2096`)
- Draw the four panel_docking arrangements with a detached terminal tab rather than a real shell (`task-2097`)
- Accept the four resize pictures with a detached terminal tab in them (`task-2097`)
- The model selector is as wide as its menu's rows need (`task-2096`)
- Accept the chat pane pictures with the wider model selector (`task-2096`)
- Window test binaries from every checkout take turns at the temp fixtures (`task-2100`)
- A window's backgrounds are in the store it was given, and nowhere else (`task-2105`)
- Accept the pictures a fresh store now draws on its own forest picture (`task-2105`)
- Format the background changes with rustfmt (`task-2105`)
- Clear the release's clippy and changelog checks (`task-2105`)

## 0.56.0 — 2026-09-23

- Five backgrounds ship in the binary, and a fresh Unluminous is drawn on the forest one (`task-2063`)
- Agent chat tool limit, zoom, screenshots, model selector and tool icon (`task-2096`)
- Accept the window suite's pictures (`task-2096`)
- Accept the resize picture that two runs in a row drew the same way (`task-2096`)
- Write the changelog out from the history, which the release checks first (`task-2096`)
- Format the ticket's own code the way cargo fmt writes it (`task-2096`)
- Build the test's settings in one expression, which clippy asks for (`task-2096`)

## 0.55.0 — 2026-09-22

- The window and its panes resize on macOS, and an agent's file changes are shown (`task-2062`)
- Install and restart, a daily check, preview zoom, Ctrl+[ and Mermaid that reads (`task-2063`)
- Drive update install and skip from the command line, accept the redrawn diagrams (`task-2063`)
- The helper's test waits for its stand-in to exit before deleting it (`task-2063`)
- Write the changelog out from the history, which the release checks first (`task-2063`)
- Take a string slice in the resize test helper, which clippy refuses as a &String (`task-2063`)
- Write the changelog out again after rebasing onto task-2062 (`task-2063`)
- The documentation tests read the reference with its line endings made plain (`task-2063`)

## 0.54.2 — 2026-09-21

- A release reclaims the build output it just superseded (`task-2011`)
- Mark tools/prune-target.sh executable, as release.sh invokes it directly (`task-2011`)
- The nightly run reclaims the test binary it superseded (`task-2011`)
- The chat pane queues, selects, and says what a tool call is (`task-2060`)
- Write the changelog out from the history, which the release checks first (`task-2060`)

## 0.54.1 — 2026-09-19

- A release asks whether the macOS archive is attached rather than filtering the list (`task-2004`)
- Every platform is checked, and a missed destination fails the release (`task-1995`)
- The seven reports, each measured against the real window (`task-2009`)
- Write the changelog out from the history, which the release checks first (`task-2009`)

## 0.54.0 — 2026-09-19

- MacOS is part of a release, not a flag, and the release says what it reached (`task-1995`)
- A divider takes room from whatever is between the sides, and a page gives the keyboard back (`task-2004`)
- Write the changelog out from the history, which a release checks (`task-2004`)

## 0.53.1 — 2026-09-19

- A release publishes both sites, because the parent page reads the manifest the product page publishes (`task-1993`)
- The changelog entry for the release publishing both sites (`task-1993`)
- Unluminous.app is built, signed and notarised on the Windows machine (`task-1995`)
- The Developer ID reaches the macOS build without a Mac to export it (`task-1996`)
- Sign with a sealed key and a certificate, not only with a .p12 (`task-1995`)
- A pane toggle opens that pane alone, and every node scrolls (`task-2003`)

## 0.53.0 — 2026-09-19

- The update check asks unluminous.com and falls back to the public repository, because every address it knew was a private one that answers 404 (`task-1993`)

## 0.52.0 — 2026-09-19

- The backdrop records how it was made, the way every plugin icon does (`task-1994`)
- A test waits for the window rather than panicking when four frames were not enough, at all 1,023 places it waits (`task-1984`)
- Three framing buffers search from where the last search stopped, and the adapter one refuses a frame that never comes (`task-1984`)
- A screen cut to a limit encodes each row once, and what the terminal costs a frame is measured rather than assumed (`task-1984`)
- A Codex item reports the part that is new without copying the part that is not (`task-1984`)
- One stem draws at most two thousand names from the project index, which puts the keystroke back under its budget (`task-1984`)
- The release gates ask about this commit rather than about an ancestor of it (`task-1984`)
- Three module comments say what the code does rather than what it nearly does (`task-1984`)
- The two tool shapes are compared by a ratio a test holds rather than by three numbers that went stale twice (`task-1984`)
- The last of the comments that said the opposite of the code, including two scroll bars that were never the scrollbar (`task-1984`)
- A range is declared once, and the clamp, the word list and the sentence a person reads all come from it (`task-1984`)
- Laying a paragraph out is its four phases, and an editing area is what it reads, what it takes and what it draws (`task-1984`)
- Three dispatchers are a verb a function, the way every plugin dispatch already was (`task-1984`)
- The canvas keeps its command line half in app/cli beside the other thirteen, a verb a function (`task-1984`)
- The board keeps its configuration and its terminals in files of their own (`task-1984`)
- The commands answered without a window are answered once, and the client only prints them (`task-1984`)
- The three structures an edit moves each move by their own rule, and the tokeniser is fuzzed with several edits at once over multi byte text (`task-1984`)
- The two files with no tests in them have tests: a ticket driven end to end, and every sentence a database dialog puts in front of somebody (`task-1984`)
- The six queries the database tree is built from are replayed against the scripted server (`task-1984`)
- A picture accepted on one platform and not the other is named on every run, on either (`task-1984`)
- Three more agent study scenarios, for the board, the themes and a chat node on the canvas (`task-1984`)
- The ground behind every text box is a named control rather than a node with nothing on it (`task-1984`)
- The rule about not running to quiet is a mechanism now, and the window tests are run in debug (`task-1984`)
- Cargo fmt over the two files the splits left it owing (`task-1984`)
- An area tool refused with no command names the commands it has, which the agent study measured at 545 refusals in 408 seconds (`task-1984`)
- Every branch of an area tool's schema requires its own command, because a converter builds one alternative per branch and every one of them accepted the empty call (`task-1984`)
- The rule that a oneOf branch states its own requirement, and what the study measured its absence costing (`task-1984`)
- An area tool's schema has no root oneOf, because a client drops the parameter object along with the combinator it cannot express and the model is then told the tool takes nothing (`task-1984`)

## 0.51.0 — 2026-09-19

- The backstop is derived, the tool's timeout reaches every area, and three tools that could never succeed (`task-1984`)
- The drawing thread never waits on a network, a database, a process or a keychain (`task-1984`)
- A frame costs what changed, and an idle one is half what it was (`task-1984`)
- The export marker is read once as the file is walked rather than re-read from the start of every declaration (`task-1984`)
- Every span shares one family name, and the memory diagnostic reports what the undo history holds (`task-1984`)
- The caret walks a window round itself rather than the whole line, so one arrow key on a minified file is 0.002 ms (`task-1984`)
- The README is a front door, and the pictures are of this version (`task-1994`)
- 0.50.0's entries were still sitting under Unreleased (`task-1994`)
- The README of a text editor now has a picture in it (`task-1994`)
- Seven of the eight crates may not mention a window, not six (`task-1994`)
- The catalogue is 214 commands across 23 areas, and 213 of them are tools (`task-1994`)
- Replace All reads every style in one walk and writes every colour in one call, and a deletion touches only the spans it reaches (`task-1984`)
- A keystroke fingerprints the paragraphs the document says it touched rather than every paragraph in the file (`task-1984`)
- The test count is the one cargo prints today, and the per-layer ones are gone (`task-1994`)
- The token list is spliced in place and the straddling token is found by binary search, so the tokeniser costs 0.31 ms a keystroke at 2 MB (`task-1984`)

## 0.50.0 — 2026-09-18

- The release gate checks what it used to check, and the window suite leaves a receipt (`task-1984`)
- One cargo fmt pass, with nothing else in it (`task-1984`)
- The window suite receipt does not ask about the nightly's own binary (`task-1984`)
- Closing the window writes every modified tab, and a tab whose save failed stays open (`task-1984`)
- A Markdown document cannot end the process however it is nested (`task-1984`)
- A value somebody typed is never read as an option, and the table test says so (`task-1984`)
- A person's source files and the instance file are written through a temporary (`task-1984`)
- A value nobody can read is refused, and a gesture cannot be made unbounded (`task-1984`)
- Four small corrections in the core, each of which showed on the screen (`task-1984`)
- A thread that will not start is one feature saying so, and no two controls share a name (`task-1984`)
- The open source files, and the one command that publishes the mirror (`task-1989`)
- The small refusals and readings that were each one line wrong (`task-1984`)
- An agent that is not Claude Code has a guide with its own name on it (`task-1989`)
- The contrast gate asks whether a pair got worse, and the changelog catches up (`task-1984`)

## 0.49.2 — 2026-09-18

- A second review of Unluminous, and the plan that acts on it (`task-1984`)

## 0.49.1 — 2026-09-16

- The toolchain pin stops describing a CI that is not there (`task-1968`)

## 0.49.0 — 2026-09-14

- A terminal tab comes back where PowerShell was, not where it started (`task-1950`)

## 0.48.0 — 2026-09-14

- The marks on the rail and the title bar, redrawn against the icons the ticket names (`task-1949`)

## 0.47.0 — 2026-09-13

- The page that kept the keyboard, the zoom that was a bitmap, and terminals that came back empty (`task-1945`)

## 0.46.0 — 2026-09-13

- No continuous integration, by choice (`task-1928`)
- Three things the coverage tests found, and the one I got wrong first (`task-1922`)

## 0.45.0 — 2026-09-13

- A code review of Unluminous, and the plan that acts on it (`task-1922`)
- CI has produced no jobs since 7 September because a `with:` block was orphaned (`task-1922`)
- Rustfmt over the workspace, no other change (`task-1922`)
- CI lints everything, the macOS images are marked, and a release runs the suite (`task-1922`)
- Clippy over every target, and B7 to B20 (`task-1922`)
- Re-accept the Windows screenshots, and three faults found by looking at them (`task-1922`)
- The first green CI run found two things this machine could not (`task-1922`)
- The changelog gate was red whenever anything was in progress (`task-1922`)
- Two more tests that only ever passed on this machine (`task-1922`)
- A second push started a second run of five jobs and failed all of them (`task-1922`)
- A test that asked whether claude is installed on the machine running it (`task-1922`)
- The five jobs that failed in seven seconds were a billing answer, not a second run (`task-1922`)
- A terminal test that failed a release could not say what went wrong (`task-1922`)
- The environment test drove a shell that writes and exits in a millisecond (`task-1922`)
- The environment test reads a file the child wrote, not a pseudoconsole (`task-1922`)
- A session rewrote the whole process environment every time one started (`task-1922`)
- The test that stopped five releases was asking about the machine (`task-1922`)
- A test that reads source text had to be told where the source went (`task-1922`)
- A settings page was drawing a control 89 points below the dialog (`task-1922`)
- The line commands are a `Code` menu, and 211 pictures gained a word in the bar (`task-1922`)
- Five palette tests that had never been run, and what each was asking for (`task-1922`)

## 0.44.1 — 2026-09-12

- Nodes that can be typed into, and controls that fit their own text (`task-1914`)

## 0.44.0 — 2026-09-12

- Drive and photograph the window without taking the keyboard (`task-1914`)

## 0.43.0 — 2026-09-12

- Two more node kinds, a page that keeps its width, and a canvas you can drop on (`task-1914`)

## 0.42.1 — 2026-09-12

- A session has a beginning, and a terminal prints its own screen back (`task-1912`)

## 0.42.0 — 2026-09-11

- The windows come back, and a terminal brings its screen with it (`task-1908`)
- The replay's own edge cases, asked rather than assumed (`task-1908`)

## 0.41.1 — 2026-09-11

- Three reports against 0.41.0, and a program named after its version (`task-1907`)

## 0.41.0 — 2026-09-11

- The canvas comes back whole, and five faults in the way of reading it (`task-1907`)

## 0.40.0 — 2026-09-11

- The second round of reported issues, and task-1848's last four sections (`task-1905`)
- A canvas comes back in the state it was left in, and a manager to find one (`task-1906`)
- The design for six more reports about the canvas (`task-1907`)
- The remaining sections of the design (`task-1907`)

## 0.39.1 — 2026-09-10

- Say that no screenshot Unluminous takes contains a web page (`task-1904`)

## 0.39.0 — 2026-09-10

- Six of the reported issues, and two faults found proving them (`task-1848`)
- The Base of Infinite Space, a canvas of nodes wired together (`task-1904`)
- The four things the review found (`task-1904`)

## 0.38.6 — 2026-09-08

- A cluster holds no copy of its text, and the document is the one owner (`task-1839`)

## 0.38.5 — 2026-09-08

- No ticketed work; a rebuild of the version before it.

## 0.38.4 — 2026-09-07

- No ticketed work; a rebuild of the version before it.

## 0.38.3 — 2026-09-07

- No ticketed work; a rebuild of the version before it.

## 0.38.2 — 2026-09-07

- No ticketed work; a rebuild of the version before it.

## 0.38.1 — 2026-09-07

- The command that reads a whole Inillucent database by hand (`task-1814`)
- The two things driving the released build found (`task-1814`)

## 0.38.0 — 2026-09-07

- The Agent-Chat beat, and how it was caught without a monitor (`task-1811`)
- Inillucent as a third engine, and a vector drawn as a vector (`task-1814`)
- Two pictures, and the two things looking at them found (`task-1814`)
- What the third engine is, written down where the conventions are (`task-1814`)
- The New Data Source dialog offers a third engine (`task-1814`)

## 0.37.2 — 2026-09-04

- Re-cut the video storyboard for the re-shoot (`task-1811`)
- Three defects the dry run and the card render found (`task-1811`)
- An answer with no words in it, above 16 pt (`task-1811`)

## 0.37.1 — 2026-09-04

- Record what the rename actually hit (`task-1808`)
- `--version`, a refusal instead of a folder, and one Unluminous window (`task-1812`)

## 0.37.0 — 2026-09-04

- Rename Unluminate to Unluminous (`task-1808`)

## 0.36.0 — 2026-09-04

- An idle window costs a millisecond instead of twenty-four (`task-1805`)

## 0.35.0 — 2026-09-04

- A release evaluation, measured rather than asserted (`task-1803`)
- The seven defects, Find and Replace, and the keystroke at 2 MB (`task-1804`)
- CI, code signing, an update check, the licences and a changelog (`task-1804`)
- Equip an agent with some of the catalogue, and configure a plugin from one (`task-1804`)
- The study's findings acted on, accessibility scoped, the palette enforced (`task-1804`)
- Write down what changed, and a clean build again (`task-1804`)

## 0.34.2 — 2026-09-04

- A `panel` reply names the pane it is about (`task-1794`)

## 0.34.1 — 2026-09-03

- No ticketed work; a rebuild of the version before it.

## 0.34.0 — 2026-09-03

- Rename Quill to Unluminate (`task-1800`)
- "an Unluminate", not "a Unluminate" (`task-1800`)

## 0.33.0 — 2026-09-03

- Document the Database plugin, with real screenshots (`task-1792`)
- The product video's storyboard, shot list and what the shoot found (`task-1793`)
- The Database plugin's twelve fixes, and the four faults they found (`task-1795`)

## 0.32.0 — 2026-09-02

- A database explorer, and the PostgreSQL client it needed (`task-1777`)
- Thirteen things a review and a real server found (`task-1777`)
- Pin that a window comes up in the improved marks (`task-1776`)

## 0.31.0 — 2026-09-02

- CLAUDE.md says what maximising does to a project's own memory (`task-1771`)
- The palette stops being constant, so a theme can say what a name means (`task-1776`)
- A theme is a plugin, and the icons belong to it (`task-1776`)
- Write down the palette, the icon sets and where the artwork came from (`task-1776`)
- Address the Codex SOL review, and three faults found beside it (`task-1776`)
- The README, the overview and the MCP counts say what is there now (`task-1776`)
- The second Codex SOL finding, and the columns a tree is read down (`task-1776`)
- A setting put back to its default leaves the file, rather than lingering (`task-1776`)

## 0.30.0 — 2026-09-02

- Every pane zooms, every pane maximises, and the board's three listings are the board's (`task-1771`)
- Eighteen findings from the Sol review, and the twelve of them that were faults (`task-1771`)

## 0.29.2 — 2026-09-01

- The handoff is submitted, and five things about the board's window (`task-28`)

## 0.29.1 — 2026-09-01

- A terminal session takes its program with it on Windows too (`task-1769`)

## 0.29.0 — 2026-09-01

- The Agent-Chat plugin, a pane you talk to a model in (`task-1767`)
- Measure the frame, name the prompt field, and tidy the scroll state (`task-1767`)
- Begin the answer once, refuse the call rather than the command (`task-1767`)
- The pane talks to the agent you already have, not the API behind it (`task-1767`)
- Codex Sol on the command-line transport, and two things it found that corrupted a conversation (`task-1767`)

## 0.28.1 — 2026-09-01

- No rail in a pane too short for one, as well as one too narrow (`task-1765`)
- The colour rule is about the manifest here too (`task-1765`)
- Sol's second review, worked through (`task-1765`)
- The design's own sketches match what shipped (`task-1765`)
- The priority mark's own comment says what it draws (`task-1765`)
- Claude Sol's third review, and a fault the snapshot tolerance hid (`task-1765`)
- The gradients are as shallow as the picture's (`task-1765`)

## 0.28.0 — 2026-08-31

- The Agent-Tasks board drawn with Vello, and the plugin seam that lets it (`task-1765`)
- The pane's ground is the window's, so the decoration is not washed out (`task-1765`)
- Claude Sol's review, worked through (`task-1765`)
- The search box appears whenever it fits, not whenever the font is small (`task-1765`)

## 0.27.5 — 2026-08-31

- No ticketed work; a rebuild of the version before it.

## 0.27.4 — 2026-08-31

- No ticketed work; a rebuild of the version before it.

## 0.27.3 — 2026-08-31

- A driving script can no longer leave a key held down (`task-1762`)
- A release build no longer depends on the shell's stray CC (`task-1762`)

## 0.27.2 — 2026-08-30

- The reported Quill and Agent-Tasks faults, with the plugin they are in (`task-28`)
- Windows build fix, merge-fallout test fixes, and Shift+Tab/Shift+Space dedent (`task-1759`)

## 0.27.1 — 2026-08-30

- Update stale HTML icon screenshot snapshot (`task-1759`)

## 0.27.0 — 2026-08-30

- Render web pages and local HTML in Quill tabs (`task-1756`)
- Record the live verification numbers in the TDD (`task-1704`)

## 0.26.3 — 2026-08-29

- Replace HTML placeholder icon (`task-1717`)

## 0.26.2 — 2026-08-29

- Document generated HTML plugin icon recipe (`task-1717`)

## 0.26.1 — 2026-08-29

- Identify missing highlight range start (`task-1703`)

## 0.26.0 — 2026-08-29

- Redirect misplaced argument guesses (`task-1703`)

## 0.25.0 — 2026-08-29

- Record complete harness verification (`task-1703`)

## 0.24.0 — 2026-08-29

- Fold collapse and expand take --recursive, and a whole block opens at once (`task-1707`)

## 0.23.0 — 2026-08-29

- Terminal send and read take --tab, and naming a tab does not show it (`task-1705`)

## 0.22.0 — 2026-08-29

- Proportionate replies — status, action list and fold collapse answer one question at a time (`task-1704`)

## 0.21.0 — 2026-08-29

- Normalize MCP names and expose grouped arguments (`task-1703`)
- Tab and Space indent the selection, and the panel header keeps the normal cursor (`task-1747`)

## 0.20.0 — 2026-08-28

- The HTML plugin, and the markup a tokeniser could not read (`task-1694`)

## 0.19.4 — 2026-08-28

- Fix saved-state undo and hypothetical completion (`task-1702`)

## 0.19.3 — 2026-08-28

- No ticketed work; a rebuild of the version before it.

## 0.19.2 — 2026-08-27

- Cover compact debug command replies (`task-1700`)
- Cover proportionate debug reply shapes (`task-1700`)

## 0.19.1 — 2026-08-27

- Foreground paused debugger state (`task-1700`)
- Tailor each debug reply envelope (`task-1700`)

## 0.19.0 — 2026-08-27

- Steer agents to Quill-native commands (`task-1699`)

## 0.18.0 — 2026-08-27

- The value under the pointer while a program is stopped (`task-1696`)

## 0.17.0 — 2026-08-27

- Watch an agent drive Quill, and write down what it did instead (`task-1695`)
- The screen-description finding was raised and not taken (`task-1695`)
- A panel is dragged to an edge of the window and snaps there (`task-1697`)

## 0.16.0 — 2026-08-26

- The gutter lines up with the text, and a window comes back where it was left (`task-1693`)

## 0.15.1 — 2026-08-26

- No ticketed work; a rebuild of the version before it.

## 0.15.0 — 2026-08-26

- Two buttons at the top right, and debugging that works out of the box (`task-1692`)

## 0.14.2 — 2026-08-26

- The command channel answers a window that is not drawing, and says so when it cannot (`task-1691`)

## 0.14.1 — 2026-08-26

- Do not paint an inline value the debugger could not read (`task-1689`)

## 0.14.0 — 2026-08-26

- Breakpoints, stepping and inspection, over the Debug Adapter Protocol (`task-1689`)

## 0.13.0 — 2026-08-26

- Collapse and expand blocks, with the line numbers kept right (`task-1686`)

## 0.12.0 — 2026-08-26

- TDD for debugging — breakpoints, stepping and inspection over DAP (`task-1687`)
- Restore markdown.rs mistakenly committed as markdown/old.rs (`task-1687`)
- Rewrite the Markdown preview as a real parser, and let it be read (`task-1685`)

## 0.11.1 — 2026-08-26

- A run keeps what its program printed (`task-1684`)

## 0.11.0 — 2026-08-26

- Run configurations, a run tile and a run widget (`task-1684`)

## 0.10.0 — 2026-08-26

- Rename a terminal tab, drag one along the strip, and answer a modal with Enter (`task-1682`)

## 0.9.1 — 2026-08-26

- Asking the delete question no longer reports a deletion (`task-1681`)

## 0.9.0 — 2026-08-26

- Delete a file, save a tab on close, and move a file with its references (`task-1681`)

## 0.8.0 — 2026-08-26

- Run configurations TDD; task-1684 filed for the implementation (`task-1683`)
- Completing an import offers the project's files and what they export (`task-1680`)

## 0.7.1 — 2026-08-26

- The folder on the command line is resolved to a real path (`task-1679`)

## 0.7.0 — 2026-08-26

- An MCP server, generated from the command catalogue (`task-1679`)

## 0.6.0 — 2026-08-26

- The TDD for auto-complete (`task-1677`)
- Auto-complete, a list of names offered under the caret (`task-1678`)

## 0.5.0 — 2026-08-26

- Go to definition, find all references and rename (`task-1676`)

## 0.4.2 — 2026-08-26

- The TDD for go to definition, find all references and rename (`task-1675`)

## 0.4.1 — 2026-08-26

- The README says how Quill is built, and how a plugin works (`task-1674`)

## 0.4.0 — 2026-08-26

- The two halves scroll together, tabs are dragged, and a document has a scrollbar (`task-1673`)

## 0.3.1 — 2026-08-25

- A zoom that keeps the line you were reading where it was (`task-1672`)

## 0.3.0 — 2026-08-25

- A CSS plugin, and Ctrl+C that interrupts in the terminal (`task-1671`)

## 0.2.1 — 2026-08-25

- Record in the TDD what was built and what the release proved (`task-1667`)
- A terminal that opens in the project, running the shell you use (`task-1670`)

## 0.2.0 — 2026-08-25

- Go to File, Find in Files, movable modals, pictures in the preview (`task-1659`)
- A command line for Quill, and the rule that keeps it complete (`task-1661`)
- Mermaid diagrams, drawn in Rust rather than by mermaid.js (`task-1660`)
- Highlighting a passage, and the file beside the project that remembers it (`task-1663`)
- The explorer follows the tab, and the editing area splits into panes (`task-1664`)
- A frame costs what is on the screen, not what is in the file (`task-1666`)
- The About box, a real build date, and a one-command release (`task-1667`)
- Read the GitHub credential through a file, not a pipe (`task-1667`)

## 0.1.0 — 2026-08-25

- Get Quill running on Windows (`task-1648`)
- Line numbers, file tabs, an explorer menu, git and plugins (`task-1649`)
- An installer for Windows and macOS, from a folder of its own (`task-1650`)
- Let the desktop show through the window on Windows (`task-1652`)
- Stop the screenshot tests crashing by sharing graphics devices (`task-1654`)
- A documentation folder of captures of the running window (`task-1655`)
- Typing in a text box no longer types into the document too (`task-1656`)
- The text options behind one button, a global font, and zooming (`task-1657`)
- The window's own frame, a rail of panes, project state and pictures (`task-1658`)
