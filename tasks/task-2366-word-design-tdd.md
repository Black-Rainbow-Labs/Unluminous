# task-2366: the Word tab in the design system

## What was asked

The Word tab (`task-2341`, `task-2359`) was built with egui's stock widgets and hand painted shapes. The
ticket lists what does not meet the standard the rest of the window keeps:

- The format bar's paragraph style dropdown is not centred in the bar and its list is not styled like the
  window's other dropdowns.
- The Find command button's magnifier is clipped, and the command search it opens is not styled like the
  window's popups.
- Outline view offers buttons that answer with an error (`body text is already the lowest level`).
- The strip under the pages repeats the window's status bar. The view mode should be a dropdown, the
  language and the zoom slider can go, and `Page N of N` belongs in the window's own status bar.
- Bold, italic, underline and the two lists should be in a menu that opens and closes.
- The `Aa` panel needs a complete overhaul. Its close button is drawn as an empty box.
- The ruler needs a better design.

## The design language this follows

Nothing new is invented here. The Word tab is drawn with the parts the window already uses, from the Claude
Design canvas "Unluminous" (pages Editing code, Settings and Agent-Chat):

| Part | Where it is defined | What it is |
| --- | --- | --- |
| Round chip | `text_tools::options_chip`, `rux::components::chip_disc` | A 32 point disc raised out of a bar with a line mark, carved in while what it opens is open. |
| Select chip | `settings_kit::select`, `rux::components::Select::chip` | A raised pill with the value and a chevron, opening a list under it. |
| Carved track | `text_tools` view modes, `settings_kit::segmented` | A well with the chosen option raised out of it. |
| Menu card | `controls::menu_frame`, `controls::menu_row` | A raised card 18 points round, rows 30 points tall, the row under the pointer carved in. |
| Pill button | `settings_kit::button` | A chip surface 30 to 34 points tall with a word on it. |
| Switch | `settings_kit::switch` | A carved track with a knob, the action's gradient when on. |
| Groove | `chrome_kit::groove` | A dark line with a lit line beside it, between a bar and what is under it. |

Two rules from the rest of the window apply to every size here:

1. **Sizes are the design's points times the tab's own zoom**, never derived from the editor's font. The
   reported pictures came from a machine whose interface font is 24 points, and every label in the Word
   chrome was set at that size, so the command search and the panel were set at twice the size of the
   window's other menus. Each size below is multiplied by `Look::zoom`, which is the tab's own zoom and is
   1.0 unless the person zoomed the tab.
2. **No mark is a character.** `✕`, `‹`, `⊕`, `⊖` and `■` are not in every face Unluminous hands egui, and a
   face without them draws an empty box, which is the close button in the report. Every mark is a
   `rux::Icon` or a stroked path in `theme::svg_icon`.

## The pieces

### 1. `components/word/kit.rs`

One file holds every part the Word tab draws with, so the bar, the panel, the outline toolbar and the
popups cannot disagree. It keeps one `rux` state per context the way `components::database::kit` does,
because the tab zooms by a multiplier and must not share the title bar's canvases.

- `layer(ui, salt, rect, reach, add)`: a `rux` layer in the Word tab.
- `round_chip(ui, rect, name, mark, on) -> Response`: the round chip. `mark` is a `Mark` enum covering a
  `rux::Icon` or an `svg_icon::SvgMark`.
- `select_chip(ui, rect, name, value, list_width, rows) -> Option<T>`: a raised pill with the value, opening
  a `controls::menu_frame` popup under it whose rows the caller draws with `menu_row`. The value is placed by
  its capitals (`crisp::top_centring_capitals`), which is what centres it in the bar.
- `pill(ui, rect, label, on, enabled) -> bool`, `pill_width(label)`: a pill button, carved in while `on`,
  drawn at 40% ink and taking no clicks when not `enabled`.
- `section(ui, title)`: a section heading inside a panel level: small spaced capitals in the quiet ink.
- `swatches(ui, name, colours, chosen) -> Option<usize>`: colour circles 18 points across with a ring round
  the chosen one, in rows that wrap.
- `number(ui, name, value, range, step, suffix) -> Option<f32>`: a carved well holding the value, with a
  round minus and plus beside it. A drag across the well changes it the way egui's `DragValue` did, so no
  behaviour is lost.
- `switch_row(ui, label, value) -> bool`: the settings switch with its label.
- `style_the_panel(ui)`: sets the panel `Ui`'s egui visuals to the design's (corners, fills, the chip
  colours, the menu card for popups, text at 12.5 points), so the controls of the deep levels that are still
  egui widgets (`TextEdit`, the rare `ComboBox`) look like the rest.

### 2. The format bar

The bar is 44 points. From the left:

