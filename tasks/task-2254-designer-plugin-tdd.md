# Designer: the technical design

`task-2254`. `tasks/task-2254-designer-plugin-prd.md` is what it does and why;
`_agent_output/task-2254-designer/research.md` is the survey; `_agent_output/task-2254-designer/mocks/` holds
the HTML mocks and their PNGs, which the drawing is measured against. This document is how it is built, in
the order it should be built.

## 1. The shape, in one paragraph

A **`designer` UI plugin** whose provider is ordinary code in `services::designer`, exactly as Agent-Tasks,
Agent-Chat and the Database are. It contributes a **pane** (the list of designs), a **menu**, a **Settings
page** and its own **commands**. What it cannot do on its own is draw the page, because a window has one
native web view and only the window owns it (`services::browser` records why). So a design opens as a
**design tab**: a plugin tab whose key is `designer:<relative path>`, drawn by the window the way a realm
tab is (`app::realm_files::show_a_realm_tab`), which asks the provider for everything that is not the page
and draws the page through the browser machinery that already exists. The page and the window talk by a
script evaluated in the page and **polled** by the host, which is the rule the element picker set in
`task-2203`: no bridge the page can call.

```
                ┌───────────────────── window (UnluminousApp) ─────────────────────┐
 explorer ──open──▶ app::designer::open_a_design_in_a_tab                         │
                │        │  PluginTab { key: designer:.designs/landing.html }      │
                │        │  BrowserTab (Local) kept in DesignerTabs                │
                │        ▼                                                        │
                │  show_a_design_tab ── components::designer::tab::show ──▶ outcome
                │        │                 (toolbar, sidebar, prompt box)          │
                │        ├─ BrowserPlacement at the device's width (+ zoom)        │
                │        └─ BrowserHost::watch(id, designer.js)  ◀── reports ──────┤ page
                │                                                                  │
                └──── provider("designer") downcast ──▶ services::designer::Designer
                                                          notes, library, history, config
                                                          ── RunCommand plugins.run agent-chat send ──▶ chat
```

## 2. Files

| File | What is in it |
|---|---|
| `crates/unluminous-app/plugins/designer/plugin.conf` | The manifest. |
| `crates/unluminous-app/plugins/designer/icon.png`, `icon-128.png`, `icon.md` | The icon, drawn programmatically like the HTML plugin's and recorded in `icon.md`. |
| `services/designer/mod.rs` | `Designer`, the provider: state, `UiProvider` impl, commands, `view`. |
| `services/designer/config.rs` | `Configuration`, read from and written to the plugin's `settings.conf`. |
| `services/designer/library.rs` | Which files are designs, listing them, making one from the starter page. |
| `services/designer/notes.rs` | `Note`, `Anchor`, `Message`, the `<name>.comments.json` file, read and written. |
| `services/designer/history.rs` | The copies taken before a request, listed, restored and pruned. |
| `services/designer/prompt.rs` | The words sent to Agent-Chat for one note, every open note, or a free request. |
| `services/designer/page.rs` | The page protocol: the script, the calls the host makes, and reading a report. |
| `services/designer/designer.js` | The script evaluated in the page. |
| `services/designer/starter.html` | The starter page, in the house style. |
| `services/designer/tests.rs` | The unit tests for all of the above. |
| `components/designer/mod.rs` | `Act`, what a press in any of the designer's surfaces asks for. |
| `components/designer/tab.rs` | The design tab: toolbar, page well, sidebar, prompt box. |
| `components/designer/notes.rs` | The note thread cards and the composer. |
| `components/designer/pane.rs` | The Designs pane. |
| `components/designer/settings_page.rs` | The Settings page. |
| `components/designer/kit.rs` | The `rux` layer helpers, the shape `components/database/kit.rs` has. |
| `app/designer.rs` | Opening a design, drawing the tab, the page's placement, and acting on what the page reports. |
| `tests/designer.rs` | Screenshot and behaviour tests that build the whole window. |

