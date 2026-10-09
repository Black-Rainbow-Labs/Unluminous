Black Rainbow Labs Rux is a neumorphic component library: surfaces lit from the top left, raised controls that sink when pressed, wells pressed into the page, and colour used as light rather than as fill. It is written in Rust for egui and vello, and this system is its web twin, used to design screens before they are carried back into the Rust. Unluminous, the editor, is built on it, and its own window parts are here too, under the Unluminous group.

## Themes

Four themes, one set of names. A theme says what a name means and never adds one, so every component reads the same tokens in every theme.

| Theme | What it is |
|---|---|
| `light` | Light Neumorphic, ai-service's normal mode. The reference the library was measured against. |
| `dark` | Dark Neumorphic, ai-service's incognito mode. Same recipes, different values: the lit half of a shadow is a lifted grey, not white. |
| `unluminous-dark` | rux retoned from Unluminous Dark: Unluminous's own surfaces, ink and azure accent with rux's shadows, gradients and other accents. What a rux control in the Unluminous window looks like. |
| `unluminous-light` | The same from Unluminous Light. |

Design Unluminous screens in `unluminous-dark` first; it is what a window opens in. Check every screen in `unluminous-light` before calling it finished.

## Content

- Write the plain wording a person would say. A control's name is its label with no decoration: `Save`, `Start Work`, `Run the selected configuration`. No two controls on one screen share a name.
- Verbs on buttons, sentence case: `Add Task`, `Post comment`, `Send to terminal`. A modal's last button names what it does (`Rename`, `Delete`), never `OK`.
- A failure says what went wrong and what to do, in the program's own words where there are any (git's error, the server's message). Never an apology.
- British spelling in prose (`colour`, `centre`), the code's spelling for names (`color` in a token).
- No emoji anywhere: marks are drawn icons.
- Real content in every example: crate names, ticket keys (`task-2230`), file names, timings. Never lorem ipsum.
- Kickers and panel titles are short uppercase mono labels: `PROMPT`, `CONTROLS`, `TODOS · 1/3`.

## Colour

- Lay a screen on `surface-1`. Raise a card or panel to `surface-2`, a modal or menu to `surface-3`. Sink a well, a track or a field into `surface-sunken` or `surface-0`.
- Words: `ink-900` for headings and anything chosen, `ink-700` for body text, `ink-500` for secondary text and kickers, `ink-400` for placeholders. `ink-300` and `ink-200` are for disabled marks and decoration only, never for words a person must read: they miss 4.5:1 on `surface-1` in the neumorphic themes, as in the reference.
- `accent-blue` is the one action colour: a hovered secondary control, the focus ring, a chosen menu row, a link. Use it sparingly; a screen that is mostly blue has no action colour left.
- A gradient fill is for the one primary action on a screen: `grad-primary` for create, `grad-mint` for save. Danger is coral ink on an ordinary surface (`accent-coral`), never a red fill.
- Colour is light. In the instrument controls a lamp, a lit key word or a segment is the colour; a control is never a block of colour and never carries a coloured stripe or side bar.
- Hover is a wash (`hover`, `accent-wash`), never a new shadow. Only a press changes depth.
- Unluminous's own window parts use the `ul-*` tokens: `ul-editor` behind the text, `ul-title-bar`, `ul-explorer`, `ul-status-bar`, `ul-accent` for anything switched on, `ul-selected-row` for the chosen row's pill. Code is coloured by the nine `ul-syntax-*` tokens. The four `ul-highlight-*` marks are the same in every theme.

## Depth

Light comes from the top left. Every elevation is a pair: a lit shadow above and left in `shadow-light`, a shaded one below and right in `shadow-dark`.

| Elevation | Use |
|---|---|
| `e-raised-sm` | Small controls: a secondary button, a chip, a select, a key. |
| `e-raised` | The one raised button a section is about, a hovered select, a panel. |
| `e-raised-lg` | The largest surfaces: the nav bar, a page card, a menu in the neumorphic themes. |
| `e-pressed-sm`, `e-pressed`, `e-pressed-deep` | A held control, a field and a track, the deepest well (a prompt box). |
| `e-primary`, `e-primary-sm`, `e-primary-pressed` | The primary gradient button, its smaller form, and held. |
| `e-modal` | A dialog over the scrim: a downward, layered shadow with a lit top edge, never the paired glow, which bleeds on a dark scrim. |
| `e-menu` | An open dropdown. In the Unluminous themes it is the small raised shadow, because the large one spread a dark blur over the messages under a menu. |