1. **Paragraph style**, a select chip 168 points wide, centred in the bar. Its list is a menu card. The
   eleven styles a person uses most come first, each name set in a size and weight that suggests the style
   (Title largest, the headings stepping down), then a groove, then every other paragraph style in the
   document in the plain row style. The chosen style has the tick `menu_row` draws.
2. **Text options**, a round chip with Lucide's `bold` mark, opening a flyout. This is the
   menu that opens and closes which the ticket asks for, and it is the same control the title bar uses for
   Markdown (`text_tools`). The flyout holds four rows, each named at its left:
   - `Text`: a carved track of bold, italic, underline and strikethrough. A raised option is on.
   - `Script`: superscript and subscript.
   - `Lists`: bulleted, numbered, decrease indent, increase indent.
   - `Align`: left, centre, right, justify.
   The chip shows a small accent dot while the caret's text is bold, italic or underlined, so the state is
   visible with the menu shut.
3. A groove.
4. **View mode**, a select chip with the view's name (`Print Layout`, `Read Mode`, `Outline`). This replaces
   the three buttons that were in the strip under the pages. It is named `View mode` because the window's
   own menu bar has a `View` menu, and two controls must not share a name.
5. At the right, three round chips: **Format** (the `Palette` mark, opens the panel at its root),
   **Insert** (`Plus`, opens the panel at Insert) and **Find a command** (`Search`). Each is carved in while
   what it opens is open. The magnifier is a 16 point mark centred in a 32 point disc, so nothing is clipped.

When the bar is narrower than everything it holds, the view chip drops its word and keeps its chevron, then
the style chip narrows to 110 points.

### 3. No strip under the pages

`views::status` and `views::STATUS` are deleted. The pages run to the bottom of the tab. What the strip said
moves to the window's status bar, through a new provider method:

```rust
/// What the window's status bar says for a tab this provider draws.
fn tab_status(&self, key: &str) -> Option<TabStatus> { None }

pub struct TabStatus {
    /// What kind of tab it is, in place of `Plugin`: `Word document`.
    pub kind: String,
    /// The facts after the kind, each a quiet word that can be pressed.
    pub items: Vec<StatusItem>,
}

pub struct StatusItem {
    pub text: String,
    /// The accessible name, which tests find it by.
    pub name: String,
    /// The provider command a press runs, as words: `["action", "go-to"]`.
    pub press: Option<Vec<String>>,
}
```

The Word tab answers `Word document`, then `Page 2 of 8` (opens Go To), `1,036 words` or `12 of 1,036 words`
(opens Review), `Tracking changes` while tracking is on (turns it off), and `3 spelling errors` when there
are any (goes to the next one). The language, the zoom slider and the zoom percent are gone. The zoom is
still Ctrl and the wheel, Ctrl and plus or minus, and `Document -> Zoom` in the panel.