Registration: `"designer"` added to `plugins::UI_PROVIDERS` (not to `CORE_PROVIDERS`: the provider draws its
own pane and page), `plugin_ui::provider` builds a `Designer`, `plugins::bundled` lists the folder, and
`PANE_ICONS` and `activity_bar::pane_icon` get a `design` mark (a pen nib over a frame), with the matching
`rux::Icon` if `rux` has one and Unluminous's own drawing otherwise.

## 3. The manifest

```
plugin.id          = designer
plugin.name        = Designer
plugin.version     = 1.0.0
plugin.vendor      = Unluminous
plugin.kind        = ui
plugin.description = ...  (PRD §4 in two sentences)
plugin.limitations = ...  (PRD §7)

ui.provider = designer
ui.chrome   = vello

pane.id      = designs
pane.label   = Designs
pane.icon    = design
pane.side    = left
pane.group   = top
pane.width   = 300
pane.height  = 320
pane.applies = always

menu.name    = Designer
menu.entries = open-pane=Show Designs, new=New Design..., -, ask-all=Send Open Notes to Agent, history=Design History, -, mode-browse=Browse, mode-select=Select, mode-comment=Comment

settings.page = Designer
settings.icon = design

explorer.shows = .designs
```

`explorer.shows` is fixed in the manifest, so a designs folder configured to another dot folder would be
hidden by the explorer. `Plugins::explorer_shows` is therefore asked by the window together with the
designer's configured folder when that folder's first component starts with a dot
(`UnluminousApp::dot_folders_the_explorer_shows`, one function the explorer already calls in one place).

## 4. What is on disk

**The designs folder** is `Configuration::folder`, relative to the project, `.designs` by default. A design
is any `*.html` or `*.htm` file directly in it or below it. `library::is_a_design(project, folder, path)`
is the one question, asked by the window before it opens a file and by the provider when it lists them.

**Notes**: `<folder>/<stem>.comments.json`, beside the design, committed with it:

```json
{
  "version": 1,
  "design": "landing.html",
  "next": 4,
  "notes": [
    {
      "id": 3,
      "state": "open",
      "anchor": {
        "selector": "main > section.hero > h1.hero-title",
        "tag": "h1.hero-title",
        "excerpt": "Ship designs with your agent",
        "fx": 0.92, "fy": 0.10,
        "page": { "x": 612.0, "y": 188.0 },
        "device": "desktop"
      },
      "messages": [
        { "author": "you", "body": "Too loud. Try 48px and the ink colour.", "at": "2026-10-12T23:40:12Z" },
        { "author": "claude", "body": "Changed to 48px, colour #1d2330.", "at": "2026-10-12T23:41:02Z" }
      ],
      "created": "2026-10-12T23:40:12Z",
      "resolved": null
    }
  ]
}
```

- `id` is a small number that never repeats inside a design (`next`), because a person and an agent both
  say "note 3".
- `author` is free text: `you` for a note made in the window, the name the agent gives (`claude`,
  `codex`, or `agent` when it gives none) for one made through a command.
- `fx`, `fy` are where the click was inside the element, as fractions of its box, so the pin sits where it
  was put on an element whose size changes. `page` is the last place the element was seen in page
  coordinates at `device`, which is where a detached note is drawn.
- **Unknown fields are kept** and written back (a `serde_json::Value` side map on each struct, through
  `#[serde(flatten)]`), the rule `realm::store` keeps for a key it does not know.
- The file is written with `services::store`'s atomic write (write a temporary file beside it and rename),
  and **only when something changed**, so opening a design and looking at it leaves `git status` clean.
- The provider re-reads a notes file whose modified time moved since it last read it (checked in
  `catch_up`, at most twice a second), so an agent that edits the JSON with its own tools is seen.

**History**: `<project>/.unluminous/designs/<folder relative path>/<stem>/<stamp>-<slug>.html`, where `stamp`
is `YYYYMMDD-HHMMSS` in local time and `slug` is the first five words of the request, lowercased, joined by
`-`, at most 40 characters. Personal rather than shared, because it is what one person's agent did in the
minutes between commits. `Configuration::history_kept` (20) newest copies are kept per design; older ones
are removed by literal path as a new one is written. `restore` first copies the current file under the
slug `before-restore`, then copies the chosen copy over the design.

