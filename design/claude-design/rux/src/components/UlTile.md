A docked panel: a 32 point header that is also its drag handle, and a body; the terminal tile by default.

Unluminous's own window component, not part of rux itself. Ported from `components::terminal_panel`, `components::dock::handle`; the measurements are `theme::size` and `design/style-guide.md`, and every colour is one of the `--ul-*` tokens, which follow the theme.

## Props
| Prop | Type | Notes |
|---|---|---|
| `title` | string | the panel's name |
| `count` | number | a quiet count after the name, as Realm and Agent-Tasks show |
| `tabs` | `{ name, active }[]` | the header's tabs |
| `terminal` | `{ text, tone, caret }[]` | the screen, when there are no children |
| `children` | node | a plugin pane's content in place of a terminal |
| `className`, `style`, others | | forwarded to the root |

## Rules
- The bottom of the window holds one tile a strip; two character grids are never stacked.
- The header is the handle: drag it to another edge to dock the panel there, double click it to fill the window.