A press moves a control down half a pixel and swaps its raised elevation for the pressed one. A column of things of one kind (chat bubbles, cards in a lane) uses one elevation for all of them.

## Shape

- Every button, the nav bar, a switch and a chip is a pill (`r-pill`). The dashed add button is the one button that is not, at `r-md`.
- Fields, selects and tiles take `r-md` (12px); panels and cards `r-lg` (18px); large cards and a modal `r-xl` (24px).
- Unluminous's own window: the window corner is `ul-window-corner` (12px), a control `ul-control-corner` (6px), a row's pill `ul-row-pill` (5px), a modal and a list card `ul-card` (10px).
- Unluminous's measurements are closed: a list row is `ul-size-row` (28px), a menu row `ul-size-menu-row` (24px), and there is no third row height. Bars: `ul-size-title-bar` 38, `ul-size-activity-bar` 36, `ul-size-status-bar` 32, `ul-size-tab-strip` 32.

## Type

- rux sets everything in Inter with JetBrains Mono for numbers and kickers, from the type styles: `rux-t-control` (12, medium) on a control, `rux-t-control-lg` on a larger one and a list row, `rux-t-button` on the primary button, `rux-t-prose` for running text in a well, `rux-t-caption` and `rux-t-panel-title` for uppercase mono kickers, `rux-t-number` for counts and times.
- Headings step `rux-t-h3` (18), `rux-t-h2` (24), `rux-t-h1` (32), `rux-t-display` (48), all semibold with tightened tracking. One display line a page at most.
- Inside Unluminous, rux controls are set in the window's interface face, not Inter: `font-ui` (Segoe UI on Windows, the system face on macOS) at the Unluminous scale: `ul-t-ordinary` 12.5 for a row, a menu item and a label; `ul-t-hint` 11.5; `ul-t-explorer-heading` 10.5 spaced capitals. Code is `font-code` (Consolas, Menlo).

## Iconography

- rux's marks are the 37 `Icon` names: 24 unit stroked SVG at 1.8 with round caps and joins, drawn in the current text colour. 12px on a small control, 13 on a medium one, 15 on the primary button. Pass `aria-label` when the icon is the only thing saying what a control does.
- Unluminous's window marks are `UlIcon`, the egui drawings exported exactly: folder, file, branch, realm, chat, board, database, terminal, run, bug and the rest, in two sets (`material`, the default, and `classic`). Each is about 10 points inside a 24 point cell at a 1.3 to 1.6 point stroke, in `ul-icon` at rest, `ul-icon-active` when its pane is open and `ul-accent` when it is on in a bar.
- Never a letter or a Unicode symbol as an icon, never emoji, never a picture that cannot be tinted. A new mark is drawn in the same weight as its neighbours and judged at the size a person sees it.
- The Icons and Unluminous Icons asset groups hold every mark as an SVG file in `#1E2530` ink.

## Components

- Mount the real components from `window.Rux` (`Rux.Button`, `Rux.Select`, `Rux.UlWindow` …), never a lookalike.
- A component decides nothing about the application: it draws, reports what happened through a callback, and takes its state as props (`value`, `open`, `chosen`) or keeps it itself when none is passed.
- Every component takes `className`, `style` and any `data-*` or `aria-*` prop on its root.
- One primary button to a screen. Secondary for everything else.
- The instrument controls (`Plate`, `Screen`, `Led`, `Readout`, `Key`, `Fader`, `Meter`, `Chart`, `Table`, `Timeline`, `Checkbox`) are the language of an agent's answer in Unluminous's chat: plates with a lit top edge, recessed screens, key caps that sink, LEDs and segmented meters.
- The Unluminous group (`UlWindow`, `UlTitleBar`, `UlActivityBar`, `UlExplorer`, `UlTabStrip`, `UlEditor`, `UlStatusBar`, `UlMenu`, `UlModal`, `UlTile`, `UlField`, `UlButton`, `UlIcon`) is the editor's own window, drawn as it is today. `UlWindow` composes the rest and takes any part as a prop, so a redesign can replace one piece at a time.

## States

- Hover: a wash, or ink turning `accent-blue`. Never a shadow change. A finger does not hover, so nothing may depend on it.
- Press: half a pixel down and the pressed elevation.
- Keyboard focus: a 2px `focus-ring` outside the control (rux), or a one point `ul-accent` ring inside a row (Unluminous, and only while that panel has the keyboard).
- Disabled: the control at 40% (`opacity: 0.4`), no hover. Absent rather than disabled when the control can never apply on this screen.
- Chosen: `ink-900` words and `accent-blue` marks, a raised segment in a pressed track, or the `ul-selected-row` pill.