**What one person was doing** (device, mode, filter, the note open, the scroll) lives in the provider's
memory for the session and, for the device alone, in the project's `.unluminous/designs.conf` as
`device.<relative path> = phone`, written through `services::store::Values`.

## 5. Configuration

`services/designer/config.rs`, the shape `services::database::config` has:

| Key in `settings.conf` | Type | Default | Meaning |
|---|---|---|---|
| `folder` | path | `.designs` | The designs folder, relative to the project. An absolute path or one containing `..` is refused with a sentence. |
| `device` | `desktop\|laptop\|tablet\|phone\|fit` | `fit` | The device a design opens at. |
| `starter` | `house\|blank` | `house` | Which starter page `new` writes. |
| `send_html` | bool | `true` | Whether a request carries the start of the element's HTML. |
| `history_kept` | 1..=200 | `20` | Copies kept a design. |

These are reachable as `settings get/set plugins.designer.<key>` through `services::plugin_settings` with
nothing added, which is that module's own promise.

## 6. Devices and the page's placement

```rust
pub enum Device { Desktop, Laptop, Tablet, Phone, Fit }
impl Device {
    pub fn width(self) -> Option<f32> { match self { Desktop => Some(1440.0), Laptop => Some(1280.0),
        Tablet => Some(834.0), Phone => Some(390.0), Fit => None } }
}
```

`components::designer::tab::page_placement(well: Rect, device: Device) -> (Rect, f64)` is pure and tested:

- `Fit`: the whole well inset by 12 points, zoom 1.0.
- A device width `w` that fits in the well (inset by 24): a rectangle `w` wide, the well's height inset by
  24, centred horizontally, zoom 1.0.
- A width that does not fit: zoom `z = available / w`, rounded down to a hundredth and never below 0.25, and
  a rectangle `w * z` wide. The page lays itself out at `w` because the engine's layout viewport is the
  bounds divided by the zoom, which is the arithmetic a browser node on the canvas already relies on.

The window pushes `BrowserPlacement { area: rect, visible: rect ∩ the tab's clip, zoom: Some(z), .. }`. The
zoom is sent only when it moves (`native::place` already does that). Around the page the well is drawn as a
carved `rux` surface; for a device other than `Fit` a thin frame and the device's name and width
(`Phone · 390`) are drawn above it.

## 7. The page protocol

### 7.1 The script

`designer.js` defines `window.__unluminousDesigner` once (idempotent like `browser_pin.js`) and gives the page
nothing to call. It draws everything inside a **closed shadow root** on a host element appended to
`document.documentElement`, positioned `fixed`, `inset: 0`, `pointer-events: none`, `z-index: 2147483647`, so
the page's own CSS cannot reach the overlay and the overlay's cannot reach the page.

State and calls the host makes (`page.rs` builds each as a string):