`status_bar::Status` gains `items: &[StatusItem]` and answers which item was pressed in `Outcome`; the window
runs the press through the provider's `command`, which is the path `plugins run word` takes, so a press and
an agent's command are the same call. The page item runs `action go-to` and the words item `action
word-count`, two actions added for them. What the command says is not repeated in the bar; only a refusal is.

### 3a. The Settings page

`Settings -> Word` is drawn with `settings_kit`'s cards, as the spreadsheet's page is: a `Word` card with the
author, the zoom a document opens at and formatting marks, and a `Proofing` card with spell check.

### 4. The command search and the slash menu

Both are drawn by one function, now a menu card (`menu_frame` look, drawn on a foreground layer):

- The search has a carved well at the top, 36 points tall, with the search mark and what was typed, or the
  hint `Find a command` in the quiet ink, and a blinking caret.
- Rows are `controls::MENU_ROW` (30 points). The chosen row and the row under the pointer are carved in, as
  every menu's are. The command's name is 12.5 points; the panel level it belongs to is at the right in the
  quiet ink at 11.5.
- Nothing found says so in one row.
- Under the rows, a line names the keys: `Up` and `Down` to choose, `Enter` to run, `Esc` to close, each key
  in a small carved cap.
- It is 460 points wide, 10 rows tall at most, and placed 12 points under the bar, centred on the pages.

The go to box (`Ctrl+G`) becomes the same card with one carved well and the hint, and a round close chip.

### 5. The panel

Width 320 points. Its ground is the pane ground with a groove along its left edge.

**The head** is 58 points: a round back chip (`ChevLeft`) when deeper than the root, the level's name in the
design's 15 point semibold, the path above it in 11 point quiet ink (`Format`, `Format · Text`), and a round
close chip (`Cross`) at the right. The empty box in the report is gone because the mark is drawn.

**The root** lists the levels as raised rows 56 points tall, 16 points round, with a 32 point carved disc
holding the level's mark, the name in 13 point semibold and the summary in 11.5 point quiet ink elided to the
room left, and a chevron at the right. The row under the pointer lifts to the hover colour. The marks:

| Level | Mark |
| --- | --- |
| Text | Lucide `type` |
| Paragraph | Lucide `pilcrow` |
| Styles | Lucide `heading` |
| Lists | Lucide `list` |
| Insert | `Plus` |
| Page | `Document` |
| References | Lucide `book-open` |
| Review | `Chat` |
| Document | `Settings` |

**Each level** is laid out from the kit: sections with headings, pill buttons in rows that wrap, select chips
for lists of values (font, underline kind, line spacing rule), number wells for sizes, switches for on and
off settings, and swatches for colours. The deep levels that edit text (`Modify Style`, `Page Setup`,
`Symbol`, `Note options`) keep their structure and pick up the panel style, so their fields are carved wells
and their buttons pills.

### 6. Outline view

The toolbar is a strip on the pane ground above the paper, 44 points, drawn with pills. Each pill asks
whether its action can run here (`actions::applies`), and one that cannot is drawn dimmed and takes no
press. `applies` learns the outline actions:

| Action | Applies when |
| --- | --- |
| `outline-to-heading-1`, `promote-heading` | the caret's paragraph is body text, or a heading below Heading 1 |
| `demote-heading` | the caret's paragraph is a heading |
| `outline-to-body` | the caret's paragraph is a heading |
| `outline-move-up`, `outline-move-down` | there is a block before (or after) it in its container; for a collapsed heading, a section of its level before (or after) it |
| `outline-expand` | the heading at the caret is collapsed |
| `outline-collapse` | the heading at the caret has something under it and is open |

The same rule reaches the command search, which already filters by `applies`, and `move-paragraph-up` and
`move-paragraph-down`, which answered with errors the same way. After the toolbar: a select chip for
`Show level` and a switch for `First line only`. `Close Outline View` goes, because the view chip in the bar
does the same.

The rows' marks are drawn: an open heading with children is a chevron pointing down in a small disc, a
collapsed one a chevron pointing right, a heading with nothing under it a short bar, and body text a small
dot. The current row is a soft accent wash rather than a box drawn round the words.

### 7. The ruler

The ruler is 26 points on the pane ground with a groove under it.

- The page is a rounded band (radius 5) in the paper colour at 35%, and the text column is the paper colour
  at full strength, so the margins read as recessed.
- Ticks every eighth of an inch are hairlines in the ink at 35%, quarter inches a little longer, half inches
  longer again, and each inch is a number in 9.5 point quiet ink.
- The indent marks are filled triangles in the accent (`chat.sky`) with a dark outline at 40%: first line at
  the top pointing down, hanging indent at the bottom pointing up, the left indent box under it, the right
  indent at the right. A mark under the pointer is drawn 20% larger and the pointer becomes the resize
  pointer.
- Tab stops are the accent marks they were, thicker (1.8 points).
- While something is dragged, the guide down the page is a dashed accent line, and a small card beside the
  pointer says what is being moved and where: `Left indent 0.50 in`.
- The tab kind selector is a 20 point round chip at the left of the ruler, with the kind's mark, and the hint
  says the kind and that a press changes it.

## What does not change

- Every action still runs through `actions::ACTIONS` and `components::word::run_action`, so the bar, the
  panel, the search, the `/` menu and `plugins run word action` are one path.
- Every control keeps an accessible name. Names the tests use and that still describe the control are kept
  (`Bold`, `Italic`, `Format: text, paragraph, styles, page and more`, `Landscape`, `Promote`, `Ruler`,
  `Tab kind`).
  `View: Outline` and `View: Read Mode` are gone: the views are the rows `Outline` and `Read Mode` of the
  `View mode` chip. `Status: page`, `Status: words` and `Status: track changes` are the window's status bar
  items, and the spelling count keeps the name `Next spelling error (F7)`.
- Keyboard handling, layout, pagination and the document model are untouched.

## Tests

- `actions::tests`: `applies` refuses each outline action in the cases above and allows it otherwise, on a
  document with a Heading 1, body text and a Heading 2.
- `services::word::tests`: `tab_status` says the page, the words, tracking and spelling, and a press of each
  runs the command named.
- `tests/word.rs`: the views test opens the view select and chooses each view; the outline test checks that
  `Demote` is dimmed on body text and that pressing it posts no notice; the format bar test opens Text
  options and presses Bold; a test presses `Page 1 of 2` in the window's status bar and finds the go to box.
- Every Word picture is taken again, looked at, and accepted: the bar, the open flyout, the style list, the
  command search, the slash menu, the panel root and three levels, the outline, the ruler while dragging, and
  Read Mode.