| Call | Does |
|---|---|
| `setMode("browse"\|"select"\|"comment")` | Browse: no listeners. Select and comment: capture phase `pointermove`, `pointerdown`, `click` and `keydown` listeners on `window` that outline the element under the pointer with its label (the picker's `describe`) and swallow the page's own click. |
| `setPins([{id, selector, fx, fy, page:{x,y}, n, active, detached}])` | Replace the pins. Each is a 22 pixel disc with its number, at the element's box `(left + fx·width, top + fy·height)`, clamped inside the viewport; a pin whose selector finds nothing is drawn at `page` and dimmed. The active one is larger and ringed. Positions follow scroll and resize through one `requestAnimationFrame` loop that runs only while there are pins or an outline. |
| `select(selector)` | Choose an element by selector (as a click would), scroll it into view and outline it steadily. |
| `focus(id)` | Scroll the pin's element into view and pulse its pin. |
| `clear()` | Forget the choice. |
| `take()` | Return and empty the report queue: `{ reports: [...] }`. |

The selector is built by the picker's own `selectorFor`, which `browser_pin.js` exposes as
`window.__unluminousSelectorFor` (one line added there; the picker keeps its private use). The designer
script is evaluated **after** `PIN_SCRIPT`, so it can call it.

### 7.2 Reports

Each report is one JSON object with a `kind`:

| `kind` | Fields | When |
|---|---|---|
| `selected` | `element` | A click in select mode, `select()`, `Alt+ArrowUp` (parent) or `Alt+ArrowDown` (first element child). |
| `comment` | `element`, `fx`, `fy` | A click in comment mode. |
| `pin` | `id` | A click on a pin, in any mode (pins take the pointer themselves). |
| `cleared` | | `Escape` in select or comment mode. |
| `anchors` | `found: [{id, x, y}]`, `missing: [id]` | After `setPins` and after every load, so the window can mark notes detached and store where each was last seen. |
| `loaded` | `url`, `scrollY` | Once a document has the script, which is how the host knows to send the mode and the pins again after a reload. |

`element` is `{ selector, tag, excerpt, html, styles, rect }`: `tag` is `describe(el)`, `excerpt` is the
element's `innerText` collapsed to one line and cut to 120 characters, `html` is `outerHTML` cut to 1200
characters on a tag boundary where possible, `styles` is eight computed properties (`font-family`,
`font-size`, `font-weight`, `color`, `background-color`, `padding`, `margin`, `border-radius`), and `rect`
is the box in page coordinates.

`page::read_reports(json) -> Vec<Report>` parses them and ignores anything it does not recognise, because a
page can write `window.__unluminousDesigner` itself and the host must not trust what comes back beyond its
shape. Every string is cut to its stated length again on the Rust side.

### 7.3 Keeping the scroll across a reload

An agent's edit reloads the page. `designer.js` writes `scrollY` to `sessionStorage` under
`__unluminousDesigner:<path>` on `pagehide`, and on its first evaluation in a new document scrolls back to it,
so a reload does not throw the person to the top.

### 7.4 How the host runs it

`BrowserHost` gains a **watch**, the generalisation of the picker's poll:

```rust
pub struct Watch { pub install: String, pub take: String }
pub fn watch(&mut self, id: u64, watch: Option<Watch>);
```

- While a watch is set for the showing tab, the host evaluates `install` on every `LoadFinished` for that tab
  (and once when the watch is set), and evaluates `take` with `evaluate_with_callback` every 150 ms, never
  two at once (the `asking` flag the picker uses), from `reconcile`, which runs before the pass.
- An answer becomes `BrowserEvent::Reported { id, json }`. `read_the_pick` stays as it is.
- The watch is dropped when the tab closes or the view is pointed at another tab, and set again when the
  design tab is shown.
- `app::frame::settle_the_native_views_before_the_pass` asks for a frame every 150 ms while a watch is set,
  as it does while the picker is open.

The window sends `setMode`, `setPins`, `select`, `focus` and `clear` through `BrowserHost::evaluate(id, …)`,
which already refuses a page that is not the one showing.

## 8. The provider

```rust
pub struct Designer {
    project: Option<PathBuf>,
    folder: Option<PathBuf>,          // the plugin's settings folder
    config: Configuration,
    designs: Vec<DesignSummary>,      // the pane's list, read on open and when the folder changes
    open: BTreeMap<PathBuf, DesignView>, // keyed by the design's path relative to the project
    showing: Option<PathBuf>,         // the design tab showing, told by the window
    outbox: Vec<Request>,             // drained by `asking`
    notes_seen: HashMap<PathBuf, SystemTime>,
}

pub struct DesignView {
    pub notes: NotesFile,
    pub device: Device,
    pub mode: Mode,
    pub filter: Filter,               // Open | Resolved
    pub selection: Option<Element>,
    pub active: Option<u32>,          // the note open in the sidebar
    pub composing: Option<Composer>,  // a new note being written: element, fx, fy, text
    pub reply: String,                // the reply box of the active note
    pub prompt: String,               // the prompt box under the page
    pub detached: BTreeSet<u32>,
    pub page_revision: u64,           // moves when pins must be sent again
}
```

The window gets at it with `provider("designer")?.as_any_mut()?.downcast_mut::<Designer>()`, which is how the
tests already reach Agent-Chat and the Database.

### 8.1 Commands

`commands()` lists, and `command()` answers, every verb in the PRD's §5 table, each through the same function
its button calls. Arguments are the words after the verb (`plugin_ui::argument` and `rest`). A verb about
"the showing design" takes `--design <name>` as its first two words instead, so an agent can work on a
design nobody is looking at; a verb that needs the page (`select`, `device`, `mode`) answers with a
refusal naming `open` when the design is not showing.

`select <selector>` cannot be answered inside `command`, because the element's details come from the page.
It records the wish (`DesignView::wants_selection`) and answers `"asked"`; the window sends `select()` on the
next frame and the page's `selected` report fills `selection`. `selection` then reads it. The agent half of
the catalogue already documents this shape for `send` and `state`.

`view()` answers the whole provider as data: the config, the designs and their counts, and for each open
design its device, mode, selection, notes and detached ids. `plugins view designer` therefore answers
everything the window draws, which is the rule.

### 8.2 Sending to Agent-Chat

`prompt.rs` builds the words. One note:

```
Design: .designs/landing.html (open in Unluminous's Designer)
Note 3 on h1.hero-title, selector `main > section.hero > h1.hero-title`:
  "Too loud. Try 48px and the ink colour."
The element now: "Ship designs with your agent"
<h1 class="hero-title">Ship designs with your agent</h1>
Computed: font-size 64px; font-weight 800; color rgb(17, 20, 26); ...

Change the design file to do what the note asks. Then reply to the note and resolve it:
  unluminous-cli plugins run designer reply 3 <what you changed>
  unluminous-cli plugins run designer resolve 3
```

`ask-all` numbers every open note under one header and asks for one reply a note. A free prompt names the
chosen element when there is one and no note otherwise. `send_html` off leaves out the HTML block.

The provider pushes, in order:

1. a history copy (`history::keep`), synchronously, because it is one file copy;
2. `Request::RunCommand { id: "designer-send", command: "plugins.run", arguments: { id: "agent-chat",
   command: "open-pane" } }`;
3. `Request::RunCommand { id: "designer-send", command: "plugins.run", arguments: { id: "agent-chat",
   command: "send", arguments: [<words>] } }`.

So the request goes down `run_cli`, the one path a command becomes a change, and is exactly what an agent
typing `unluminous-cli plugins run agent-chat send …` would cause. The answer, through `answered`, sets a
status line on the design (`Sent to Agent-Chat`, `Queued behind the answer arriving`, or the refusal, for
example when Agent-Chat is switched off).

`ask <note>` adds a system line to the note's thread (`author: "designer"`, `body: "Sent to the agent"`), so
the thread shows when it was asked.

## 9. The window

`app/designer.rs`:

- `pub(crate) const DESIGN_TAB: &str = "designer:";` and the key helpers, the shape of `realm_tab_key`.
- `open_path_in_tab` asks first: when the designer is on and `library::is_a_design` says yes, it calls
  `open_a_design_in_a_tab(path)` and returns. `open_a_design_as_text(path)` is the path `Open Source` takes,
  which skips that question (a `bool` argument on a private `open_path_in_tab_as` rather than a flag on the
  window).
- `open_a_design_in_a_tab` opens a `PluginTab { key, plugin: "designer", label: file name }` and a
  `BrowserTab` for `BrowserLocation::Local` on the file (root: the project, so a design can link
  `../shared.css` inside the project), kept in `self.design_tabs: HashMap<String, BrowserTab>`. Closing the
  plugin tab closes the browser tab (`close_tab` already special cases plugin tabs; one arm).
- `design_tabs` join the list `settle_the_native_views_before_the_pass` reconciles, and `browser_tab` and
  `change_browser_tab` look there as their third place, so titles and load events reach them.
- `show_editing_area` asks for `tab.plugin == "designer"` beside the realm's arm and calls
  `show_a_design_tab(ui, area, &tab)`, which:
  1. tells the provider the design is showing (`Designer::show`, which also loads its notes once);
  2. calls `components::designer::tab::show(designer, ui, area, look)` and gets `(TabOutcome, Rect well)`;
  3. pushes the placement from `page_placement(well, device)` unless a modal or popup covers the well;
  4. sets the watch on the browser tab, and sends what changed: `setMode` when the mode moved, `setPins`
     when `page_revision` moved, `select`/`focus`/`clear` for a wish the provider recorded;
  5. acts on `TabOutcome`: `OpenSource`, `Reload`, `ShowHistory`, and the provider's requests.
- `act_on_browser_events` handles `BrowserEvent::Reported` for a design tab's browser by handing each report
  to `Designer::page_reported(path, report)`, which updates the selection, opens the composer, makes a pin
  active, records `anchors` (detached ids and each note's last place, written only when a place moved by more
  than 2 pixels), and on `loaded` bumps `page_revision` so the mode and pins are sent again.
- Keyboard: while a design tab has the keyboard and no text box does, `Escape` clears the selection,
  `C` switches to comment mode, `V` to select, `B` to browse, `[` / `]` move the selection to the parent and
  the first child. They are read in `route_the_keys_before_the_panes` with the notebook's keys.
- The explorer's dot folders: `UnluminousApp::dot_folders_the_explorer_shows` (§3).
- Session: a design tab is a plugin tab, so `plugin-tabs.txt` already brings it back; the browser tab is
  made again from the key when the tab is first shown, and is never written to `browser-tabs.conf`.

## 10. The drawing

Everything is `rux` through `components/designer/kit.rs`, the pattern `components/database/kit.rs` and
`components/agent_chat/kit.rs` follow: a small layer per piece, words drawn after so they sit on top, every
size in the design's points whatever the interface font is (`Look::at_a_fixed_size`), no colour named.

**The tab** (`tab.rs`), from `mocks/design-tab.png`:

- A toolbar 50 points tall: the design's name in the title style; a `PillSwitch` for the mode (`Browse`,
  `Select`, `Comment`); a `PillSwitch::small` for the device (`Desktop`, `Laptop`, `Tablet`, `Phone`,
  `Fit`); the zoom reading (`64%`) in a carved chip; round chip buttons `History`, `Source`, `Reload`.
- The page well: everything between the toolbar, the sidebar and the prompt box, a carved surface 18 points
  round.
- The sidebar, 320 points wide (collapsing to nothing below a tab 760 points wide, with a chip on the
  toolbar that brings it back as an overlay): `Notes` with a `PillSwitch::small` `Open` / `Resolved` and the
  count; the chosen element card (tag, selector in the mono style, `Parent` / `Child` chips, `Comment`
  button); then the threads.
- A thread (`notes.rs`) is a raised card like a chat message: the number disc, the element's tag chip, each
  message with its author and its words drawn with `components::markdown_text` so a reply's code is
  coloured, a carved reply box when it is the active thread, and `Ask agent`, `Resolve` (or `Reopen`) and a
  `...` menu with `Move to the chosen element` and `Delete`. A detached thread carries a `Detached`
  `NotePill`.
- The composer for a new note sits at the top of the threads with the element's tag, a carved text area,
  `Add note` and `Add and ask agent`.
- The prompt box under the page: one carved box with a hint naming what it is about (`Ask about
  h1.hero-title...` or `Ask about this design...`), the 44 point round send button, and a pill `Send open
  notes (3)` beside it when there are open notes.
- A status line in the toolbar's right end for the last thing that happened (`Sent to Agent-Chat`).

**The pane** (`pane.rs`): a header with `New Design` as a round chip, a carved search well, then one chip a
design: its name, its folder when it is in a subfolder, and a badge with its open note count; the showing one
raised. A click opens it. An empty folder draws one sentence and the `New Design` button.

**The Settings page** (`settings_page.rs`): `settings_kit::plugin_page` with two cards, `Designs` (folder as a
`path_well` with `Choose...`, starter as `segmented`, default device as `segmented`) and `Agent requests`
(send HTML as a `switch`, history kept as a `slider`).

**History** is a `modal::show_unframed` dialog listing copies newest first, each a chip with its time and
words and a `Restore` button, the shape of the realm manager.

## 11. Agent-Chat knows the design is open

`UiProvider::showing(project, file)` already tells every provider which file is showing. For a design tab the
window passes the design's path, so Agent-Chat's existing context (what file is open) already names the
design, and nothing in Agent-Chat changes. The messages in §8.2 carry everything else.

## 12. Tests

**Unit tests** (`services/designer/tests.rs`), no window:

- `library`: which paths are designs for the default and a configured folder; a folder outside the project
  refused; `new` writes the starter and refuses a name that exists or holds a separator.
- `notes`: a round trip keeps unknown fields; ids never repeat after a delete; nothing is written when
  nothing changed; a file another program changed is read again.
- `history`: a copy's name; pruning keeps the newest N; restore copies the current file first.
- `prompt`: one note, all open notes, a free prompt with and without an element, `send_html` off; every
  message names the reply and resolve commands with the note's number.
- `page`: `read_reports` reads each kind, ignores an unknown kind and a wrong shape, and cuts long strings;
  the scripts it builds quote a selector with a quote and a backslash in it correctly.
- `page_placement`: fit, a device that fits, one that does not, the zoom floor.
- `commands`: every verb in `commands()` is answered; `comment`, `reply`, `resolve`, `reopen` change the
  file as the buttons do; `ask` pushes the history copy and the two `RunCommand`s in order.

**Window tests** (`tests/designer.rs`), the shape of `tests/notebooks.rs`: open a project with a `.designs`
folder, open the design through `tab open`, and assert the tab is a design tab and a `BrowserPlacement`
was pushed at the device's width; feed `BrowserEvent::Reported` events through `act_on_browser_events`
(the seam `task-1905` added for exactly this) and assert the selection, the composer and the detached
marks; drive `comment` and `ask` and assert the history copy and the chat's queued message; screenshots
`designer_tab`, `designer_tab_phone`, `designer_pane`, `designer_settings`. A native page is in no
screenshot (CLAUDE.md, "no screenshot Unluminous takes contains a page"), so the pictures show the toolbar,
the well, the sidebar and the prompt box; the page itself is checked in the real window with
`tools/capture-window.ps1`.

**The CLI**: `plugins run designer ...` needs no catalogue rows, because `plugins run` is one already. The
`plugins` area's preamble in `catalogue.rs` names the Designer beside the other three, and
`unluminous-cli/docs/commands.md` is regenerated.

**In the real window**: build, install, open a project, `New Design`, select, comment, `Ask agent` with a
real `claude` row, watch the file change and the page reload, the note resolved; capture with
`tools/capture-window.ps1`.

## 13. Order of work

1. `services/designer` with its tests (config, library, notes, history, prompt, page), the manifest, the
   registry entries and the icon.
2. `BrowserHost::watch` and `BrowserEvent::Reported`, `__unluminousSelectorFor`, `designer.js`.
3. `app/designer.rs`: opening, the tab, the placement, the events, the keys.
4. The components: tab, notes, pane, settings page, history dialog.
5. Window tests and screenshots; `commands.md`; CLAUDE.md section; CHANGELOG through the release.
6. Build, install, drive it for real, release.

## 14. Left out, and why

Everything in PRD §7, plus two things about the build:

- **A screenshot of the element sent with a request** (Orca sends one). The page is a native child, and
  `PrintWindow` of the window costs 11 to 18 ms and needs cropping to the element; it is a follow-up that
  needs no change to the protocol (`rect` is already reported).
- **Pins drawn by egui over the page.** A native child paints above everything egui draws, so pins are drawn
  by the page's own script, inside the page.
